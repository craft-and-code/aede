use super::*;
use crate::accounts_test_support::Fixture;
use crate::test_support::test_runtime;
use axum::extract::ws::WebSocketUpgrade;
use axum::routing::get;
use std::sync::mpsc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "runtime_test_support.rs"]
mod support;

#[test]
fn local_http_shutdown_waits_for_the_upgraded_socket_to_release_its_slot() {
    test_runtime().block_on(async {
        let mut state = crate::test_support::sample_state();
        state.connection_slots = Arc::new(Semaphore::new(1));
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (release_socket, released_socket) = tokio::sync::oneshot::channel();
        let released_socket = Arc::new(std::sync::Mutex::new(Some(released_socket)));
        let application = Router::new().route(
            "/upgrade",
            get(move |upgrade: WebSocketUpgrade| {
                let released_socket = released_socket.clone();
                async move {
                    upgrade.on_upgrade(move |socket| async move {
                        let released = released_socket.lock().unwrap().take().unwrap();
                        let _ = released.await;
                        drop(socket);
                    })
                }
            }),
        );
        let shutdown_receiver = state.shutdown.subscribe();
        let server_state = state.clone();
        let mut server = tokio::spawn(async move {
            serve_connections(listener, None, application, &server_state, shutdown_receiver).await
        });
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!("GET /upgrade HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let response = support::response_headers(&mut client).await;
        assert!(response.starts_with(b"HTTP/1.1 101"));
        assert_eq!(state.connection_slots.available_permits(), 0);
        state.shutdown.send(()).unwrap();
        let early_result = tokio::time::timeout(Duration::from_millis(50), &mut server).await;
        let waited_for_upgrade = early_result.is_err();
        release_socket.send(()).unwrap();
        let result = match early_result {
            Ok(result) => result,
            Err(_) => tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .expect("the released socket must allow shutdown"),
        };
        result.unwrap().unwrap();
        assert!(
            waited_for_upgrade,
            "a WebSocket callback remains part of graceful shutdown"
        );
        assert_eq!(state.connection_slots.available_permits(), 1);
    });
}

#[test]
fn local_http_admission_counts_clients_with_incomplete_headers() {
    test_runtime().block_on(async {
        let mut fixture = Fixture::new();
        fixture.0.connection_slots = Arc::new(Semaphore::new(1));
        let state = fixture.0.clone();
        let (address, shutdown, server) = support::start_http(state.clone()).await;
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(b"GET /api/v1/catalog HTTP/1.1\r\n")
            .await
            .unwrap();
        let admitted = tokio::time::timeout(Duration::from_millis(500), async {
            while state.connection_slots.available_permits() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        drop(client);
        shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("an incomplete header must not hold shutdown")
            .unwrap()
            .unwrap();
        assert!(
            admitted,
            "an admitted HTTP client must hold a connection slot"
        );
        assert_eq!(state.connection_slots.available_permits(), 1);
    });
}

#[test]
fn an_already_requested_shutdown_stops_a_catalog_watcher_before_its_first_poll() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let watcher = watch_catalog(store::catalog_path(&fixture.0.data_dir), fixture.0.clone());
        assert!(fixture.0.shutdown.send(()).is_ok());
        tokio::time::timeout(Duration::from_millis(200), watcher)
            .await
            .expect("the watcher must observe shutdown requested before its first poll");
    });
}

#[test]
fn local_http_shutdown_waits_for_an_accepted_synchronous_scan() {
    test_runtime().block_on(async {
        let mut fixture = Fixture::new();
        let accounts = fixture.accounts();
        let (principal, _) = auth::start_session(
            &fixture.0,
            &accounts,
            accounts.find("operator").unwrap(),
        )
        .unwrap();
        let (release, released) = mpsc::channel();
        let released = std::sync::Mutex::new(released);
        fixture.0.admin.as_mut().unwrap().scan = Arc::new(move |_| {
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .map_err(|failure| failure.to_string())
        });
        let mut activity = fixture.0.events.subscribe();
        let (address, shutdown, mut server) = support::start_http(fixture.0.clone()).await;
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!(
                    "POST /api/admin/v1/scan HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    auth::session_token(&principal),
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    activity.recv().await.unwrap(),
                    CatalogEvent::TaskStarted { task_kind: "scan", .. }
                ) {
                    break;
                }
            }
        })
        .await
        .expect("the synchronous scan must be accepted");
        shutdown.send(()).unwrap();
        let early_result = tokio::time::timeout(Duration::from_millis(50), &mut server).await;
        let waited_for_scan = early_result.is_err();
        release.send(()).unwrap();
        let result = match early_result {
            Ok(result) => result,
            Err(_) => tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .expect("shutdown must finish after the scan"),
        };
        result.unwrap().unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert!(waited_for_scan, "shutdown must preserve the accepted scan");
        assert!(String::from_utf8(response).unwrap().starts_with("HTTP/1.1 200"));
    });
}

#[test]
fn local_http_shutdown_does_not_wait_for_an_unfinished_get_body() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let accounts = fixture.accounts();
        let (principal, _) = auth::start_session(
            &fixture.0,
            &accounts,
            accounts.find("alice").unwrap(),
        )
        .unwrap();
        let (address, shutdown, mut server) = support::start_http(fixture.0.clone()).await;
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!(
                    "GET /api/v1/status HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nContent-Length: 100\r\n\r\n",
                    auth::session_token(&principal),
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let _ = support::response_headers(&mut client).await;
        shutdown.send(()).unwrap();
        let stopped = tokio::time::timeout(Duration::from_secs(2), &mut server).await;
        let stopped_without_body = stopped.is_ok();
        drop(client);
        match stopped {
            Ok(result) => result.unwrap().unwrap(),
            Err(_) => tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .expect("dropping the incomplete client must release shutdown")
                .unwrap()
                .unwrap(),
        }
        assert!(
            stopped_without_body,
            "an unfinished GET body must not hold shutdown"
        );
    });
}

#[test]
fn runtime_shutdown_is_bounded_while_a_blocking_worker_is_stalled() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (entered_at_worker, entered) = mpsc::channel();
    let (release, released_to_worker) = mpsc::channel();
    let (finished_by_worker, finished) = mpsc::channel();
    runtime.spawn_blocking(move || {
        entered_at_worker.send(()).unwrap();
        released_to_worker.recv().unwrap();
        finished_by_worker.send(()).unwrap();
    });
    entered.recv_timeout(Duration::from_secs(1)).unwrap();

    let started = Instant::now();
    shutdown_runtime(runtime, Duration::from_millis(50));
    assert!(started.elapsed() < Duration::from_secs(1));

    // `shutdown_timeout` does not cancel blocking work. Release this fixture
    // and observe its exit so it cannot survive into another test.
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(1)).unwrap();
}
