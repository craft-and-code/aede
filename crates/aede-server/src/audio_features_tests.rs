use super::*;

fn cached_features() -> aede_moods::Features {
    let mut extractor = aede_moods::Extractor::new(1).unwrap();
    extractor.push(&vec![0.; 4096], || Ok(())).unwrap();
    extractor.finish().unwrap()
}

#[test]
fn unknown_catalogue_precision_does_not_match_a_known_zero_acoustic_stamp() {
    let state = crate::test_support::sample_state();
    let mut guard = state.catalog.try_write().unwrap();
    let catalog = guard.as_mut().unwrap();
    let file = &catalog.files[0];
    let id = file.id;
    let path = file.path.clone();
    let mut facts = Facts::default();
    facts.rows.insert(
        path.clone(),
        Record {
            input: InputIdentity {
                path: path.clone(),
                size: file.size,
                modified: file.mtime,
                nanos: 0,
            },
            features: cached_features(),
        },
    );
    catalog.file_mtime_subseconds.remove(&path);
    assert!(
        current(&facts, catalog, id).is_none(),
        "an unknown scanned fraction cannot establish an exact cache identity"
    );
    catalog.file_mtime_subseconds.insert(path.clone(), 0);
    assert!(current(&facts, catalog, id).is_some());
    catalog.file_mtime_subseconds.insert(path, 1);
    assert!(current(&facts, catalog, id).is_none());
}

#[test]
fn unknown_catalogue_precision_is_unavailable_without_discarding_other_acoustic_work() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::playback_test_support::{additional_flac, install_wav};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_wav(&fixture, 4096).await;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&first.path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            )
            .unwrap();
        {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            catalog.files[0].mtime = 1_700_000_000;
            catalog
                .file_mtime_subseconds
                .insert(first.path.to_str().unwrap().into(), 0);
        }
        additional_flac(&fixture, "known-precision.flac", false).await;
        let first_record = {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            let features = cached_features();
            let records = catalog
                .files
                .iter()
                .map(|file| Record {
                    input: InputIdentity {
                        path: file.path.clone(),
                        size: file.size,
                        modified: file.mtime,
                        nanos: catalog.file_mtime_subseconds[&file.path],
                    },
                    features: features.clone(),
                })
                .collect::<Vec<_>>();
            for record in &records {
                aede_core::audio_features::save(&fixture.0.data_dir, record).unwrap();
            }
            catalog
                .file_mtime_subseconds
                .remove(first.path.to_str().unwrap());
            store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
            records.into_iter().next().unwrap()
        };
        let before = std::fs::read(&first.path).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "operator");
        let (code, task, _) = http(
            address,
            "POST",
            "/api/admin/v1/mood-features",
            Some(&token),
            "{}",
        );
        assert_eq!(code, 202, "{task}");
        let id = task["task_id"].as_u64().unwrap();
        let mut outcome = serde_json::Value::Null;
        for _ in 0..200 {
            outcome = http(
                address,
                "GET",
                &format!("/api/admin/v1/tasks/{id}"),
                Some(&token),
                "",
            )
            .1;
            if matches!(outcome["status"].as_str(), Some("completed" | "failed")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        server.abort();
        assert_eq!(outcome["status"], "failed", "{outcome}");
        assert_eq!(outcome["progress"]["total"], 2);
        assert_eq!(outcome["progress"]["done"], 2);
        assert_eq!(outcome["progress"]["failed"], 1);
        assert_eq!(outcome["progress"]["reused"], 1);
        assert!(
            outcome["result"]["stderr"]
                .as_str()
                .unwrap()
                .contains("rescan")
        );
        assert_eq!(std::fs::read(&first.path).unwrap(), before);
        assert_eq!(
            aede_core::audio_features::load(&fixture.0.data_dir, &first_record.input).unwrap(),
            Some(first_record)
        );
    });
}

#[test]
fn a_supervised_catalogue_cannot_replace_the_shared_acoustic_cache_with_its_subset() {
    use crate::accounts_test_support::Fixture;
    use crate::playback_test_support::install_wav;
    crate::test_support::test_runtime().block_on(async {
        for preloaded in [true, false] {
            let fixture = Fixture::new();
            let installed = install_wav(&fixture, 4096).await;
            let (record, epoch, blocked) = {
                let guard = fixture.0.catalog.read().await;
                let catalog = guard.as_ref().unwrap();
                let reference = EntityRef::parse_token(&installed.reference).unwrap();
                let source = playback_api::source_from_catalog(catalog, &reference).unwrap();
                (
                    Record {
                        input: identity(&source),
                        features: aede_core::audio_features::extract(&installed.path, || Ok(()))
                            .unwrap(),
                    },
                    signature(catalog),
                    EntityRef::of(catalog, EntityKind::Artist, 0)
                        .unwrap()
                        .to_token(),
                )
            };
            aede_core::audio_features::save(&fixture.0.data_dir, &record).unwrap();
            if preloaded {
                let mut facts = fixture.0.audio_features.facts.lock().unwrap();
                facts.rows.insert(record.input.path.clone(), record.clone());
                facts.epoch = Some(epoch);
            }
            let owner = fixture.accounts().find("alice").unwrap().id.clone();
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
            warm(&scoped);
            tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.audio_features.warming.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let full = fixture.0.catalog.read().await;
            let restricted = scoped.catalog.read().await;
            let facts = fixture.0.audio_features.facts.lock().unwrap();
            assert_eq!(
                coverage(&fixture.0, full.as_ref().unwrap(), &facts)["ready"],
                1,
                "a restricted read must preserve shared source availability (preloaded={preloaded})"
            );
            assert_eq!(
                coverage(&scoped, restricted.as_ref().unwrap(), &facts)["ready"],
                0
            );
            assert_eq!(facts.epoch, Some(epoch));
            if preloaded {
                assert_eq!(
                    facts.revision, 0,
                    "unchanged shared evidence must not invalidate paging"
                );
            }
        }
    });
}

#[test]
fn foreground_activity_parks_extraction_and_stop_retires_a_paused_job() {
    let state = crate::test_support::sample_state();
    let cancel = AtomicBool::new(false);
    let mut shutdown = state.shutdown.subscribe();
    let permit = state.playback_slots.try_acquire().unwrap();
    let worker = std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            let mut control =
                worker::Control::new(&state, &cancel, &mut shutdown, None, Default::default());
            control.checkpoint()
        });
        std::thread::sleep(Duration::from_millis(30));
        cancel.store(true, Ordering::Release);
        handle.join().unwrap()
    });
    assert!(worker.is_err());
    drop(permit);
}
#[test]
fn sticky_shutdown_cannot_be_lost_before_a_background_worker_starts() {
    let state = crate::test_support::sample_state();
    state.audio_features.stop();
    let cancel = AtomicBool::new(false);
    let mut shutdown = state.shutdown.subscribe();
    let mut control =
        worker::Control::new(&state, &cancel, &mut shutdown, None, Default::default());
    assert!(control.checkpoint().is_err());
    assert!(control.checkpoint().is_err());
}

#[test]
fn feature_jobs_require_administration_and_foreground_leases_require_a_real_owner() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let operator = login(address, "operator");
        let bob = login(address, "bob");
        let auditor = login(address, "auditor");
        assert_eq!(
            http(address, "POST", "/api/admin/v1/mood-features", None, "{}").0,
            401
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/admin/v1/mood-features",
                Some(&bob),
                "{}"
            )
            .0,
            403
        );
        let priority = "/api/me/v1/background-priority";
        let body = r#"{"client":"11111111-1111-4111-8111-111111111111","active":true}"#;
        assert_eq!(http(address, "POST", priority, None, body).0, 401);
        assert_eq!(http(address, "POST", priority, Some(&alice), body).0, 200);
        assert_eq!(http(address, "POST", priority, Some(&auditor), body).0, 200);
        assert_eq!(fixture.0.audio_features.leases.lock().unwrap().len(), 2);
        assert_eq!(
            http(
                address,
                "POST",
                priority,
                Some(&bob),
                r#"{"client":"11111111-1111-4111-8111-111111111111","active":false}"#
            )
            .0,
            200
        );
        assert_eq!(fixture.0.audio_features.leases.lock().unwrap().len(), 2);
        assert_eq!(
            http(
                address,
                "POST",
                priority,
                Some(&alice),
                r#"{"client":"bad","active":true}"#
            )
            .0,
            400
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/admin/v1/mood-features",
                Some(&operator),
                r#"{"path":"/arbitrary/source.flac"}"#
            )
            .0,
            400
        );
        server.abort();
    });
}

#[test]
fn native_job_parks_then_persists_each_source_and_reuses_it_without_rewriting_audio() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = crate::playback_test_support::install_wav(&fixture, 16000).await;
        let before = std::fs::read(&track.path).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "operator");
        let permit = fixture.0.playback_slots.try_acquire().unwrap();
        let start = || {
            let (code, body, _) = http(
                address,
                "POST",
                "/api/admin/v1/mood-features",
                Some(&token),
                "{}",
            );
            assert_eq!(code, 202);
            body["task_id"].as_u64().unwrap()
        };
        let read = |id| {
            http(
                address,
                "GET",
                &format!("/api/admin/v1/tasks/{id}"),
                Some(&token),
                "",
            )
            .1
        };
        let id = start();
        for _ in 0..100 {
            if read(id)["progress"]["phase"] == "waiting_for_playback" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(read(id)["progress"]["phase"], "waiting_for_playback");
        assert_eq!(read(id)["progress"]["total"], 1);
        assert!(!fixture.0.data_dir.join("acoustic-v1").exists());
        assert_eq!(
            http(
                address,
                "POST",
                "/api/admin/v1/mood-features",
                Some(&token),
                "{}"
            )
            .0,
            409
        );
        drop(permit);
        for _ in 0..200 {
            if read(id)["status"] == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let first = read(id);
        assert_eq!(first["status"], "completed", "{first}");
        assert_eq!(first["progress"]["analyzed"], 1);
        let source = tokio::task::spawn_blocking({
            let state = fixture.0.clone();
            move || sources(&state).unwrap().1.remove(0).unwrap()
        })
        .await
        .unwrap();
        assert!(
            aede_core::audio_features::load(&fixture.0.data_dir, &identity(&source))
                .unwrap()
                .is_some()
        );
        let id = start();
        for _ in 0..200 {
            if read(id)["status"] == "completed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(read(id)["progress"]["reused"], 1);
        assert_eq!(std::fs::read(&track.path).unwrap(), before);
        let permit = fixture.0.playback_slots.try_acquire().unwrap();
        let id = start();
        assert_eq!(
            http(
                address,
                "POST",
                &format!("/api/admin/v1/tasks/{id}/cancel"),
                Some(&token),
                ""
            )
            .0,
            202
        );
        for _ in 0..200 {
            if read(id)["status"] == "cancelled" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(read(id)["status"], "cancelled");
        drop(permit);
        server.abort();
    });
}

#[test]
fn shared_descriptors_never_train_on_another_owner_and_legacy_reads_remain_unchanged() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{encoded, start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = crate::playback_test_support::install_wav(&fixture, 16_000).await;
        let features = aede_core::audio_features::extract(&installed.path, || Ok(())).unwrap();
        let refs = {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            for n in 1..=2 {
                let mut file = catalog.files[0].clone();
                file.id = n;
                let path = fixture.0.data_dir.join(format!("mood-source-{n}.wav"));
                std::fs::copy(&installed.path, &path).unwrap();
                let metadata = std::fs::metadata(&path).unwrap();
                file.path = path.to_str().unwrap().into();
                file.size = metadata.len();
                file.mtime = aede_core::clock::mtime_seconds(&metadata);
                catalog.files.push(file);
                let mut track = catalog.tracks[0].clone();
                track.id = n;
                track.file_id = n;
                catalog.tracks.push(track);
                catalog.releases[0].track_ids.push(n);
                catalog.recordings[0].track_ids.push(n);
            }
            let mut facts = fixture.0.audio_features.facts.lock().unwrap();
            for file in &catalog.files {
                let metadata = std::fs::metadata(&file.path).unwrap();
                let nanos = aede_core::clock::mtime_subseconds(&metadata);
                catalog
                    .file_mtime_subseconds
                    .insert(file.path.clone(), nanos);
                facts.rows.insert(
                    file.path.clone(),
                    Record {
                        input: InputIdentity {
                            path: file.path.clone(),
                            size: file.size,
                            modified: file.mtime,
                            nanos,
                        },
                        features: features.clone(),
                    },
                );
            }
            facts.epoch = Some(signature(catalog));
            store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
            catalog
                .tracks
                .iter()
                .map(|track| {
                    EntityRef::of(catalog, EntityKind::Track, track.id)
                        .unwrap()
                        .to_token()
                })
                .collect::<Vec<_>>()
        };
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        for reference in refs.iter().take(2) {
            let path = format!("/api/me/v1/annotation?ref={}", encoded(reference));
            assert_eq!(
                http(address, "PUT", &path, Some(&alice), r#"{"tags":["calm"]}"#).0,
                200
            );
        }
        let (_, legacy, _) = http(
            address,
            "GET",
            "/api/me/v1/moods?mood=calm",
            Some(&alice),
            "",
        );
        assert_eq!(legacy["total"], 2);
        assert!(legacy.get("acoustic").is_none());
        let (code, page, _) = http(
            address,
            "GET",
            "/api/me/v1/moods?audio=true&mood=calm",
            Some(&alice),
            "",
        );
        assert_eq!(code, 200, "{page}");
        assert_eq!(page["total"], 3);
        assert_eq!(page["items"][2]["reason"]["scope"], "acoustic");
        let (_, other, _) = http(
            address,
            "GET",
            "/api/me/v1/moods?audio=true&mood=calm",
            Some(&bob),
            "",
        );
        assert_eq!(other["total"], 0);
        assert_eq!(other["acoustic"]["ready"], 3);
        assert!(other["moods"][0]["model"].is_null());
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/moods?audio=true&mood=calm&revision=bad",
                Some(&alice),
                ""
            )
            .0,
            400
        );
        server.abort();
    });
}
