use super::*;
use crate::accounts_test_support::{ADMIN_TOKEN, Fixture, http, login};
use aede_core::user;

#[tokio::test]
async fn a_second_administrators_session_cannot_read_or_overwrite_the_local_owners_private_routes()
{
    let fixture = Fixture::new();
    let mut accounts = fixture.accounts();
    accounts
        .set_role("bob", aede_core::accounts::Role::Administrator, 11)
        .unwrap();
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
    )
    .unwrap();
    let catalog = fixture.0.catalog.read().await.clone().unwrap();
    let reference = EntityRef::of(&catalog, EntityKind::Track, 0)
        .unwrap()
        .to_token();
    let annotation = format!(
        "/api/me/v1/annotation?ref={}",
        crate::test_support::encoded(&reference)
    );
    let admin_annotation = annotation.replace("/api/me/", "/api/admin/");
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    let user_path = user::user_path(&fixture.0.data_dir);
    tokio::task::spawn_blocking(move || {
        let local = login(address, "operator");
        let bob = login(address, "bob");
        assert_eq!(
            http(
                address,
                "PUT",
                &annotation,
                Some(&local),
                r#"{"loved":true,"rating":5,"note":"Local private note","tags":["local-private"]}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(
                address,
                "PUT",
                "/api/me/v1/collection?name=Private",
                Some(&local),
                r#"{"expression":"loved"}"#
            )
            .0,
            200
        );
        let event =
            format!(r#"{{"track":"{reference}","at":100,"ms_played":1000,"completed":true}}"#);
        assert_eq!(
            http(address, "POST", "/api/me/v1/history", Some(&local), &event).0,
            201
        );
        let before = std::fs::read(&user_path).unwrap();
        for route in [
            &admin_annotation,
            "/api/admin/v1/history",
            "/api/admin/v1/collection?name=Private",
            "/api/admin/v1/collections",
        ] {
            assert_eq!(http(address, "GET", route, Some(&bob), "").0, 403);
            assert_eq!(http(address, "GET", route, Some(&local), "").0, 200);
            assert_eq!(http(address, "GET", route, Some(ADMIN_TOKEN), "").0, 200);
        }
        for (method, route, body) in [
            ("PUT", admin_annotation.as_str(), r#"{"note":"overwrite"}"#),
            ("POST", "/api/admin/v1/history", event.as_str()),
            ("DELETE", "/api/admin/v1/history", r#"{"confirm":true}"#),
            (
                "PUT",
                "/api/admin/v1/collection?name=Private",
                r#"{"expression":"title:other"}"#,
            ),
            (
                "PATCH",
                "/api/admin/v1/collection?name=Private",
                r#"{"name":"Renamed"}"#,
            ),
            ("DELETE", "/api/admin/v1/collection?name=Private", ""),
        ] {
            assert_eq!(http(address, method, route, Some(&bob), body).0, 403);
        }
        assert_eq!(std::fs::read(&user_path).unwrap(), before);
        assert!(http(address, "GET", &annotation, Some(&bob), "").1["annotation"].is_null());
        assert_eq!(
            http(address, "GET", "/api/me/v1/history", Some(&bob), "").1["total"],
            0
        );
        assert_eq!(
            http(address, "GET", "/api/admin/v1/accounts", Some(&bob), "").0,
            200
        );
        assert_eq!(
            http(address, "GET", "/api/v1/albums", Some(&bob), "").0,
            200
        );
        assert_eq!(
            http(
                address,
                "PUT",
                &annotation,
                Some(&bob),
                r#"{"note":"Bob private note"}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(address, "GET", &annotation, Some(&local), "").1["annotation"]["note"],
            "Local private note"
        );
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn accepted_history_writes_keep_other_owners_and_only_roll_the_writers_recent_journal() {
    let fixture = Fixture::new();
    let accounts = fixture.accounts();
    let alice_owner = accounts.find("alice").unwrap().id.clone();
    let bob_owner = accounts.find("bob").unwrap().id.clone();
    let catalog = fixture.0.catalog.read().await.clone().unwrap();
    let reference = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = user::UserData::default();
    data.record_play(user::Play {
        owner: alice_owner.clone(),
        track: reference.clone(),
        at: 1,
        ms_played: 1000,
        completed: true,
    });
    for at in 2..=user::HISTORY_LIMIT as u64 {
        data.record_play(user::Play {
            owner: bob_owner.clone(),
            track: reference.clone(),
            at,
            ms_played: 1000,
            completed: true,
        });
    }
    let path = user::user_path(&fixture.0.data_dir);
    user::save(&data, &path).unwrap();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        for at in [501, 502] {
            let event = format!(
                r#"{{"track":"{}","at":{at},"ms_played":1000,"completed":true}}"#,
                reference.to_token()
            );
            assert_eq!(
                http(address, "POST", "/api/me/v1/history", Some(&bob), &event).0,
                201
            );
        }
        let ours = http(address, "GET", "/api/me/v1/history", Some(&alice), "");
        assert_eq!(ours.0, 200);
        assert_eq!(ours.1["total"], 1);
        assert_eq!(ours.1["items"][0]["at"], 1);
        assert_eq!(ours.1["items"][0]["play_count"], 1);
        let theirs = http(address, "GET", "/api/me/v1/history", Some(&bob), "");
        assert_eq!(theirs.1["total"], user::HISTORY_LIMIT);
        assert_eq!(theirs.1["items"][0]["play_count"], user::HISTORY_LIMIT + 1);
        let restored = user::load(&path).unwrap().unwrap();
        assert_eq!(
            restored
                .plays
                .iter()
                .filter(|p| p.owner == alice_owner)
                .count(),
            1
        );
        assert_eq!(
            restored
                .plays
                .iter()
                .filter(|p| p.owner == bob_owner)
                .count(),
            user::HISTORY_LIMIT
        );
        assert_eq!(
            http(
                address,
                "DELETE",
                "/api/me/v1/history",
                Some(&bob),
                r#"{"confirm":true}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(address, "GET", "/api/me/v1/history", Some(&alice), "").1["total"],
            1
        );
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn opinions_and_same_named_collection_queries_remain_private_while_catalog_reads_are_shared()
{
    let fixture = Fixture::new();
    let catalog = fixture.0.catalog.read().await.clone().unwrap();
    let reference = EntityRef::of(&catalog, EntityKind::Track, 0)
        .unwrap()
        .to_token();
    let annotation = format!(
        "/api/me/v1/annotation?ref={}",
        crate::test_support::encoded(&reference)
    );
    let public = annotation.replace("/api/me/v1/annotation", "/api/v1/track");
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let a = r#"{"loved":true,"rating":5,"note":"Alice marker","tags":["private-alice"]}"#;
        let b = r#"{"loved":false,"rating":1,"note":"Bob marker","tags":["private-bob"]}"#;
        assert_eq!(http(address, "PUT", &annotation, Some(&alice), a).0, 200);
        assert_eq!(http(address, "PUT", &annotation, Some(&bob), b).0, 200);
        for (owner, note, rating, tag) in [
            (&alice, "Alice marker", 5, "private-alice"),
            (&bob, "Bob marker", 1, "private-bob"),
        ] {
            let value = http(address, "GET", &annotation, Some(owner), "").1;
            assert_eq!(value["annotation"]["note"], note);
            assert_eq!(value["annotation"]["rating"], rating);
            assert_eq!(value["annotation"]["tags"], serde_json::json!([tag]));
            assert_eq!(
                http(
                    address,
                    "PUT",
                    "/api/me/v1/collection?name=Saved",
                    Some(owner),
                    r#"{"expression":"loved"}"#
                )
                .0,
                200
            );
        }
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/collection/albums?name=Saved",
                Some(&alice),
                ""
            )
            .1["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            http(
                address,
                "GET",
                "/api/me/v1/collection/albums?name=Saved",
                Some(&bob),
                ""
            )
            .1["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for expression in ["tag:private-alice", "note:Alice", "rating:5", "loved"] {
            let route = format!(
                "/api/me/v1/search?expression={}",
                crate::test_support::encoded(expression)
            );
            assert_eq!(
                http(address, "GET", &route, Some(&bob), "").1["total"],
                0,
                "{expression}"
            );
        }
        let theirs = http(address, "GET", &public, Some(&bob), "");
        assert_eq!(theirs.0, 200);
        assert_eq!(http(address, "GET", &public, Some(&alice), "").1, theirs.1);
        for private in ["Alice marker", "Bob marker", "private-alice", "private-bob"] {
            assert!(!theirs.1.to_string().contains(private));
        }
        assert_eq!(
            http(
                address,
                "PATCH",
                "/api/me/v1/collection?name=Saved",
                Some(&bob),
                r#"{"name":"Renamed"}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(
                address,
                "DELETE",
                "/api/me/v1/collection?name=Renamed",
                Some(&bob),
                ""
            )
            .0,
            204
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/collection?name=Saved",
                Some(&alice),
                ""
            )
            .1["expression"],
            "loved"
        );
        assert_eq!(
            http(
                address,
                "PUT",
                &annotation,
                Some(&bob),
                r#"{"loved":false,"rating":null,"note":null,"tags":null}"#
            )
            .0,
            200
        );
        assert!(http(address, "GET", &annotation, Some(&bob), "").1["annotation"].is_null());
        assert_eq!(
            http(address, "GET", &annotation, Some(&alice), "").1["annotation"]["note"],
            "Alice marker"
        );
    })
    .await
    .unwrap();
    server.abort();
}
