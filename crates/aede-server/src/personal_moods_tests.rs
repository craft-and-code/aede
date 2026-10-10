use super::*;
#[test]
fn principal_artist_tags_select_owned_tracks_without_guest_or_other_owner_leakage() {
    let state = crate::test_support::sample_state();
    let guard = state.catalog.try_read().unwrap();
    let catalog = guard.as_ref().unwrap();
    let track = EntityRef::of(catalog, EntityKind::Track, 0).unwrap();
    let release = catalog
        .release(catalog.track(0).unwrap().release_id.unwrap())
        .unwrap();
    let artist = EntityRef::of(
        catalog,
        EntityKind::Artist,
        release.album_artist_id.unwrap(),
    )
    .unwrap();
    let mut data = UserData::default();
    data.entry("one", &artist, 1)
        .tags
        .insert("relaxation".into());
    let calm = domain(&rules(&data, "one").unwrap()[0]);
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm).len(),
        1
    );
    assert!(matches(&evidence(catalog, &data, "two").unwrap(), &calm).is_empty());
    data.entry("one", &track, 2).tags.insert("calme".into());
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm).len(),
        1
    );
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm)[0]
            .1
            .tag
            .as_deref(),
        Some("calme")
    );
}

#[test]
fn mood_http_rules_remain_owner_private_and_select_normalized_artist_tags() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{encoded, start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let artist = {
            let guard = fixture.0.catalog.read().await;
            let catalog = guard.as_ref().unwrap();
            let release = catalog
                .release(catalog.track(0).unwrap().release_id.unwrap())
                .unwrap();
            EntityRef::of(
                catalog,
                EntityKind::Artist,
                release.album_artist_id.unwrap(),
            )
            .unwrap()
            .to_token()
        };
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let auditor = login(address, "auditor");
        let path = "/api/me/v1/moods";
        assert_eq!(http(address, "GET", path, None, "").0, 401);
        let rule = r#"{"id":"winter","name":"Winter","tags":["Concentration"],"genres":[]}"#;
        assert_eq!(http(address, "PUT", path, Some(&auditor), rule).0, 403);
        assert_eq!(http(address, "PUT", path, Some(&alice), rule).0, 200);
        let annotation = format!("/api/me/v1/annotation?ref={}", encoded(&artist));
        assert_eq!(
            http(
                address,
                "PUT",
                &annotation,
                Some(&alice),
                r#"{"tags":["concentration"]}"#
            )
            .0,
            200
        );
        let (status, page, _) = http(
            address,
            "GET",
            "/api/me/v1/moods?mood=winter&limit=1",
            Some(&alice),
            "",
        );
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["reason"]["tag"], "concentration");
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/moods?mood=winter",
                Some(&bob),
                ""
            )
            .0,
            400
        );
        let bytes = std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap();
        for invalid in [
            r#"{"id":"winter","name":"Winter","tags":[],"genres":[]}"#,
            r#"{"id":"winter","name":"Winter","tags":["x"],"genres":[],"ignored":true}"#,
        ] {
            assert_eq!(http(address, "PUT", path, Some(&alice), invalid).0, 400);
        }
        assert_eq!(
            std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap(),
            bytes
        );
        server.abort();
    });
}
