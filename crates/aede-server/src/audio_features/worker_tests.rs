use super::*;
use crate::accounts_test_support::Fixture;
use crate::playback_test_support::wav_bytes;
use crate::test_support::test_runtime;
use std::path::{Path, PathBuf};

struct PendingExtraction {
    id: u64,
    events: broadcast::Receiver<CatalogEvent>,
    cancellation: tokio::sync::oneshot::Receiver<Arc<AtomicBool>>,
    outcome: tokio::sync::oneshot::Receiver<Result<jobs::JobOutput, String>>,
}

struct ReservedWorker {
    release: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ReservedWorker {
    fn new(state: &ApiState) -> Self {
        let state = state.clone();
        let (ready, acquired) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _worker = state.audio_features.worker.lock().unwrap();
            ready.send(()).unwrap();
            released.recv().unwrap();
        });
        acquired.recv().unwrap();
        Self {
            release: Some(release),
            thread: Some(thread),
        }
    }
}

impl Drop for ReservedWorker {
    fn drop(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
        self.thread.take().unwrap().join().unwrap();
    }
}

impl PendingExtraction {
    fn start(state: &ApiState) -> Self {
        let id = state.next_task_id.load(Ordering::Acquire);
        let events = state.events.subscribe();
        let (send_cancel, cancellation) = tokio::sync::oneshot::channel();
        let (send_outcome, outcome) = tokio::sync::oneshot::channel();
        let response = jobs::enqueue_background(state.clone(), move |state, cancel, id| {
            send_cancel.send(cancel.clone()).unwrap();
            let outcome = run(state, cancel, id, false);
            send_outcome.send(outcome.clone()).unwrap();
            outcome
        })
        .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        Self {
            id,
            events,
            cancellation,
            outcome,
        }
    }

    async fn selected_sources(&mut self, count: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let CatalogEvent::TaskProgress {
                    task_id,
                    total,
                    phase: "",
                    ..
                } = self.events.recv().await.unwrap()
                    && task_id == self.id
                {
                    assert_eq!(total, count);
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    async fn finish(mut self) -> Result<jobs::JobOutput, String> {
        let outcome = tokio::time::timeout(Duration::from_secs(10), self.outcome)
            .await
            .unwrap()
            .unwrap();
        // A new task may be admitted only after the registry retires this one.
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match self.events.recv().await.unwrap() {
                    CatalogEvent::TaskCompleted { task_id, .. }
                    | CatalogEvent::TaskFailed { task_id, .. }
                        if task_id == self.id =>
                    {
                        break;
                    }
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        outcome
    }
}

fn wav_catalogue(fixture: &Fixture, name: &str, frames: u32) -> (Catalog, InputIdentity) {
    let path = fixture.0.data_dir.join(name);
    std::fs::write(&path, wav_bytes(frames)).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let mut catalog = crate::test_support::sample_state()
        .catalog
        .try_read()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    let file = &mut catalog.files[0];
    file.path = path.to_str().unwrap().into();
    file.size = metadata.len();
    file.mtime = aede_core::clock::mtime_seconds(&metadata);
    let input = InputIdentity {
        path: file.path.clone(),
        size: file.size,
        modified: file.mtime,
        nanos: aede_core::clock::mtime_subseconds(&metadata),
    };
    catalog
        .file_mtime_subseconds
        .insert(input.path.clone(), input.nanos);
    (catalog, input)
}

async fn activate(state: &ApiState, catalog: &Catalog) {
    *state.catalog.write().await = Some(catalog.clone());
}

fn cache_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let directory = root.join(format!("acoustic-v{}", aede_moods::PROFILE));
    if !directory.exists() {
        return BTreeMap::new();
    }
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

fn assert_active_facts(state: &ApiState, catalog: &Catalog, inputs: &[&InputIdentity]) {
    let facts = state.audio_features.facts.lock().unwrap();
    let actual = facts.rows.keys().map(PathBuf::from).collect::<Vec<_>>();
    let mut expected = inputs
        .iter()
        .map(|input| PathBuf::from(&input.path))
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        actual, expected,
        "resident facts must follow the active catalogue"
    );
    assert!(facts.rows.len() <= catalog.files.len());
    assert_eq!(facts.epoch, Some(signature(catalog)));
    for input in inputs {
        assert_eq!(facts.rows[&input.path].input, **input);
    }
}

async fn finish_warming(state: &ApiState) {
    warm(state);
    tokio::time::timeout(Duration::from_secs(10), async {
        while state.audio_features.warming.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn disjoint_catalogues_bound_resident_facts_and_preserve_reusable_durable_records() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let catalogues = [
            wav_catalogue(&fixture, "first.wav", 4096),
            wav_catalogue(&fixture, "second.wav", 6144),
            wav_catalogue(&fixture, "third.wav", 8192),
        ];
        let original_audio = catalogues
            .iter()
            .map(|(_, input)| {
                let path = PathBuf::from(&input.path);
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<BTreeMap<_, _>>();
        let mut durable = BTreeMap::new();
        for (catalog, input) in &catalogues {
            activate(&fixture.0, catalog).await;
            let output = PendingExtraction::start(&fixture.0).finish().await.unwrap();
            assert_eq!(output.exit_code, 0, "{output:?}");
            assert_eq!(output.stdout, "1 analyzed, 0 reused, 0 unavailable");
            assert_active_facts(&fixture.0, catalog, &[input]);
            let after = cache_bytes(&fixture.0.data_dir);
            for (path, bytes) in &durable {
                assert_eq!(after.get(path), Some(bytes));
            }
            assert_eq!(after.len(), durable.len() + 1);
            durable = after;
            let revision = fixture.0.audio_features.facts.lock().unwrap().revision;
            let reused = PendingExtraction::start(&fixture.0).finish().await.unwrap();
            assert_eq!(reused.stdout, "0 analyzed, 1 reused, 0 unavailable");
            finish_warming(&fixture.0).await;
            assert_active_facts(&fixture.0, catalog, &[input]);
            assert_eq!(
                fixture.0.audio_features.facts.lock().unwrap().revision,
                revision
            );
            assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
        }
        let (catalog, input) = &catalogues[0];
        activate(&fixture.0, catalog).await;
        let reused = PendingExtraction::start(&fixture.0).finish().await.unwrap();
        assert_eq!(reused.stdout, "0 analyzed, 1 reused, 0 unavailable");
        assert_active_facts(&fixture.0, catalog, &[input]);
        let mut restarted = fixture.0.clone();
        restarted.audio_features = Arc::new(FeatureState::default());
        finish_warming(&restarted).await;
        assert_active_facts(&restarted, catalog, &[input]);
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
        for (path, bytes) in original_audio {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    });
}

#[test]
fn unavailable_replacement_catalogue_does_not_keep_historical_resident_facts() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (first, first_input) = wav_catalogue(&fixture, "completed.wav", 4096);
        activate(&fixture.0, &first).await;
        assert_eq!(
            PendingExtraction::start(&fixture.0)
                .finish()
                .await
                .unwrap()
                .exit_code,
            0
        );
        let durable = cache_bytes(&fixture.0.data_dir);
        let (missing, missing_input) = wav_catalogue(&fixture, "unavailable.wav", 4096);
        std::fs::remove_file(&missing_input.path).unwrap();
        activate(&fixture.0, &missing).await;
        let output = PendingExtraction::start(&fixture.0).finish().await.unwrap();
        assert_eq!(output.exit_code, 1);
        assert_eq!(output.stdout, "0 analyzed, 0 reused, 1 unavailable");
        assert_active_facts(&fixture.0, &missing, &[]);
        finish_warming(&fixture.0).await;
        assert_active_facts(&fixture.0, &missing, &[]);
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
        activate(&fixture.0, &first).await;
        let resumed = PendingExtraction::start(&fixture.0).finish().await.unwrap();
        assert_eq!(resumed.stdout, "0 analyzed, 1 reused, 0 unavailable");
        assert_active_facts(&fixture.0, &first, &[&first_input]);
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
    });
}

#[test]
fn catalogue_reload_while_worker_waits_cannot_publish_obsolete_cached_sources_or_epoch() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (first, _) = wav_catalogue(&fixture, "first.wav", 4096);
        activate(&fixture.0, &first).await;
        PendingExtraction::start(&fixture.0).finish().await.unwrap();
        let (queued, queued_input) = wav_catalogue(&fixture, "queued.wav", 4096);
        let cached = Record {
            input: queued_input.clone(),
            features: aede_core::audio_features::extract(Path::new(&queued_input.path), || Ok(()))
                .unwrap(),
        };
        aede_core::audio_features::save(&fixture.0.data_dir, &cached).unwrap();
        let durable = cache_bytes(&fixture.0.data_dir);
        let (replacement, replacement_input) = wav_catalogue(&fixture, "replacement.wav", 4096);
        activate(&fixture.0, &queued).await;
        let worker = ReservedWorker::new(&fixture.0);
        let mut pending = PendingExtraction::start(&fixture.0);
        pending.selected_sources(1).await;
        activate(&fixture.0, &replacement).await;
        drop(worker);
        let _outcome = pending.finish().await;
        {
            let facts = fixture.0.audio_features.facts.lock().unwrap();
            assert!(
                facts
                    .rows
                    .keys()
                    .all(|path| path == &replacement_input.path)
            );
            assert_ne!(facts.epoch, Some(signature(&queued)));
            assert!(facts.epoch.is_none() || facts.epoch == Some(signature(&replacement)));
        }
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
        assert_eq!(
            aede_core::audio_features::load(&fixture.0.data_dir, &queued_input).unwrap(),
            Some(cached)
        );
        PendingExtraction::start(&fixture.0).finish().await.unwrap();
        assert_active_facts(&fixture.0, &replacement, &[&replacement_input]);
    });
}

#[test]
fn cancelled_queued_extraction_keeps_completed_cache_reusable_after_restart() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (first, _) = wav_catalogue(&fixture, "first.wav", 4096);
        activate(&fixture.0, &first).await;
        PendingExtraction::start(&fixture.0).finish().await.unwrap();
        let (queued, queued_input) = wav_catalogue(&fixture, "queued.wav", 4096);
        let cached = Record {
            input: queued_input.clone(),
            features: aede_core::audio_features::extract(Path::new(&queued_input.path), || Ok(()))
                .unwrap(),
        };
        aede_core::audio_features::save(&fixture.0.data_dir, &cached).unwrap();
        let durable = cache_bytes(&fixture.0.data_dir);
        activate(&fixture.0, &queued).await;
        let worker = ReservedWorker::new(&fixture.0);
        let mut pending = PendingExtraction::start(&fixture.0);
        pending.selected_sources(1).await;
        (&mut pending.cancellation)
            .await
            .unwrap()
            .store(true, Ordering::Release);
        drop(worker);
        assert_eq!(pending.finish().await.unwrap().exit_code, 130);
        {
            let facts = fixture.0.audio_features.facts.lock().unwrap();
            assert!(!facts.rows.contains_key(&queued_input.path));
            assert_ne!(facts.epoch, Some(signature(&queued)));
        }
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
        let resumed = PendingExtraction::start(&fixture.0).finish().await.unwrap();
        assert_eq!(resumed.stdout, "0 analyzed, 1 reused, 0 unavailable");
        assert_active_facts(&fixture.0, &queued, &[&queued_input]);
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
    });
}

#[test]
fn a_supervised_worker_keeps_the_shared_catalogue_as_its_resident_fact_boundary() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (catalog, input) = wav_catalogue(&fixture, "shared.wav", 4096);
        activate(&fixture.0, &catalog).await;
        PendingExtraction::start(&fixture.0).finish().await.unwrap();
        let durable = cache_bytes(&fixture.0.data_dir);
        let owner = fixture.accounts().find("alice").unwrap().id.clone();
        let blocked = EntityRef::of(&catalog, EntityKind::Artist, 0)
            .unwrap()
            .to_token();
        let mut policies = aede_core::parental::ParentalStore::default();
        policies
            .set_policy(
                &owner,
                serde_json::from_value(crate::parental_test_support::policy(vec![&blocked]))
                    .unwrap(),
                0,
            )
            .unwrap();
        policies
            .save(&aede_core::parental::path(&fixture.0.data_dir))
            .unwrap();
        let scoped = crate::parental::scoped(fixture.0.clone(), owner, false, true)
            .await
            .unwrap();
        assert!(
            scoped
                .catalog
                .read()
                .await
                .as_ref()
                .unwrap()
                .files
                .is_empty()
        );
        let reused = PendingExtraction::start(&scoped).finish().await.unwrap();
        assert_eq!(reused.stdout, "0 analyzed, 1 reused, 0 unavailable");
        assert_active_facts(&fixture.0, &catalog, &[&input]);
        assert_eq!(cache_bytes(&fixture.0.data_dir), durable);
    });
}
