use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::test_support::{
    hold_blocking_worker, start_server, websocket_event, websocket_with_headers,
};

#[path = "auth_notification_test_support.rs"]
mod support;
use std::io::Read;
use support::{ReadBudgets, ReadKind, queue_read};

#[test]
fn pending_notification_authentication_does_not_block_shutdown_or_peer_disconnection() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for shutdown in [true, false] {
            let fixture = Fixture::new();
            let slots = fixture.0.websocket_slots.available_permits();
            let budgets = ReadBudgets::capture(&fixture.0);
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = websocket_with_headers(
                address,
                "/api/v1/activity",
                &format!("Authorization: Bearer {token}\r\n"),
            );
            assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
            let queued = queue_read(fixture.0.clone(), budgets, ReadKind::Credentials).await;
            if shutdown {
                fixture.0.shutdown.send(()).unwrap();
            } else {
                socket.stream.shutdown(std::net::Shutdown::Both).unwrap();
            }
            let released = tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.websocket_slots.available_permits() != slots {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            let reads_while_retired = fixture.0.auth.read_slots.available_permits();
            queued.release().await;
            server.abort();
            assert!(released.is_ok(), "a queued authentication read must not hold a retired notification socket (shutdown={shutdown})");
            assert_eq!(reads_while_retired, budgets.credentials - 1, "queued credential work must retain its own read admission until it exits");
            assert_eq!(fixture.0.auth.read_slots.available_permits(), budgets.credentials);
            assert_eq!(fixture.0.parental.read_slots.available_permits(), budgets.policy);
        }
    });
}

#[test]
fn queued_notification_policy_reads_do_not_block_shutdown_or_peer_disconnection() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for route in ["/api/v1/events", "/api/v1/activity"] {
            for shutdown in [true, false] {
                let fixture = Fixture::new();
                let slots = fixture.0.websocket_slots.available_permits();
                let budgets = ReadBudgets::capture(&fixture.0);
                let (address, server) = start_server(fixture.0.clone()).await;
                let token = login(address, "alice");
                let mut socket = websocket_with_headers(
                    address,
                    route,
                    &format!("Authorization: Bearer {token}\r\n"),
                );
                assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
                let queued = queue_read(fixture.0.clone(), budgets, ReadKind::Policy).await;
                if shutdown {
                    fixture.0.shutdown.send(()).unwrap();
                } else {
                    socket.stream.shutdown(std::net::Shutdown::Both).unwrap();
                }
                let retired = tokio::time::timeout(Duration::from_secs(2), async {
                    while fixture.0.websocket_slots.available_permits() != slots {
                        tokio::task::yield_now().await;
                    }
                })
                .await;
                let reads_while_retired = fixture.0.parental.read_slots.available_permits();
                queued.release().await;
                server.abort();
                assert!(retired.is_ok(), "{route}: a queued parental-policy read must not hold a retired notification socket (shutdown={shutdown})");
                assert_eq!(reads_while_retired, 0, "queued policy work must retain its permit until it exits");
                assert_eq!(fixture.0.auth.read_slots.available_permits(), budgets.credentials);
                assert_eq!(fixture.0.parental.read_slots.available_permits(), budgets.policy);
            }
        }
    });
}

#[test]
fn queued_notification_policy_reads_cannot_publish_after_logout_or_revocation() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut published = Vec::new();
        for route in ["/api/v1/events", "/api/v1/activity"] {
            for revoke in [false, true] {
                let fixture = Fixture::new();
                let budgets = ReadBudgets::capture(&fixture.0);
                let (address, server) = start_server(fixture.0.clone()).await;
                let token = login(address, "alice");
                let mut socket = websocket_with_headers(
                    address,
                    route,
                    &format!("Authorization: Bearer {token}\r\n"),
                );
                assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
                let queued = queue_read(fixture.0.clone(), budgets, ReadKind::Policy).await;
                if revoke {
                    let _lock = StoreLock::try_acquire(&fixture.0.data_dir).unwrap();
                    let mut data = fixture.accounts();
                    data.revoke("alice", 20).unwrap();
                    accounts::save(&data, &accounts::accounts_path(&fixture.0.data_dir)).unwrap();
                } else {
                    let data = fixture.accounts();
                    let captured = principal(&fixture.0, &data, &token, false, Instant::now()).unwrap();
                    end_session(&fixture.0, &captured).unwrap();
                }
                queued.release().await;
                socket.stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut frame = [0; 64];
                let closed = match socket.stream.read(&mut frame) {
                    Ok(0) => true,
                    Ok(_) => frame[0] & 0x0f == 8,
                    Err(error) => error.kind() == std::io::ErrorKind::ConnectionReset,
                };
                if !closed {
                    published.push((route, revoke));
                }
                server.abort();
            }
        }
        assert!(published.is_empty(), "retired logins published notifications after queued policy validation (route, revoked): {published:?}");
    });
}

#[test]
fn pending_pcm_authentication_does_not_block_shutdown_or_peer_disconnection() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut delayed = Vec::new();
        for (username, shutdown) in [
            ("operator", true),
            ("operator", false),
            ("alice", true),
            ("alice", false),
        ] {
            let fixture = Fixture::new();
            let slots = fixture.0.playback_slots.available_permits();
            let reads = fixture.0.auth.read_slots.available_permits();
            let policy_reads = fixture.0.parental.read_slots.available_permits();
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, username);
            let socket = websocket_with_headers(
                address,
                "/api/me/v1/playback",
                &format!("Authorization: Bearer {token}\r\n"),
            );
            let (release, worker) = hold_blocking_worker().await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.auth.read_slots.available_permits()
                    + fixture.0.parental.read_slots.available_permits()
                    == reads + policy_reads
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the PCM socket must have a credential or policy read queued");
            if shutdown {
                fixture.0.shutdown.send(()).unwrap();
            } else {
                socket.stream.shutdown(std::net::Shutdown::Both).unwrap();
            }
            let limit = if shutdown {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(3)
            };
            let released = tokio::time::timeout(limit, async {
                while fixture.0.playback_slots.available_permits() != slots {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            let pending_reads = fixture.0.auth.read_slots.available_permits()
                + fixture.0.parental.read_slots.available_permits();
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
            drop(socket);
            server.abort();
            if released.is_err() {
                delayed.push((
                    username,
                    if shutdown {
                        "shutdown"
                    } else {
                        "peer disconnection"
                    },
                ));
            }
            assert_eq!(
                pending_reads,
                reads + policy_reads - 1,
                "queued reads retain their admission until the worker exits"
            );
            assert_eq!(fixture.0.auth.read_slots.available_permits(), reads);
            assert_eq!(
                fixture.0.parental.read_slots.available_permits(),
                policy_reads
            );
            assert!(
                aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            delayed.is_empty(),
            "queued credential reads kept retired PCM streams alive: {delayed:?}"
        );
    });
}
