use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::test_support::{
    hold_blocking_worker, start_server, websocket_event, websocket_with_headers,
};

#[test]
fn pending_notification_authentication_does_not_block_shutdown_or_peer_disconnection() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for shutdown in [true, false] {
            let fixture = Fixture::new();
            let slots = fixture.0.websocket_slots.available_permits();
            let reads = fixture.0.auth.read_slots.available_permits();
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = websocket_with_headers(
                address,
                "/api/v1/activity",
                &format!("Authorization: Bearer {token}\r\n"),
            );
            assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
            let (release, worker) = hold_blocking_worker().await;
            fixture
                .0
                .events
                .send(CatalogEvent::CatalogChanged { scanned_at: Some(1) })
                .unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.auth.read_slots.available_permits() == reads {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the established socket must have a credential read queued");
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
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
            server.abort();
            assert!(released.is_ok(), "a queued authentication read must not hold a retired notification socket (shutdown={shutdown})");
            assert_eq!(reads_while_retired, reads - 1, "queued credential work must retain its own read admission until it exits");
            assert_eq!(fixture.0.auth.read_slots.available_permits(), reads);
        }
    });
}
