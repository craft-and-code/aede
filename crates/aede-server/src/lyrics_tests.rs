use super::*;
use crate::accounts_test_support::{ADMIN_TOKEN, Fixture, http, login};
use crate::playback_test_support::install_wav;
use crate::test_support::{encoded, start_server, test_runtime};
use aede_core::accounts;

async fn set_words(fixture: &Fixture, tag: Option<&str>, sidecar: Option<&Path>) {
    let mut guard = fixture.0.catalog.write().await;
    let file = &mut guard.as_mut().unwrap().files[0];
    file.tags.remove("lyrics");
    if let Some(tag) = tag {
        file.tags.insert("lyrics".into(), vec![tag.into()]);
    }
    file.lyrics_path = sidecar.map(|path| path.to_string_lossy().into_owned());
}

fn path(reference: &str) -> String {
    format!("/api/v1/lyrics?track={}", encoded(reference))
}

fn failure(address: SocketAddr, path: &str, token: Option<&str>, status: u16, code: &str) {
    let (actual, body, _) = http(address, "GET", path, token, "");
    assert_eq!(actual, status, "{body}");
    assert_eq!(body["error"]["code"], code, "{body}");
}

#[test]
fn native_lyrics_are_complete_own_clock_data_and_do_not_expose_a_filesystem_origin() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        std::fs::remove_file(accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        let track = install_wav(&fixture, 8000).await;
        set_words(
            &fixture,
            Some("[offset:250]\n[00:03]third\n[00:01][00:02]chorus\n[00:04]\n"),
            None,
        )
        .await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let (status, body, headers) = http(address, "GET", &path(&track.reference), None, "");
        assert_eq!(status, 200, "{body}");
        assert!(headers.contains("application/json"));
        assert!(headers.contains("no-store"));
        assert_eq!(body["track"], track.reference);
        assert_eq!(body["lyrics"]["source"], "tag");
        assert_eq!(body["lyrics"]["synced"], true);
        assert_eq!(
            body["lyrics"]["lines"],
            serde_json::json!([
                {"at_ms":3250,"text":"third"},
                {"at_ms":1250,"text":"chorus"},
                {"at_ms":2250,"text":"chorus"},
                {"at_ms":4250,"text":""}
            ])
        );
        assert!(body["lyrics"].get("origin").is_none());
        assert_eq!(body["lyrics"].as_object().unwrap().len(), 3);
        server.abort();
    });
}

#[test]
fn native_lyrics_allow_all_read_roles_and_protect_the_route_when_accounts_exist() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        failure(address, &path(&track.reference), None, 401, "unauthorized");
        for username in ["operator", "alice", "auditor"] {
            let token = login(address, username);
            let (status, body, _) = http(address, "GET", &path(&track.reference), Some(&token), "");
            assert_eq!(status, 200, "{body}");
            assert_eq!(body["track"], track.reference);
            assert!(body["lyrics"].is_null());
        }
        assert_eq!(
            http(
                address,
                "GET",
                &path(&track.reference),
                Some(ADMIN_TOKEN),
                ""
            )
            .0,
            200
        );
        server.abort();
    });
}

#[test]
fn native_lyrics_refuse_unknown_duplicate_missing_and_non_track_parameters() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let valid = path(&track.reference);
        for invalid in [
            "/api/v1/lyrics".to_string(),
            "/api/v1/lyrics?track=".into(),
            "/api/v1/lyrics?track=artist:ac%2Fdc".into(),
            "/api/v1/lyrics?track=not-a-reference".into(),
            format!("{valid}&track={}", encoded(&track.reference)),
            format!("{valid}&offset=0"),
            format!("{valid}&ignored=true"),
        ] {
            failure(address, &invalid, Some(&token), 400, "invalid_query");
        }
        failure(
            address,
            "/api/v1/lyrics?track=track:missing",
            Some(&token),
            404,
            "entity_not_found",
        );
        assert_eq!(http(address, "HEAD", &valid, Some(&token), "").0, 200);
        let (status, body, _) = http(address, "POST", &valid, Some(&token), "{}");
        assert_eq!(status, 405, "{body}");
        assert_eq!(body["error"]["code"], "method_not_allowed");
        server.abort();
    });
}

#[test]
fn native_lyrics_read_current_adjacent_sidecars_with_tag_precedence_and_explicit_absence() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let sidecar = track.path.with_extension("LRC");
        std::fs::write(&sidecar, b"first\n\nsecond\xff").unwrap();
        set_words(&fixture, None, Some(&sidecar)).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let (_, body, _) = http(address, "GET", &path(&track.reference), Some(&token), "");
        assert_eq!(body["lyrics"]["source"], "sidecar");
        assert_eq!(body["lyrics"]["synced"], false);
        assert_eq!(
            body["lyrics"]["lines"],
            serde_json::json!([
                {"at_ms":null,"text":"first"},
                {"at_ms":null,"text":""},
                {"at_ms":null,"text":"second\u{fffd}"}
            ])
        );
        set_words(&fixture, Some("[00:01]tag words"), Some(&sidecar)).await;
        let (_, body, _) = http(address, "GET", &path(&track.reference), Some(&token), "");
        assert_eq!(body["lyrics"]["source"], "tag");
        assert_eq!(body["lyrics"]["lines"][0]["text"], "tag words");
        set_words(&fixture, Some("[ar:metadata]"), Some(&sidecar)).await;
        std::fs::write(&sidecar, "[ar:metadata]\n\n").unwrap();
        let (_, body, _) = http(address, "GET", &path(&track.reference), Some(&token), "");
        assert!(body["lyrics"].is_null());
        std::fs::remove_file(&sidecar).unwrap();
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            404,
            "lyrics_unavailable",
        );
        server.abort();
    });
}

#[test]
fn native_lyrics_reject_stale_audio_and_legacy_imprecise_catalogs() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        set_words(&fixture, Some("words"), None).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let expected = {
            let mut guard = fixture.0.catalog.write().await;
            guard
                .as_mut()
                .unwrap()
                .file_mtime_subseconds
                .remove(&track.path.to_string_lossy().into_owned())
                .unwrap()
        };
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            409,
            "source_changed",
        );
        fixture
            .0
            .catalog
            .write()
            .await
            .as_mut()
            .unwrap()
            .file_mtime_subseconds
            .insert(
                track.path.to_string_lossy().into_owned(),
                expected.wrapping_add(1),
            );
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            409,
            "source_changed",
        );
        std::fs::remove_file(&track.path).unwrap();
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            404,
            "source_unavailable",
        );
        server.abort();
    });
}

#[test]
fn native_lyrics_reject_non_adjacent_or_non_regular_sidecars() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let other = fixture.0.data_dir.join("private.lrc");
        std::fs::write(&other, "private words").unwrap();
        set_words(&fixture, None, Some(&other)).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            409,
            "lyrics_changed",
        );
        let sidecar = track.path.with_extension("lrc");
        std::fs::create_dir(&sidecar).unwrap();
        set_words(&fixture, None, Some(&sidecar)).await;
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            409,
            "lyrics_changed",
        );
        #[cfg(unix)]
        {
            std::fs::remove_dir(&sidecar).unwrap();
            std::os::unix::fs::symlink(&other, &sidecar).unwrap();
            failure(
                address,
                &path(&track.reference),
                Some(&token),
                409,
                "lyrics_changed",
            );
        }
        server.abort();
    });
}

#[test]
fn native_lyrics_bound_input_timestamp_expansion_and_json_overhead() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        for words in [
            "x".repeat(MAX_INPUT_BYTES + 1),
            format!("{}{}", "[00:01]".repeat(1000), "x".repeat(2000)),
            "[0:0]".repeat(52_000),
        ] {
            set_words(&fixture, Some(&words), None).await;
            failure(
                address,
                &path(&track.reference),
                Some(&token),
                413,
                "lyrics_too_large",
            );
        }
        let sidecar = track.path.with_extension("lrc");
        std::fs::write(&sidecar, "x".repeat(MAX_INPUT_BYTES + 1)).unwrap();
        set_words(&fixture, None, Some(&sidecar)).await;
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            413,
            "lyrics_too_large",
        );
        server.abort();
    });
}

#[test]
fn native_lyrics_share_bounded_workers_and_recheck_revocation_and_account_activation() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 8000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let identity = auth::SocketIdentity {
            session: Some(
                auth::principal(
                    &fixture.0,
                    &fixture.accounts(),
                    &token,
                    false,
                    Instant::now(),
                )
                .unwrap(),
            ),
            administrative: false,
        };
        assert!(confirm_access(&fixture.0, &identity).is_ok());
        let held = fixture.0.inspection_slots.try_acquire_many(2).unwrap();
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            429,
            "inspection_busy",
        );
        assert_eq!(
            http(address, "GET", "/api/v1/status", Some(&token), "").0,
            200
        );
        drop(held);
        assert_eq!(
            http(address, "DELETE", "/api/auth/v1/session", Some(&token), "").0,
            204
        );
        assert_eq!(
            confirm_access(&fixture.0, &identity).unwrap_err().status,
            StatusCode::UNAUTHORIZED
        );
        failure(
            address,
            &path(&track.reference),
            Some(&token),
            401,
            "unauthorized",
        );
        fixture.0.catalog.write().await.take();
        failure(
            address,
            &path(&track.reference),
            Some(ADMIN_TOKEN),
            503,
            "catalog_unavailable",
        );
        server.abort();

        let anonymous = Fixture::new();
        let accounts_path = accounts::accounts_path(&anonymous.0.data_dir);
        let configured = anonymous.accounts();
        std::fs::remove_file(&accounts_path).unwrap();
        assert!(confirm_access(&anonymous.0, &auth::SocketIdentity::default()).is_ok());
        accounts::save(&configured, &accounts_path).unwrap();
        assert_eq!(
            confirm_access(&anonymous.0, &auth::SocketIdentity::default())
                .unwrap_err()
                .status,
            StatusCode::UNAUTHORIZED
        );
    });
}
