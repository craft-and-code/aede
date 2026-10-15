use super::*;
use crate::accounts_test_support::{Fixture, http, login};

#[tokio::test]
async fn playlist_playback_resolves_shared_files_and_private_occurrences_without_cross_owner_access()
 {
    let fixture = Fixture::new();
    let music = fixture.0.data_dir.join("music");
    std::fs::create_dir_all(&music).unwrap();
    let music = music.canonicalize().unwrap();
    let list = music.join("Example.m3u");
    std::fs::write(&list, "#EXTM3U\none.flac\none.flac\n").unwrap();
    let mut catalog = fixture.0.catalog.write().await;
    let c = catalog.as_mut().unwrap();
    c.roots = vec![music.to_str().unwrap().into()];
    c.files[0].path = music.join("one.flac").to_str().unwrap().into();
    let track = EntityRef::of(c, EntityKind::Track, 0).unwrap();
    store::save_catalog_only(c, &store::catalog_path(&fixture.0.data_dir)).unwrap();
    drop(catalog);
    let owner = fixture.accounts().find("alice").unwrap().id.clone();
    let mut data = UserData::default();
    let id = data
        .create_playlist(&owner, "Private", vec![track.clone(), track.clone()], 1)
        .unwrap();
    let path = user::user_path(&fixture.0.data_dir);
    user::save(&data, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    let changed = list.clone();
    tokio::task::spawn_blocking(move || {
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let route = format!("/api/me/v1/m3u/{id}/playback");
        assert_eq!(http(address, "GET", &route, None, "").0, 401);
        assert_eq!(http(address, "GET", &route, Some(&bob), "").0, 404);
        let (status, selection, _) = http(address, "GET", &route, Some(&alice), "");
        assert_eq!(status, 200, "{selection}");
        assert_eq!(selection["items"].as_array().unwrap().len(), 2);
        assert_eq!(selection["items"][0]["reference"], track.to_token());
        let (_, page, _) = http(address, "GET", "/api/me/v1/m3u-files", Some(&alice), "");
        let id = page["items"][0]["id"].as_str().unwrap();
        let route = format!("/api/me/v1/m3u-files/{id}/playback");
        let (status, selection, _) = http(address, "GET", &route, Some(&bob), "");
        assert_eq!(status, 200, "{selection}");
        assert_eq!(selection["items"].as_array().unwrap().len(), 2);
        std::fs::write(&changed, "one.flac\nhttps://example.invalid/unknown.flac\n").unwrap();
        assert_eq!(http(address, "GET", &route, Some(&alice), "").0, 409);
    })
    .await
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    server.abort();
}

#[test]
fn private_playlist_selection_refuses_an_ambiguous_catalogued_file() {
    let fixture = Fixture::new();
    let mut held = fixture.0.catalog.blocking_write();
    let catalog = held.as_mut().unwrap();
    let target = EntityRef::of(catalog, EntityKind::Track, 0).unwrap();
    let mut duplicate = catalog.tracks[0].clone();
    duplicate.id = catalog.tracks.len() as aede_core::model::Id;
    catalog.tracks.push(duplicate);
    assert!(rows(catalog, vec![target]).is_err());
}
