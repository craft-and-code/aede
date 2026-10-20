use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
use aede_core::sources::{ArtistFacts, SourceReview};

fn seed(fixture: &Fixture) -> Sources {
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: "ac/dc".into(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("11111111-1111-4111-8111-111111111111".into()),
        fetched_at: 1,
        confidence: Confidence::Matched(95),
        facts: Facts::Artist(ArtistFacts {
            area: Some("Example region".into()),
            kind: Some("Group".into()),
            ..Default::default()
        }),
    });
    // Unrelated decisions are retained even when their original claim is absent.
    held.set_review(SourceReview {
        entity: EntityRef {
            kind: EntityKind::Artist,
            key: "absent artist".into(),
        },
        source: "manual".into(),
        source_id: None,
        decision: ReviewDecision::Rejected,
        reviewed_at: 1,
    });
    sources::save(&held, &sources::sources_path(&fixture.0.data_dir)).unwrap();
    held
}
#[test]
fn source_review_is_admin_only_durable_reversible_and_rejects_stale_decisions() {
    test_runtime().block_on(async {
        let fixture = Fixture::new(); let before = seed(&fixture);
        let (address, server) = start_server(fixture.0.clone()).await;
        let admin = login(address,"operator"); let user = login(address,"alice");
        assert_eq!(http(address,"GET","/api/admin/v1/source-review",Some(&user),"").0,403);
        let (status,page,_) = http(address,"GET","/api/admin/v1/source-review",Some(&admin),"");
        assert_eq!(status,200,"{page}"); assert_eq!(page["total"],1);
        let row = &page["items"][0]; assert_eq!(row["score"],95); assert_eq!(row["kind"],"artist");
        assert!(row["url"].as_str().unwrap().starts_with("https://musicbrainz.org/artist/"));
        let input = serde_json::json!({"id":row["id"],"revision":row["revision"],"decision":"accept"}).to_string();
        let (status,accepted,_) = http(address,"PATCH","/api/admin/v1/source-review",Some(&admin),&input);
        assert_eq!(status,200,"{accepted}"); assert_eq!(accepted["decision"],"accepted");
        let saved = sources::load_all(&sources::sources_path(&fixture.0.data_dir)).unwrap().unwrap();
        assert_eq!(saved.records,before.records); assert_eq!(saved.reviews.len(),2);
        let bytes = std::fs::read(sources::sources_path(&fixture.0.data_dir)).unwrap();
        assert_eq!(http(address,"PATCH","/api/admin/v1/source-review",Some(&admin),&input).0,409);
        assert_eq!(std::fs::read(sources::sources_path(&fixture.0.data_dir)).unwrap(),bytes);
        assert_eq!(http(address,"GET","/api/admin/v1/source-review",Some(&admin),"").1["total"],0);
        let undo = serde_json::json!({"id":accepted["id"],"revision":accepted["revision"],"decision":"undo"}).to_string();
        assert_eq!(http(address,"PATCH","/api/admin/v1/source-review",Some(&admin),&undo).0,200);
        assert_eq!(http(address,"GET","/api/admin/v1/source-review",Some(&admin),"").1["total"],1);
        server.abort();
    });
}
#[test]
fn review_does_not_overwrite_refreshed_facts_and_refuses_unsupported_inputs() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let mut held = seed(&fixture);
        let (address, server) = start_server(fixture.0.clone()).await;
        let admin = login(address, "operator");
        let page = http(
            address,
            "GET",
            "/api/admin/v1/source-review",
            Some(&admin),
            "",
        )
        .1;
        if let Facts::Artist(facts) = &mut held.records[0].facts {
            facts.area = Some("Changed region".into());
        }
        sources::save(&held, &sources::sources_path(&fixture.0.data_dir)).unwrap();
        let row = &page["items"][0];
        let body =
            serde_json::json!({"id":row["id"],"revision":row["revision"],"decision":"reject"})
                .to_string();
        assert_eq!(
            http(
                address,
                "PATCH",
                "/api/admin/v1/source-review",
                Some(&admin),
                &body
            )
            .0,
            409
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/admin/v1/source-review?unknown=1",
                Some(&admin),
                ""
            )
            .0,
            400
        );
        assert_eq!(
            http(
                address,
                "PATCH",
                "/api/admin/v1/source-review",
                Some(&admin),
                r#"{"id":"a","revision":"a","decision":"accept"}"#
            )
            .0,
            400
        );
        assert_eq!(
            sources::load_all(&sources::sources_path(&fixture.0.data_dir))
                .unwrap()
                .unwrap(),
            held
        );
        server.abort();
    });
}

#[test]
fn review_denies_guests_and_auditors_and_preserves_data_under_a_writer_or_bad_input() {
    test_runtime().block_on(async {
        let fixture = Fixture::new(); seed(&fixture);
        let (address, server) = start_server(fixture.0.clone()).await;
        let admin = login(address,"operator"); let auditor = login(address,"auditor");
        let row = http(address,"GET","/api/admin/v1/source-review",Some(&admin),"").1["items"][0].clone();
        let body = serde_json::json!({"id":row["id"],"revision":row["revision"],"decision":"accept"}).to_string();
        for (token, expected) in [(None,401),(Some(auditor.as_str()),403)] {
            assert_eq!(http(address,"GET","/api/admin/v1/source-review",token,"").0,expected);
            assert_eq!(http(address,"PATCH","/api/admin/v1/source-review",token,&body).0,expected);
        }
        let path=sources::sources_path(&fixture.0.data_dir);
        let bytes=std::fs::read(&path).unwrap();
        let lock=StoreLock::try_acquire(&fixture.0.data_dir).unwrap();
        assert_eq!(http(address,"PATCH","/api/admin/v1/source-review",Some(&admin),&body).0,409);
        drop(lock);
        for (uri, body) in [
            ("/api/admin/v1/source-review?all=true",body.clone()),
            ("/api/admin/v1/source-review",serde_json::json!({"id":row["id"],"revision":row["revision"],"decision":"accept","unknown":true}).to_string()),
        ] { assert_eq!(http(address,"PATCH",uri,Some(&admin),&body).0,400); }
        assert_eq!(std::fs::read(&path).unwrap(),bytes);
        let (_,empty,_) = http(address,"GET","/api/admin/v1/source-review?q=unknown",Some(&admin),"");
        assert_eq!(empty["total"],0);
        let (_,page,_) = http(address,"GET","/api/admin/v1/source-review?offset=1&limit=1&q=ac%2Fdc",Some(&admin),"");
        assert_eq!(page["total"],1); assert_eq!(page["items"].as_array().unwrap().len(),0);
        std::fs::write(&path,b"invalid source store").unwrap();
        assert_eq!(http(address,"PATCH","/api/admin/v1/source-review",Some(&admin),&body).0,500);
        assert_eq!(std::fs::read(&path).unwrap(),b"invalid source store");
        server.abort();
    });
}

#[test]
fn release_review_compares_the_same_identity_namespace_and_retires_changed_local_groups() {
    let state = crate::test_support::sample_state();
    let mut catalog = state.catalog.blocking_read().as_ref().unwrap().clone();
    catalog.releases[0].mbid = Some("11111111-1111-4111-8111-111111111111".into());
    catalog.release_groups[0].mbid = "22222222-2222-4222-8222-222222222222".into();
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).unwrap();
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: entity.key,
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("33333333-3333-4333-8333-333333333333".into()),
        fetched_at: 1,
        confidence: Confidence::Matched(95),
        facts: Facts::Release(sources::ReleaseFacts::default()),
    });
    let item = held.review_items(&catalog).remove(0);
    let before = view(&catalog, &held, &item).unwrap();
    assert_eq!(
        before.local_mbid.as_deref(),
        Some(catalog.release_groups[0].mbid.as_str()),
        "release-group claims must be compared with group tags, never edition tags"
    );
    catalog.release_groups[0].mbid = "44444444-4444-4444-8444-444444444444".into();
    let after = view(&catalog, &held, &held.review_items(&catalog).remove(0)).unwrap();
    assert_ne!(
        before.revision, after.revision,
        "a changed local identity requires renewed consent even for a fuzzy proposal"
    );
}

#[test]
fn recording_and_label_reviews_include_local_ids_and_retire_changed_local_identities() {
    let state = crate::test_support::sample_state();
    let original = state.catalog.blocking_read().as_ref().unwrap().clone();
    for kind in [EntityKind::Track, EntityKind::Label] {
        let mut catalog = original.clone();
        let local_id = "11111111-1111-4111-8111-111111111111";
        match kind {
            EntityKind::Track => catalog.recordings[0].mbid = Some(local_id.into()),
            EntityKind::Label => catalog.labels[0].mbid = Some(local_id.into()),
            _ => unreachable!(),
        }
        let entity = EntityRef::of(&catalog, kind, 0).unwrap();
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: entity.key,
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("22222222-2222-4222-8222-222222222222".into()),
            fetched_at: 1,
            confidence: Confidence::Matched(95),
            facts: match kind {
                EntityKind::Track => Facts::Track(Default::default()),
                EntityKind::Label => Facts::Label(Default::default()),
                _ => unreachable!(),
            },
        });
        let item = held.review_items(&catalog).remove(0);
        let before = view(&catalog, &held, &item).unwrap();
        assert_eq!(
            before.local_mbid.as_deref(),
            Some(local_id),
            "{kind:?}: show the local identity before consent"
        );
        match kind {
            EntityKind::Track => catalog.recordings[0].mbid = None,
            EntityKind::Label => catalog.labels[0].mbid = None,
            _ => unreachable!(),
        };
        let after = view(&catalog, &held, &held.review_items(&catalog).remove(0)).unwrap();
        assert_ne!(
            before.revision, after.revision,
            "{kind:?}: stale consent must be rejected"
        );
    }
}
