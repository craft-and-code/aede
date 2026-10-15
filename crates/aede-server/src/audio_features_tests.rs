use super::*;
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
            move || sources(&state).unwrap().1.remove(0)
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
        let refs = {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            for n in 1..=2 {
                let mut file = catalog.files[0].clone();
                file.id = n;
                file.path = format!("/music/album/{n}.flac");
                catalog.files.push(file);
                let mut track = catalog.tracks[0].clone();
                track.id = n;
                track.file_id = n;
                catalog.tracks.push(track);
                catalog.releases[0].track_ids.push(n);
            }
            let mut facts = fixture.0.audio_features.facts.lock().unwrap();
            facts.epoch = Some(signature(catalog));
            let mut extract = aede_moods::Extractor::new(1).unwrap();
            let samples = (0..44100)
                .map(|n| 0.2 * (std::f32::consts::TAU * 440. * n as f32 / 22050.).sin())
                .collect::<Vec<_>>();
            extract.push(&samples, || Ok(())).unwrap();
            let features = extract.finish().unwrap();
            for file in &catalog.files {
                facts.rows.insert(
                    file.path.clone(),
                    Record {
                        input: InputIdentity {
                            path: file.path.clone(),
                            size: file.size,
                            modified: file.mtime,
                            nanos: 0,
                        },
                        features: features.clone(),
                    },
                );
            }
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
