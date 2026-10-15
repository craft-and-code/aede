use super::*;
use crate::accounts_test_support::{Fixture, http, login};

#[tokio::test]
async fn root_m3u_files_without_saved_exports_are_listed_and_read_without_rewriting() {
    let fixture = Fixture::new();
    let music = fixture.0.data_dir.join("music");
    let empty_album = music.join("No catalogued audio");
    std::fs::create_dir_all(&empty_album).unwrap();
    let playlist = empty_album.join("CLI selection.m3u8");
    let content = "\u{feff}#EXTM3U\r\n#EXTINF:1,First\r\n01.flac\r\n01.flac\r\n";
    std::fs::write(&playlist, content).unwrap();
    fixture.0.catalog.write().await.as_mut().unwrap().roots = vec![
        music.canonicalize().unwrap().to_str().unwrap().into(),
        empty_album.canonicalize().unwrap().to_str().unwrap().into(),
    ];
    let user_path = user::user_path(&fixture.0.data_dir);
    std::fs::write(&user_path, b"retained private store").unwrap();
    let state = fixture.0.clone();
    let (address, server) = crate::test_support::start_server(state).await;
    tokio::task::spawn_blocking(move || {
        let token = login(address, "alice");
        let (status, page, _) = http(address, "GET", "/api/me/v1/m3u-files", Some(&token), "");
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["name"], "CLI selection");
        assert!(page["items"][0]["track_count"].is_null());
        let id = page["items"][0]["id"].as_str().unwrap();
        assert_eq!(id.len(), "m3u-file-".len() + 64);
        let (status, detail, _) = http(
            address,
            "GET",
            &format!("/api/me/v1/m3u-files/{id}"),
            Some(&token),
            "",
        );
        assert_eq!(status, 200, "{detail}");
        assert_eq!(detail["content"], content);
        assert_eq!(detail["filename"], "CLI selection.m3u8");
        assert_eq!(detail["track_count"], 2);
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(&playlist).unwrap(), content.as_bytes());
    assert_eq!(
        std::fs::read(&user_path).unwrap(),
        b"retained private store"
    );
    server.abort();
}

#[tokio::test]
async fn root_files_are_shared_authenticated_reads_with_search_before_pagination_and_invalid_detail_errors()
 {
    let fixture = Fixture::new();
    let root = fixture.0.data_dir.join("music/Folder Match");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.m3u"), "01.flac\n").unwrap();
    std::fs::write(root.join("b.m3u8"), [0xff]).unwrap();
    fixture.0.catalog.write().await.as_mut().unwrap().roots = vec![
        root.canonicalize().unwrap().to_str().unwrap().into(),
        fixture.0.data_dir.join("missing").to_str().unwrap().into(),
    ];
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        assert_eq!(
            http(address, "GET", "/api/me/v1/m3u-files", None, "").0,
            401
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/m3u-files",
                Some(crate::accounts_test_support::ADMIN_TOKEN),
                ""
            )
            .0,
            401
        );
        for name in ["alice", "bob", "auditor"] {
            let token = login(address, name);
            let (status, page, _) = http(
                address,
                "GET",
                "/api/me/v1/m3u-files?q=folder%20match&offset=1&limit=1",
                Some(&token),
                "",
            );
            assert_eq!(status, 200, "{page}");
            assert_eq!(page["total"], 2);
            assert_eq!(page["unavailable_roots"], 1);
            assert_eq!(page["items"][0]["filename"], "b.m3u8");
            let id = page["items"][0]["id"].as_str().unwrap();
            let (status, error, _) = http(
                address,
                "GET",
                &format!("/api/me/v1/m3u-files/{id}"),
                Some(&token),
                "",
            );
            assert_eq!(status, 422);
            assert_eq!(error["error"]["code"], "m3u_text_invalid");
            assert_eq!(
                http(
                    address,
                    "GET",
                    "/api/me/v1/m3u-files/not-a-file-id",
                    Some(&token),
                    ""
                )
                .0,
                400
            );
            assert_eq!(
                http(
                    address,
                    "GET",
                    "/api/me/v1/m3u-files?path=%2Fother",
                    Some(&token),
                    ""
                )
                .0,
                400
            );
            assert_eq!(
                http(
                    address,
                    "GET",
                    "/api/me/v1/m3u-files?q=b.m3u8",
                    Some(&token),
                    ""
                )
                .1["total"],
                1
            );
        }
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(root.join("b.m3u8")).unwrap(), [0xff]);
    server.abort();
}

#[test]
fn changed_root_exclusion_catalog_or_revoked_session_refuses_a_prepared_response() {
    let fixture = Fixture::new();
    fixture
        .0
        .catalog
        .blocking_write()
        .as_mut()
        .unwrap()
        .roots
        .push("/music".into());
    let accounts = fixture.accounts();
    let (principal, _) =
        auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap()).unwrap();
    let owner = PersonalOwner {
        id: principal.owner.clone(),
        principal: Some(principal.clone()),
    };
    let scope = Scope::capture(&fixture.0).unwrap();
    assert!(scope.confirm(&fixture.0, &owner).is_ok());
    for change in 0..3 {
        let original = fixture.0.catalog.blocking_read().clone();
        {
            let mut guard = fixture.0.catalog.blocking_write();
            let catalog = guard.as_mut().unwrap();
            match change {
                0 => catalog.roots.clear(),
                1 => catalog.excluded.push("/music/Excluded".into()),
                _ => catalog.scanned_at += 1,
            }
        }
        assert!(scope.confirm(&fixture.0, &owner).is_err());
        *fixture.0.catalog.blocking_write() = original;
    }
    auth::end_session(&fixture.0, &principal).unwrap();
    assert!(scope.confirm(&fixture.0, &owner).is_err());
}

#[test]
fn json_expansion_is_refused_before_a_partial_response_can_be_published() {
    let content = "\t".repeat(8 * 1024 * 1024);
    assert!(response::serialize(&serde_json::json!({"content": content})).is_err());
}

#[cfg(unix)]
#[test]
fn retargeted_root_or_exclusion_alias_refuses_a_prepared_file_listing() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let first = fixture.0.data_dir.join("first");
    let second = fixture.0.data_dir.join("second");
    let included = first.join("Included");
    let excluded = first.join("Excluded");
    for folder in [&included, &excluded, &second] {
        std::fs::create_dir_all(folder).unwrap();
    }
    std::fs::write(included.join("selection.m3u"), b"01.flac\n").unwrap();
    let root_alias = fixture.0.data_dir.join("root-alias");
    let exclusion_alias = fixture.0.data_dir.join("exclusion-alias");
    symlink(&first, &root_alias).unwrap();
    symlink(&excluded, &exclusion_alias).unwrap();
    {
        let mut guard = fixture.0.catalog.blocking_write();
        let catalog = guard.as_mut().unwrap();
        catalog.roots = vec![root_alias.to_str().unwrap().into()];
        catalog.excluded = vec![exclusion_alias.to_str().unwrap().into()];
    }
    let accounts = fixture.accounts();
    let (principal, _) =
        auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap()).unwrap();
    let owner = PersonalOwner {
        id: principal.owner.clone(),
        principal: Some(principal),
    };
    for (alias, replacement, original) in [
        (&root_alias, &second, &first),
        (&exclusion_alias, &included, &excluded),
    ] {
        let scope = Scope::capture(&fixture.0).unwrap();
        assert_eq!(
            library_files::discover(&scope.roots, &scope.excluded)
                .unwrap()
                .files
                .len(),
            1
        );
        std::fs::remove_file(alias).unwrap();
        symlink(replacement, alias).unwrap();
        assert!(scope.confirm(&fixture.0, &owner).is_err());
        std::fs::remove_file(alias).unwrap();
        symlink(original, alias).unwrap();
    }
}

#[tokio::test]
async fn album_file_lookup_is_read_only_exact_and_confined_to_shared_roots() {
    let fixture = Fixture::new();
    let album = fixture.0.data_dir.join("music/Artist/First");
    std::fs::create_dir_all(&album).unwrap();
    let reference = {
        let mut held = fixture.0.catalog.write().await;
        let catalog = held.as_mut().unwrap();
        catalog.roots = vec![fixture.0.data_dir.join("music").to_string_lossy().into()];
        catalog.releases[0].folder = album.to_string_lossy().into();
        EntityRef::of(catalog, EntityKind::Release, catalog.releases[0].id)
            .unwrap()
            .to_token()
    };
    let query = format!(
        "/api/me/v1/m3u-files?ref={}&limit=1",
        reference
            .bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect::<String>()
    );
    let file = album.join("First.m3u");
    let state = fixture.0.clone();
    let (address, server) = crate::test_support::start_server(state).await;
    let shared = fixture.0.clone();
    tokio::task::spawn_blocking(move || {
        let token = login(address, "alice");
        let (status, page, _) = http(address, "GET", &query, Some(&token), "");
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["reference"], reference);
        assert_eq!(page["total"], 0);
        std::fs::write(album.join("unrelated.m3u"), b"other.flac\n").unwrap();
        let (status, page, _) = http(address, "GET", &query, Some(&token), "");
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 0);
        let bytes = b"#EXTM3U\n01.flac\n";
        std::fs::write(&file, bytes).unwrap();
        let (status, page, _) = http(address, "GET", &query, Some(&token), "");
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["filename"], "First.m3u");
        assert!(page["items"][0]["track_count"].is_null());
        let id = page["items"][0]["id"].as_str().unwrap();
        let (status, detail, _) = http(
            address,
            "GET",
            &format!("/api/me/v1/m3u-files/{id}"),
            Some(&token),
            "",
        );
        assert_eq!(status, 200, "{detail}");
        assert_eq!(detail["content"], "#EXTM3U\n01.flac\n");
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        let (status, _, _) = http(
            address,
            "GET",
            "/api/me/v1/m3u-files?ref=artist%3Aexample",
            Some(&token),
            "",
        );
        assert_eq!(status, 400);
        let private = shared.data_dir.join("playlists/bob/Private");
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(private.join("Private.m3u"), b"bob-private\n").unwrap();
        let private_reference = {
            let mut catalog = shared.catalog.blocking_write();
            let catalog = catalog.as_mut().unwrap();
            catalog.roots = vec![shared.data_dir.to_string_lossy().into()];
            catalog.releases[0].folder = private.to_string_lossy().into();
            EntityRef::of(catalog, EntityKind::Release, catalog.releases[0].id)
                .unwrap()
                .to_token()
        };
        let private_query = format!(
            "/api/me/v1/m3u-files?ref={}",
            private_reference
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect::<String>()
        );
        let (status, page, _) = http(address, "GET", &private_query, Some(&token), "");
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 0);
    })
    .await
    .unwrap();
    server.abort();
}
