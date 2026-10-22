use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::parental_test_support::{policy, save_policy};
use crate::test_support::{
    WebSocketReader, start_server, test_runtime, websocket_event, websocket_with_headers,
};
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

#[test]
fn established_notifications_close_when_parental_controls_are_enabled() {
    test_runtime().block_on(async {
        let mut published = Vec::new();
        for route in ["/api/v1/events", "/api/v1/activity"] {
            let fixture = Fixture::new();
            let (address, server) = start_server(fixture.0.clone()).await;
            let admin = login(address, "operator");
            let token = login(address, "alice");
            let mut socket = websocket_with_headers(
                address,
                route,
                &format!("Authorization: Bearer {token}\r\n"),
            );
            assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
            save_policy(address, &admin, "alice", policy(vec!["artist:ac/dc"]));
            assert_eq!(http(address, "GET", route, Some(&token), "").0, 403);
            // The heartbeat may already have retired the only subscriber.
            let _ = fixture.0.events.send(notification_for(route));
            if !notification_closed(&mut socket) {
                published.push(route);
            }
            server.abort();
        }
        assert!(
            published.is_empty(),
            "newly supervised sockets published notifications: {published:?}"
        );
    });
}

#[test]
fn pending_notification_snapshots_close_when_parental_controls_are_enabled() {
    test_runtime().block_on(async {
        let mut published = Vec::new();
        for route in ["/api/v1/events", "/api/v1/activity"] {
            for release_catalog in [true, false] {
                let fixture = Fixture::new();
                let (address, server) = start_server(fixture.0.clone()).await;
                let admin = login(address, "operator");
                let token = login(address, "alice");
                let mut catalog = Some(fixture.0.catalog.write().await);
                let mut socket = websocket_with_headers(
                    address,
                    route,
                    &format!("Authorization: Bearer {token}\r\n"),
                );
                wait_for_initial_snapshot(&fixture.0).await;
                save_policy(address, &admin, "alice", policy(vec!["artist:ac/dc"]));
                if release_catalog {
                    drop(catalog.take());
                }
                if !notification_closed(&mut socket) {
                    published.push((route, release_catalog));
                }
                drop(catalog);
                server.abort();
            }
        }
        assert!(published.is_empty(), "newly supervised pending snapshots were not retired (route, catalog released): {published:?}");
    });
}

#[test]
fn established_notifications_fail_closed_when_an_observed_parental_store_is_unavailable() {
    test_runtime().block_on(async {
        let mut published = Vec::new();
        for route in ["/api/v1/events", "/api/v1/activity"] {
            for missing in [true, false] {
                let fixture = Fixture::new();
                let (address, server) = start_server(fixture.0.clone()).await;
                let admin = login(address, "operator");
                let token = login(address, "alice");
                save_disabled_policy(address, &admin);
                let mut socket = websocket_with_headers(
                    address,
                    route,
                    &format!("Authorization: Bearer {token}\r\n"),
                );
                assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
                make_policy_unavailable(&fixture.0, missing);
                assert_eq!(http(address, "GET", route, Some(&token), "").0, 503);
                let _ = fixture.0.events.send(notification_for(route));
                if !notification_closed(&mut socket) {
                    published.push((route, missing));
                }
                server.abort();
            }
        }
        assert!(published.is_empty(), "unverifiable parental policies allowed notifications (route, store missing): {published:?}");
    });
}

#[test]
fn pending_notification_snapshots_fail_closed_when_an_observed_parental_store_is_unavailable() {
    test_runtime().block_on(async {
        let mut published = Vec::new();
        for route in ["/api/v1/events", "/api/v1/activity"] {
            for missing in [true, false] {
                for release_catalog in [true, false] {
                    let fixture = Fixture::new();
                    let (address, server) = start_server(fixture.0.clone()).await;
                    let admin = login(address, "operator");
                    let token = login(address, "alice");
                    save_disabled_policy(address, &admin);
                    let mut catalog = Some(fixture.0.catalog.write().await);
                    let mut socket = websocket_with_headers(
                        address,
                        route,
                        &format!("Authorization: Bearer {token}\r\n"),
                    );
                    wait_for_initial_snapshot(&fixture.0).await;
                    make_policy_unavailable(&fixture.0, missing);
                    if release_catalog {
                        drop(catalog.take());
                    }
                    if !notification_closed(&mut socket) {
                        published.push((route, missing, release_catalog));
                    }
                    drop(catalog);
                    server.abort();
                }
            }
        }
        assert!(published.is_empty(), "unverifiable parental policies allowed pending snapshots (route, store missing, catalog released): {published:?}");
    });
}

#[test]
fn permitted_notifications_survive_catalog_publications_and_filter_task_events() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let admin = login(address, "operator");
        let token = login(address, "alice");
        save_disabled_policy(address, &admin);
        let policy_reads = fixture.0.parental.read_slots.available_permits();
        let mut sockets = Vec::new();
        for index in 0..policy_reads + 2 {
            let route = if index % 2 == 0 {
                "/api/v1/events"
            } else {
                "/api/v1/activity"
            };
            let mut socket = websocket_with_headers(
                address,
                route,
                &format!("Authorization: Bearer {token}\r\n"),
            );
            assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
            sockets.push((route, socket));
        }
        for scanned_at in [101, 102] {
            let gate = fixture
                .0
                .parental
                .read_slots
                .clone()
                .acquire_many_owned(policy_reads.try_into().unwrap())
                .await
                .unwrap();
            fixture.0.catalog.write().await.as_mut().unwrap().scanned_at = scanned_at;
            fixture.0.parental.invalidate();
            fixture
                .0
                .events
                .send(notification_for("/api/v1/activity"))
                .unwrap();
            fixture
                .0
                .events
                .send(CatalogEvent::CatalogChanged {
                    scanned_at: Some(scanned_at),
                })
                .unwrap();
            // More sockets than policy workers must queue through a burst and
            // retain input handling while their shared admission is occupied.
            for (_, socket) in &mut sockets {
                ping_notification(socket);
            }
            drop(gate);
            for (route, socket) in &mut sockets {
                if *route == "/api/v1/activity" {
                    let task = websocket_event(socket);
                    assert_eq!(task["type"], "task_failed");
                    assert_eq!(task["message"], "fictional scan failure");
                }
                let event = websocket_event(socket);
                assert_eq!(event["type"], "catalog_changed");
                assert_eq!(event["scanned_at"], scanned_at);
            }
        }
        server.abort();
    });
}

fn notification_for(route: &str) -> CatalogEvent {
    if route == "/api/v1/activity" {
        CatalogEvent::TaskFailed {
            task_id: 7,
            task_kind: "scan",
            code: "scan_failed",
            message: "fictional scan failure".into(),
        }
    } else {
        CatalogEvent::CatalogChanged {
            scanned_at: Some(101),
        }
    }
}

fn notification_closed(socket: &mut WebSocketReader) -> bool {
    socket
        .stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut frame = [0; 64];
    match socket.stream.read(&mut frame) {
        Ok(0) => true,
        Ok(_) => frame[0] & 0x0f == 8,
        Err(error) => error.kind() == std::io::ErrorKind::ConnectionReset,
    }
}

fn ping_notification(socket: &mut WebSocketReader) {
    socket
        .stream
        .write_all(&[0x89, 0x81, 1, 2, 3, 4, b'x' ^ 1])
        .unwrap();
    let mut pong = [0; 3];
    socket.stream.read_exact(&mut pong).unwrap();
    assert_eq!(pong, [0x8a, 1, b'x']);
}

fn save_disabled_policy(address: SocketAddr, admin: &str) {
    let mut disabled = policy(vec![]);
    disabled["enabled"] = serde_json::json!(false);
    save_policy(address, admin, "alice", disabled);
}

fn make_policy_unavailable(state: &ApiState, missing: bool) {
    let path = aede_core::parental::path(&state.data_dir);
    if missing {
        std::fs::remove_file(path).unwrap();
    } else {
        std::fs::write(path, "invalid protected store").unwrap();
    }
}
