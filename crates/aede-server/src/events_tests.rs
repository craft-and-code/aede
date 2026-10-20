use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime, websocket_with_headers};
use std::io::{Read, Write};

#[test]
fn logout_while_the_initial_notification_snapshot_waits_cannot_publish_it() {
    test_runtime().block_on(async {
        for release_catalog in [true, false] {
            let fixture = Fixture::new();
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut catalog = Some(fixture.0.catalog.write().await);
            let mut socket = websocket_with_headers(
                address,
                "/api/v1/events",
                &format!("Authorization: Bearer {token}\r\n"),
            );
            wait_for_initial_snapshot(&fixture.0).await;
            assert_eq!(
                http(address, "DELETE", "/api/auth/v1/session", Some(&token), "").0,
                204
            );
            if release_catalog {
                drop(catalog.take());
            }
            socket
                .stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut frame = [0; 64];
            let result = socket.stream.read(&mut frame);
            drop(catalog);
            server.abort();
            assert!(
                result.as_ref().is_ok_and(|count| *count == 0 || frame[0] & 0x0f == 8),
                "logout must close the socket without publishing its pending snapshot (catalog released={release_catalog}): {result:?}"
            );
        }
    });
}

#[test]
fn shutdown_interrupts_the_initial_notification_catalog_wait() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let catalog = fixture.0.catalog.write().await;
        let mut socket = websocket_with_headers(
            address,
            "/api/v1/activity",
            &format!("Authorization: Bearer {token}\r\n"),
        );
        wait_for_initial_snapshot(&fixture.0).await;
        fixture.0.shutdown.send(()).unwrap();
        socket
            .stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let mut frame = [0; 64];
        let result = socket.stream.read(&mut frame);
        drop(catalog);
        server.abort();
        assert!(
            result
                .as_ref()
                .is_ok_and(|count| *count == 0 || frame[0] & 0x0f == 8),
            "shutdown must close notifications without waiting for catalog publication: {result:?}"
        );
    });
}

async fn wait_for_initial_snapshot(state: &ApiState) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.events.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("notifications must reach the initial catalog read");
}

#[test]
fn a_disconnected_notification_client_releases_its_slot_before_the_catalog_is_available() {
    test_runtime().block_on(async {
        for path in ["/api/v1/events", "/api/v1/activity"] {
            let fixture = Fixture::new();
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let catalog = fixture.0.catalog.write().await;
            let slots = fixture.0.websocket_slots.available_permits();
            let socket = websocket_with_headers(
                address,
                path,
                &format!("Authorization: Bearer {token}\r\n"),
            );
            wait_for_initial_snapshot(&fixture.0).await;
            assert_eq!(fixture.0.websocket_slots.available_permits(), slots - 1);
            drop(socket);
            let released = tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.websocket_slots.available_permits() != slots {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            drop(catalog);
            server.abort();
            assert!(released.is_ok(), "{path}: a disconnected client must not retain its notification slot while waiting for a catalog writer");
        }
    });
}

#[test]
fn pending_notification_snapshots_keep_ping_and_forbidden_input_handling_active() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let catalog = fixture.0.catalog.write().await;
        let mut socket = websocket_with_headers(
            address,
            "/api/v1/activity",
            &format!("Authorization: Bearer {token}\r\n"),
        );
        wait_for_initial_snapshot(&fixture.0).await;
        socket
            .stream
            .write_all(&[0x89, 0x81, 1, 2, 3, 4, b'x' ^ 1])
            .unwrap();
        let mut pong = [0; 3];
        socket.stream.read_exact(&mut pong).unwrap();
        assert_eq!(pong, [0x8a, 1, b'x']);
        socket
            .stream
            .write_all(&[0x81, 0x81, 1, 2, 3, 4, b'x' ^ 1])
            .unwrap();
        let mut close = [0; 2];
        let result = socket.stream.read_exact(&mut close);
        drop(catalog);
        server.abort();
        result.unwrap();
        assert_eq!(
            close[0] & 0x0f,
            8,
            "application data must close the socket even before its first snapshot"
        );
    });
}
