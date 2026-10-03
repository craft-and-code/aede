use super::*;
use crate::accounts_test_support::{ADMIN_TOKEN, Fixture, PASSWORD, http, login};
use crate::test_support::{start_server, test_runtime};

#[test]
fn login_protects_catalog_and_keeps_personal_data_separate() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        assert_eq!(http(address, "GET", "/api/v1/albums", None, "").0, 401);
        let alice = login(address, "ALICE");
        let bob = login(address, "bob");
        let catalog = fixture.0.catalog.read().await;
        let reference = EntityRef::of(catalog.as_ref().unwrap(), EntityKind::Track, 0)
            .unwrap()
            .to_token();
        drop(catalog);
        let path = format!(
            "/api/me/v1/annotation?ref={}",
            crate::test_support::encoded(&reference)
        );
        assert_eq!(
            http(
                address,
                "PUT",
                &path,
                Some(&alice),
                r#"{"loved":true,"note":"Alice's note"}"#
            )
            .0,
            200
        );
        assert!(http(address, "GET", &path, Some(&bob), "").1["annotation"].is_null());
        assert_eq!(
            http(address, "GET", &path, Some(&alice), "").1["annotation"]["note"],
            "Alice's note"
        );
        assert_eq!(
            http(
                address,
                "PUT",
                "/api/me/v1/collection?name=saved",
                Some(&alice),
                r#"{"expression":"loved"}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(address, "GET", "/api/me/v1/collections", Some(&bob), "").1["total"],
            0
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/me/v1/history",
                Some(&alice),
                &format!(r#"{{"track":"{reference}","at":100,"ms_played":10,"completed":true}}"#)
            )
            .0,
            201
        );
        assert_eq!(
            http(address, "GET", "/api/me/v1/history", Some(&bob), "").1["total"],
            0
        );
        assert_eq!(
            http(address, "GET", "/api/admin/v1/accounts", Some(&alice), "").0,
            403
        );
        assert_eq!(http(address, "GET", &path, Some(ADMIN_TOKEN), "").0, 401);
        assert_eq!(
            http(
                address,
                "GET",
                &format!("/api/v1/albums?token={alice}"),
                None,
                ""
            )
            .0,
            401
        );
        server.abort();
    });
}

#[test]
fn administration_preserves_owners_and_revokes_changed_sessions() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let operator = login(address, "operator");
        let alice = login(address, "alice");
        let owner = fixture.accounts().find("alice").unwrap().id.clone();
        let (status, view, _) = http(
            address,
            "PATCH",
            "/api/admin/v1/accounts/alice",
            Some(&operator),
            r#"{"username":"renamed","enabled":false}"#,
        );
        assert_eq!(status, 200, "{view}");
        assert_eq!(view["id"], owner);
        assert_eq!(
            http(address, "GET", "/api/v1/albums", Some(&alice), "").0,
            401
        );
        let before = std::fs::read(accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        assert_eq!(
            http(
                address,
                "PATCH",
                "/api/admin/v1/accounts/operator",
                Some(&operator),
                r#"{"enabled":false}"#
            )
            .0,
            400
        );
        assert_eq!(
            std::fs::read(accounts::accounts_path(&fixture.0.data_dir)).unwrap(),
            before
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/admin/v1/accounts",
                Some(&operator),
                &format!(r#"{{"username":"new-user","password":"{PASSWORD}","role":"user"}}"#)
            )
            .0,
            201
        );
        let new_session = login(address, "new-user");
        assert_eq!(
            http(
                address,
                "DELETE",
                "/api/admin/v1/accounts/new-user/sessions",
                Some(&operator),
                ""
            )
            .0,
            204
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/auth/v1/session",
                Some(&new_session),
                ""
            )
            .0,
            401
        );
        let list = http(
            address,
            "GET",
            "/api/admin/v1/accounts",
            Some(ADMIN_TOKEN),
            "",
        )
        .1
        .to_string();
        assert!(
            !list.contains("argon2") && !list.contains("password") && !list.contains("revision")
        );
        server.abort();
    });
}

#[test]
fn password_change_logout_and_invalid_input_have_no_partial_effect() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        assert_eq!(http(address, "PUT", "/api/auth/v1/password", Some(&alice), r#"{"current_password":"wrong","new_password":"another long passphrase"}"#).0, 401);
        assert_eq!(http(address, "PUT", "/api/auth/v1/password", Some(&alice), &format!(r#"{{"current_password":"{PASSWORD}","new_password":"another long passphrase"}}"#)).0, 204);
        assert_eq!(http(address, "GET", "/api/v1/albums", Some(&alice), "").0, 401);
        let bob = login(address, "bob");
        assert_eq!(http(address, "DELETE", "/api/auth/v1/session", Some(&bob), "").0, 204);
        assert_eq!(http(address, "GET", "/api/v1/albums", Some(&bob), "").0, 401);
        let before = std::fs::read(accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        for body in [r#"{"enabled":null}"#, r#"{"role":"owner"}"#, r#"{"username":"../escape"}"#, r#"{"extra":true}"#] {
            assert_eq!(http(address, "PATCH", "/api/admin/v1/accounts/alice", Some(ADMIN_TOKEN), body).0, 400);
            assert_eq!(std::fs::read(accounts::accounts_path(&fixture.0.data_dir)).unwrap(), before);
        }
        server.abort();
    });
}

#[test]
fn activation_closes_anonymous_sockets_and_revocation_closes_account_sockets() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let data = fixture.accounts();
        std::fs::remove_file(accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let mut socket = crate::test_support::websocket(address, "/api/v1/events");
        let _ = crate::test_support::websocket_event(&mut socket);
        accounts::save(&data, &accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        let mut bytes = [0; 64];
        let count = std::io::Read::read(&mut socket.stream, &mut bytes).unwrap();
        assert!(
            count == 0 || bytes[0] & 0x0f == 8,
            "activation must close anonymous access"
        );
        let alice = login(address, "alice");
        let mut account_socket = crate::test_support::websocket_with_headers(
            address,
            "/api/v1/events",
            &format!("Authorization: Bearer {alice}\r\n"),
        );
        let _ = crate::test_support::websocket_event(&mut account_socket);
        let principal = auth::principal(&fixture.0, &data, &alice, false, Instant::now()).unwrap();
        let identity = auth::SocketIdentity {
            session: Some(principal),
            administrative: false,
        };
        assert!(auth::socket_live(&fixture.0, &identity).await);
        let mut revoked = data;
        revoked.revoke("alice", 20).unwrap();
        accounts::save(&revoked, &accounts::accounts_path(&fixture.0.data_dir)).unwrap();
        assert!(!auth::socket_live(&fixture.0, &identity).await);
        let count = std::io::Read::read(&mut account_socket.stream, &mut bytes).unwrap();
        assert!(
            count == 0 || bytes[0] & 0x0f == 8,
            "revocation must close an existing account connection"
        );
        server.abort();
    });
}

#[test]
fn account_administrator_works_without_a_legacy_token_and_password_workers_are_bounded() {
    test_runtime().block_on(async {
        let mut fixture = Fixture::new();
        fixture.0.admin.as_mut().unwrap().token.clear();
        let (address, server) = start_server(fixture.0.clone()).await;
        let operator = login(address, "operator");
        assert_eq!(
            http(
                address,
                "GET",
                "/api/admin/v1/accounts",
                Some(&operator),
                ""
            )
            .0,
            200
        );
        let permit = fixture
            .0
            .auth
            .password_slots
            .clone()
            .acquire_many_owned(2)
            .await
            .unwrap();
        assert_eq!(
            http(
                address,
                "POST",
                "/api/auth/v1/session",
                None,
                &format!(r#"{{"username":"alice","password":"{PASSWORD}"}}"#)
            )
            .0,
            503
        );
        assert_eq!(
            http(address, "GET", "/api/v1/albums", Some(&operator), "").0,
            200
        );
        drop(permit);
        let _ = login(address, "alice");
        for _ in 0..5 {
            assert_eq!(
                http(
                    address,
                    "POST",
                    "/api/auth/v1/session",
                    None,
                    r#"{"username":"unknown","password":"wrong"}"#
                )
                .0,
                401
            );
        }
        let (status, _, headers) = http(
            address,
            "POST",
            "/api/auth/v1/session",
            None,
            r#"{"username":"unknown","password":"wrong"}"#,
        );
        assert_eq!(status, 429);
        assert!(headers.to_ascii_lowercase().contains("retry-after: 60"));
        server.abort();
    });
}
