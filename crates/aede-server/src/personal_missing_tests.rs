use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
use aede_core::sources::{ArtistFacts, Confidence, Facts, KnownRelease, SourceRecord, Sources};
#[test]
fn missing_routes_keep_owner_decisions_reversible_without_changing_sources() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let mut catalog = fixture.0.catalog.blocking_read().clone().unwrap();
    catalog.artists[0].mbid = Some("artist-id".into());
    store::save_catalog_only(&catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: catalog.artists[0].key.clone(),
        source: aede_core::sources::MUSICBRAINZ.into(),
        source_id: Some("artist-id".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            discography: vec![KnownRelease {
                mbid: "12345678-1234-1234-1234-123456789abc".into(),
                title: "Unowned studio album".into(),
                first_released: Some("2001-02".into()),
                primary_type: Some("Album".into()),
                secondary_types: vec![],
            }],
            ..Default::default()
        }),
    });
    let path = aede_core::sources::sources_path(&fixture.0.data_dir);
    aede_core::sources::save(&sources, &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let auditor = login(address, "auditor");
    let (status, page, _) = http(address, "GET", "/api/me/v1/missing", Some(&alice), "");
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["year"], 2001);
    let body = r#"{"release_group":"12345678-1234-1234-1234-123456789abc","set_aside":true}"#;
    assert_eq!(
        http(address, "PUT", "/api/me/v1/missing", Some(&auditor), body).0,
        403
    );
    assert_eq!(
        http(address, "PUT", "/api/me/v1/missing", Some(&alice), body).0,
        200
    );
    assert_eq!(
        http(address, "GET", "/api/me/v1/missing", Some(&alice), "").1["total"],
        0
    );
    assert_eq!(
        http(address, "GET", "/api/me/v1/missing", Some(&bob), "").1["total"],
        1
    );
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/missing?include_set_aside=true",
            Some(&alice),
            ""
        )
        .1["items"][0]["set_aside"],
        true
    );
    assert_eq!(
        http(
            address,
            "PUT",
            "/api/me/v1/missing",
            Some(&alice),
            &body.replace("true", "false")
        )
        .0,
        200
    );
    assert_eq!(
        http(address, "GET", "/api/me/v1/missing", Some(&alice), "").1["total"],
        1
    );
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/missing?owner=bob",
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
            "/api/me/v1/missing?limit=201",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
    let user_path = user::user_path(&fixture.0.data_dir);
    std::fs::write(&user_path, "{").unwrap();
    assert_eq!(
        http(address, "PUT", "/api/me/v1/missing", Some(&alice), body).0,
        500
    );
    assert_eq!(std::fs::read_to_string(user_path).unwrap(), "{");
    server.abort();
}
