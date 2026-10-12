use super::*;
use crate::accounts_test_support::{Fixture, http, login};

#[tokio::test]
async fn enclosing_watched_roots_never_expose_private_exports_through_shared_file_routes() {
    let fixture = Fixture::new();
    let root = fixture.0.data_dir.canonicalize().unwrap();
    let mut state = fixture.0.clone();
    state.data_dir = root.join("data");
    state.admin.as_mut().unwrap().data_dir = state.data_dir.clone();
    let accounts = fixture.accounts();
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&state.data_dir),
    )
    .unwrap();
    let catalog = {
        let mut guard = state.catalog.write().await;
        let catalog = guard.as_mut().unwrap();
        catalog.roots = vec![root.to_str().unwrap().into()];
        catalog.clone()
    };
    store::save_catalog_only(&catalog, &store::catalog_path(&state.data_dir)).unwrap();
    let public_folder = root.join("music");
    std::fs::create_dir(&public_folder).unwrap();
    let public_path = public_folder.join("public.m3u");
    std::fs::write(&public_path, b"01.flac\n").unwrap();
    let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = UserData::default();
    let alice = super::save_selection(
        &catalog,
        &mut data,
        &state.data_dir,
        &accounts.find("alice").unwrap().id,
        "Alice private",
        vec![track.clone()],
    )
    .unwrap();
    let bob = super::save_selection(
        &catalog,
        &mut data,
        &state.data_dir,
        &accounts.find("bob").unwrap().id,
        "Bob private",
        vec![track],
    )
    .unwrap();
    let alice_path = super::target_path(
        &state.data_dir,
        &accounts.find("alice").unwrap().id,
        &alice.id,
    )
    .unwrap();
    let bob_path =
        super::target_path(&state.data_dir, &accounts.find("bob").unwrap().id, &bob.id).unwrap();
    let files = library_files::discover(&catalog.roots, &[]).unwrap().files;
    let alice_file_id = files
        .iter()
        .find(|file| Path::new(&file.path) == alice_path)
        .unwrap()
        .id
        .clone();
    let bob_file_id = files
        .iter()
        .find(|file| Path::new(&file.path) == bob_path)
        .unwrap()
        .id
        .clone();
    let user_path = user::user_path(&state.data_dir);
    let before = std::fs::read(&user_path).unwrap();
    let alice_before = std::fs::read(&alice_path).unwrap();
    let bob_before = std::fs::read(&bob_path).unwrap();
    let (address, server) = crate::test_support::start_server(state).await;
    tokio::task::spawn_blocking(move || {
        for (name, own, foreign) in [("bob", &bob, &alice), ("alice", &alice, &bob)] {
            let token = login(address, name);
            for id in [&alice_file_id, &bob_file_id] {
                let (status, denied, _) = http(
                    address,
                    "GET",
                    &format!("/api/me/v1/m3u-files/{id}"),
                    Some(&token),
                    "",
                );
                assert_eq!(status, 404);
                assert_eq!(denied["error"]["code"], "m3u_file_not_found");
            }
            let (status, page, _) = http(address, "GET", "/api/me/v1/m3u-files", Some(&token), "");
            assert_eq!(status, 200);
            assert_eq!(page["total"], 1);
            assert_eq!(page["items"][0]["filename"], "public.m3u");
            let public_id = page["items"][0]["id"].as_str().unwrap();
            let (status, detail, _) = http(
                address,
                "GET",
                &format!("/api/me/v1/m3u-files/{public_id}"),
                Some(&token),
                "",
            );
            assert_eq!(status, 200);
            assert_eq!(detail["content"], "01.flac\n");
            let (status, private, _) = http(address, "GET", "/api/me/v1/m3u", Some(&token), "");
            assert_eq!(status, 200);
            assert_eq!(private["total"], 1);
            assert_eq!(private["items"][0]["name"], own.name);
            assert_eq!(
                http(
                    address,
                    "GET",
                    &format!("/api/me/v1/m3u/{}", foreign.id),
                    Some(&token),
                    ""
                )
                .0,
                404
            );
            assert_eq!(
                http(
                    address,
                    "GET",
                    &format!("/api/me/v1/m3u/{}", own.id),
                    Some(&token),
                    ""
                )
                .0,
                200
            );
        }
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(&public_path).unwrap(), b"01.flac\n");
    assert_eq!(std::fs::read(&alice_path).unwrap(), alice_before);
    assert_eq!(std::fs::read(&bob_path).unwrap(), bob_before);
    assert_eq!(std::fs::read(&user_path).unwrap(), before);
    server.abort();
}
