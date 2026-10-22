use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::playback_test_support::{install_wav, wav_bytes};
use crate::test_support::{encoded, start_server, test_runtime};
use std::net::SocketAddr;

#[derive(Clone, Copy)]
enum SourceChange {
    Size,
    Nanoseconds,
    Missing,
}

fn stamp(path: &Path, seconds: u64, nanos: u32) {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + Duration::new(seconds, nanos)),
        )
        .unwrap();
}

async fn measured_source(fixture: &Fixture) -> Record {
    let installed = install_wav(fixture, 16_000).await;
    stamp(&installed.path, 1_700_000_000, 123_456_789);
    let metadata = std::fs::metadata(&installed.path).unwrap();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().unwrap();
    let file = &mut catalog.files[0];
    file.mtime = aede_core::clock::mtime_seconds(&metadata);
    let path = file.path.clone();
    catalog
        .file_mtime_subseconds
        .insert(path.clone(), aede_core::clock::mtime_subseconds(&metadata));
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
    let record = Record {
        input: InputIdentity {
            path,
            size: metadata.len(),
            modified: aede_core::clock::mtime_seconds(&metadata),
            nanos: aede_core::clock::mtime_subseconds(&metadata),
        },
        features: aede_core::audio_features::extract(&installed.path, || Ok(())).unwrap(),
    };
    assert!(record.features.usable());
    aede_core::audio_features::save(&fixture.0.data_dir, &record).unwrap();
    record
}

async fn settle_warming(state: &ApiState) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.audio_features.warming.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("bounded acoustic cache warming");
}

fn cache_bytes(fixture: &Fixture) -> BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(
        fixture
            .0
            .data_dir
            .join(format!("acoustic-v{}", aede_moods::PROFILE)),
    )
    .unwrap()
    .map(|entry| {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect()
}

fn page(address: SocketAddr, token: &str, query: &str) -> (u16, serde_json::Value) {
    let (status, value, _) = http(
        address,
        "GET",
        &format!("/api/me/v1/moods?audio=true&{query}"),
        Some(token),
        "",
    );
    (status, value)
}

async fn source_change_refuses_reuse(change: SourceChange, cold: bool) {
    let mut fixture = Fixture::new();
    let record = measured_source(&fixture).await;
    let path = Path::new(&record.input.path);
    let recipe = aede_moods::audio_recipes()
        .into_iter()
        .find(|recipe| recipe.measure(&record.features).is_some())
        .expect("the real WAV has a measured descriptor selection");
    let query = format!("mood={}", encoded(recipe.id));
    let saved_cache = cache_bytes(&fixture);
    let catalog_path = store::catalog_path(&fixture.0.data_dir);
    let saved_catalog = std::fs::read(&catalog_path).unwrap();
    warm(&fixture.0);
    settle_warming(&fixture.0).await;
    let (mut address, mut server) = start_server(fixture.0.clone()).await;
    let mut token = login(address, "alice");
    let (status, baseline) = page(address, &token, &query);
    assert_eq!(status, 200, "{baseline}");
    assert_eq!(baseline["acoustic"]["ready"], 1, "{baseline}");
    assert_eq!(baseline["total"], 1, "{baseline}");
    assert_eq!(
        baseline["items"][0]["reason"]["audio"]["kind"],
        "descriptor"
    );
    let revision = baseline["selection_revision"].as_str().unwrap();
    settle_warming(&fixture.0).await;
    if cold {
        server.abort();
        let _ = (&mut server).await;
        fixture.0.audio_features = Arc::new(FeatureState::default());
    }
    match change {
        SourceChange::Size => {
            std::fs::write(path, wav_bytes(16_001)).unwrap();
            stamp(path, record.input.modified, record.input.nanos);
            let metadata = std::fs::metadata(path).unwrap();
            assert_ne!(metadata.len(), record.input.size);
            assert_eq!(
                aede_core::clock::mtime_seconds(&metadata),
                record.input.modified
            );
            assert_eq!(
                aede_core::clock::mtime_subseconds(&metadata),
                record.input.nanos
            );
        }
        SourceChange::Nanoseconds => {
            stamp(path, record.input.modified, 987_654_321);
            let metadata = std::fs::metadata(path).unwrap();
            assert_eq!(metadata.len(), record.input.size);
            assert_eq!(
                aede_core::clock::mtime_seconds(&metadata),
                record.input.modified
            );
            assert_ne!(
                aede_core::clock::mtime_subseconds(&metadata),
                record.input.nanos
            );
        }
        SourceChange::Missing => std::fs::remove_file(path).unwrap(),
    }
    let source_after_change = std::fs::read(path).ok();
    if cold {
        warm(&fixture.0);
        settle_warming(&fixture.0).await;
        assert!(
            fixture
                .0
                .audio_features
                .facts
                .lock()
                .unwrap()
                .rows
                .is_empty(),
            "cold warming must not retain a cache for a changed or missing original"
        );
        (address, server) = start_server(fixture.0.clone()).await;
        token = login(address, "alice");
    }
    let (status, changed) = page(address, &token, &query);
    assert_eq!(status, 200, "{changed}");
    assert_eq!(changed["acoustic"]["ready"], 0, "{changed}");
    assert_eq!(changed["acoustic"]["missing"], 1, "{changed}");
    assert_eq!(changed["total"], 0, "{changed}");
    assert!(changed["items"].as_array().unwrap().is_empty());
    assert_ne!(
        changed["selection_revision"],
        baseline["selection_revision"]
    );
    let (status, refused) = page(address, &token, &format!("{query}&revision={revision}"));
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["error"]["code"], "selection_changed", "{refused}");
    settle_warming(&fixture.0).await;
    assert_eq!(std::fs::read(path).ok(), source_after_change);
    assert_eq!(std::fs::read(catalog_path).unwrap(), saved_catalog);
    assert_eq!(cache_bytes(&fixture), saved_cache);
    assert_eq!(
        aede_core::audio_features::load(&fixture.0.data_dir, &record.input).unwrap(),
        Some(record),
        "missing evidence must retain the completed original cache bytes"
    );
    server.abort();
}

#[test]
fn hot_size_replacement_invalidates_descriptor_and_paging_without_rescan() {
    test_runtime().block_on(source_change_refuses_reuse(SourceChange::Size, false));
}

#[test]
fn hot_nanosecond_change_invalidates_descriptor_and_paging_without_rescan() {
    test_runtime().block_on(source_change_refuses_reuse(
        SourceChange::Nanoseconds,
        false,
    ));
}

#[test]
fn hot_missing_source_invalidates_descriptor_and_paging_without_rescan() {
    test_runtime().block_on(source_change_refuses_reuse(SourceChange::Missing, false));
}

#[test]
fn cold_size_replacement_cannot_warm_a_stale_cached_descriptor() {
    test_runtime().block_on(source_change_refuses_reuse(SourceChange::Size, true));
}

#[test]
fn cold_nanosecond_change_cannot_warm_a_stale_cached_descriptor() {
    test_runtime().block_on(source_change_refuses_reuse(SourceChange::Nanoseconds, true));
}

#[test]
fn cold_missing_source_cannot_warm_a_stale_cached_descriptor() {
    test_runtime().block_on(source_change_refuses_reuse(SourceChange::Missing, true));
}

#[test]
fn warming_revalidates_an_unchanged_catalogue_and_can_reuse_a_restored_original() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let record = measured_source(&fixture).await;
        let saved_source = std::fs::read(&record.input.path).unwrap();
        let saved_cache = cache_bytes(&fixture);
        warm(&fixture.0);
        settle_warming(&fixture.0).await;
        let (epoch, revision) = {
            let facts = fixture.0.audio_features.facts.lock().unwrap();
            assert_eq!(facts.rows.len(), 1);
            (facts.epoch, facts.revision)
        };
        std::fs::remove_file(&record.input.path).unwrap();
        warm(&fixture.0);
        settle_warming(&fixture.0).await;
        {
            let facts = fixture.0.audio_features.facts.lock().unwrap();
            assert!(
                facts.rows.is_empty(),
                "warming must itself retire stale facts"
            );
            assert_eq!(facts.epoch, epoch, "the catalogue has not been rescanned");
            assert!(facts.revision > revision);
        }
        std::fs::write(&record.input.path, &saved_source).unwrap();
        stamp(
            Path::new(&record.input.path),
            record.input.modified,
            record.input.nanos,
        );
        warm(&fixture.0);
        settle_warming(&fixture.0).await;
        assert_eq!(
            fixture
                .0
                .audio_features
                .facts
                .lock()
                .unwrap()
                .rows
                .get(&record.input.path),
            Some(&record)
        );
        assert_eq!(std::fs::read(&record.input.path).unwrap(), saved_source);
        assert_eq!(cache_bytes(&fixture), saved_cache);
    });
}

#[test]
fn unchanged_real_source_reuses_persisted_features_and_preserves_paging_and_bytes() {
    test_runtime().block_on(async {
        let mut fixture = Fixture::new();
        let record = measured_source(&fixture).await;
        let saved_source = std::fs::read(&record.input.path).unwrap();
        let saved_cache = cache_bytes(&fixture);
        let catalog_path = store::catalog_path(&fixture.0.data_dir);
        let saved_catalog = std::fs::read(&catalog_path).unwrap();
        let mut revision = None;
        for _ in 0..2 {
            warm(&fixture.0);
            settle_warming(&fixture.0).await;
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let (status, baseline) = page(address, &token, "");
            assert_eq!(status, 200, "{baseline}");
            assert_eq!(baseline["acoustic"]["ready"], 1);
            let current = baseline["selection_revision"].as_str().unwrap();
            if let Some(previous) = revision.as_deref() {
                assert_eq!(current, previous);
            }
            let (status, repeated) = page(address, &token, &format!("revision={current}"));
            assert_eq!(status, 200, "{repeated}");
            assert_eq!(repeated["acoustic"]["ready"], 1);
            assert_eq!(
                repeated["selection_revision"],
                baseline["selection_revision"]
            );
            revision = Some(current.to_owned());
            settle_warming(&fixture.0).await;
            assert_eq!(std::fs::read(&record.input.path).unwrap(), saved_source);
            assert_eq!(cache_bytes(&fixture), saved_cache);
            assert_eq!(std::fs::read(&catalog_path).unwrap(), saved_catalog);
            server.abort();
            let _ = server.await;
            fixture.0.audio_features = Arc::new(FeatureState::default());
        }
    });
}
