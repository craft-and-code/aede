use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
use serde_json::json;
#[test]
fn advanced_search_retains_owner_scope_and_refuses_unknown_fields() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let catalog = fixture.0.catalog.blocking_read().clone().unwrap();
    let token = EntityRef::of(&catalog, EntityKind::Track, 0)
        .unwrap()
        .to_token();
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let auditor = login(address, "auditor");
    let target = format!(
        "/api/me/v1/annotation?ref={}",
        crate::test_support::encoded(&token)
    );
    assert_eq!(
        http(
            address,
            "PUT",
            &target,
            Some(&alice),
            r#"{"rating":4,"loved":true}"#
        )
        .0,
        200
    );
    let route = "/api/me/v1/search?expression=loved%20rating:4&limit=50";
    let (status, answer, _) = http(address, "GET", route, Some(&alice), "");
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["total"], 1);
    assert_eq!(answer["items"][0]["reference"], token);
    assert_eq!(http(address, "GET", route, Some(&bob), "").1["total"], 0);
    assert_eq!(http(address, "GET", route, Some(&auditor), "").0, 200);
    assert_eq!(http(address, "GET", route, None, "").0, 401);
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/search?expression=bitdepth:NaN",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/search?expression=loved&owner=bob",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    server.abort();
}

#[test]
fn track_browse_reverses_before_paging_and_saves_only_individual_owner_membership() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let (first, last) = {
        let mut guard = fixture.0.catalog.blocking_write();
        let catalog = guard.as_mut().unwrap();
        catalog.releases[0].year = Some(1995);
        let mut file = catalog.files[0].clone();
        file.id = 1;
        file.path = "/music/album/02.flac".into();
        catalog.files.push(file);
        let mut track = catalog.tracks[0].clone();
        track.id = 1;
        track.file_id = 1;
        track.title = "Zed".into();
        catalog.tracks.push(track);
        catalog.releases[0].track_ids.push(1);
        catalog.recordings[0].track_ids.push(1);
        store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
        (
            EntityRef::of(catalog, EntityKind::Track, 0)
                .unwrap()
                .to_token(),
            EntityRef::of(catalog, EntityKind::Track, 1)
                .unwrap()
                .to_token(),
        )
    };
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let mut filter = json!({"view":"tracks","mode":"years","years":[1990,2000],"alphabet":[0,26],"includeUnknownYear":false,"query":"","reverse":true});
    let mut expression = format!("browse-v1:{filter}");
    let route = format!(
        "/api/me/v1/search?limit=1&expression={}",
        crate::test_support::encoded(&expression)
    );
    let (status, result, _) = http(address, "GET", &route, Some(&alice), "");
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["total"], 2);
    assert_eq!(result["items"][0]["reference"], last);
    filter["query"] = json!("Hells Bells");
    expression = format!("browse-v1:{filter}");
    let body = json!({"expression":expression,"create_only":true}).to_string();
    assert_eq!(
        http(
            address,
            "PUT",
            "/api/me/v1/collection?name=Selected",
            Some(&alice),
            &body
        )
        .0,
        200
    );
    let route = format!(
        "/api/me/v1/search?expression={}",
        crate::test_support::encoded(&expression)
    );
    let result = http(address, "GET", &route, Some(&alice), "").1;
    assert_eq!(result["total"], 1);
    assert_eq!(result["items"][0]["reference"], first);
    assert_eq!(
        http(address, "GET", "/api/me/v1/collections", Some(&bob), "").1["total"],
        0
    );
    server.abort();
}
