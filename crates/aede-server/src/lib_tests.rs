use super::*;
use crate::test_support::*;
use aede_core::model::{Artist, AudioFile, Recording, Release, Track};
use std::io::{Read, Write};

#[test]
fn the_api_follows_recordings_and_other_canonical_entities() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });

        let page = json_response(address, "/api/v1/recordings?limit=1");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["reference"], "recording:recording-1");
        assert_eq!(page["items"][0]["work_count"], 1);
        let empty = json_response(address, "/api/v1/recordings?offset=1");
        assert_eq!(empty["total"], 1);
        assert!(empty["items"].as_array().unwrap().is_empty());

        let recording = json_response(address, "/api/v1/entities?ref=recording%3Arecording-1");
        assert_eq!(recording["kind"], "recording");
        assert_eq!(recording["tracks"][0], "track:/music/album/01.flac");
        assert_eq!(recording["works"][0], "work:work-1");

        let work = json_response(address, "/api/v1/entities?ref=work%3Awork-1");
        assert_eq!(work["recordings"][0], "recording:recording-1");

        let group = json_response(address, "/api/v1/entities?ref=release_group%3Agroup-1");
        assert_eq!(group["kind"], "release_group");
        assert!(
            group["releases"][0]
                .as_str()
                .unwrap()
                .starts_with("release:")
        );

        let label = json_response(address, "/api/v1/entities?ref=label%3Aatlantic");
        assert_eq!(label["name"], "Atlantic");
        assert_eq!(label["releases"], group["releases"]);

        let genre = json_response(address, "/api/v1/entities?ref=genre%3Arock");
        assert_eq!(genre["name"], "Rock");
        assert_eq!(genre["releases"], group["releases"]);
        assert_eq!(genre["tracks"][0], "track:/music/album/01.flac");

        server.abort();
    });
}

#[test]
fn http_and_websocket_reject_foreign_or_ambiguous_authorities_and_origins() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });
        let upgrade = "Upgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n";
        let authorities = [
            String::new(),
            "attacker.invalid:8787".to_string(),
            format!("127.0.0.1:{}", if address.port() == 1 { 2 } else { 1 }),
            format!("localhost:{}@attacker.invalid", address.port()),
            format!("127.0.0.1:{}, attacker.invalid", address.port()),
            format!("127.0.0.1:{}\r\nHost: localhost:{}", address.port(), address.port()),
        ];
        for path in ["/api/v1/status", "/api/v1/events", "/api/v1/activity"] {
            let absent_host = response_headers(address, &format!("GET {path} HTTP/1.0\r\nConnection: close\r\n\r\n"));
            assert!(absent_host.starts_with("HTTP/1.0 403") || absent_host.starts_with("HTTP/1.0 400"), "{absent_host}");
            for host in &authorities {
                let response = response_headers(address, &format!(
                    "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: Upgrade\r\n{upgrade}\r\n"
                ));
                assert!(response.starts_with("HTTP/1.1 400") || response.starts_with("HTTP/1.1 403"), "{host}: {response}");
            }
            for origin in [
                "https://attacker.invalid".to_string(),
                "null".to_string(),
                format!("http://{address}/extra"),
                format!("http://{address}, http://{address}"),
                format!("http://{address}\r\nOrigin: http://{address}"),
                format!("http://localhost:{}", address.port()),
            ] {
                let response = response_headers(address, &format!(
                    "GET {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nConnection: Upgrade\r\n{upgrade}\r\n"
                ));
                assert!(response.starts_with("HTTP/1.1 403"), "{origin}: {response}");
            }
            for host in [address.to_string(), format!("localhost:{}", address.port())] {
                for origin in [String::new(), format!("Origin: http://{host}\r\n")] {
                    let response = response_headers(address, &format!(
                        "GET {path} HTTP/1.1\r\nHost: {host}\r\n{origin}Connection: Upgrade\r\n{upgrade}\r\n"
                    ));
                    let expected = if path.ends_with("status") { "HTTP/1.1 200" } else { "HTTP/1.1 101" };
                    assert!(response.starts_with(expected), "{response}");
                }
            }
        }
        for target in ["http://attacker.invalid/api/v1/status".to_string(), format!("https://{address}/api/v1/status")] {
            let response = response_headers(address, &format!("GET {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"));
            assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        }
        server.abort();
    });
}

#[test]
fn local_origin_checks_accept_equivalent_default_http_ports() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        // Exercise the port-80 policy without requiring a privileged TCP bind.
        let server = tokio::spawn(async move {
            let policy_address = SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 80));
            let _ = axum::serve(listener, router(sample_state(), policy_address)).await;
        });
        for (host, origin) in [("127.0.0.1:80", "http://127.0.0.1"), ("localhost", "http://localhost:80"), ("LOCALHOST:80", "http://localhost")] {
            let response = response_headers(address, &format!("GET /api/v1/status HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nConnection: close\r\n\r\n"));
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        }
        server.abort();
    });
}

#[test]
fn administrative_scan_rejects_unknown_queries_and_invalid_bodies_before_starting() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut state = sample_state();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = calls.clone();
        state.admin = Some(Admin {
            token: "0123456789abcdef0123456789abcdef".into(),
            data_dir: std::env::temp_dir().join(format!("aede_invalid_scan_{}", std::process::id())),
            job: Arc::new(|_, _| Err("unexpected job".into())),
            scan: Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                Err("unexpected scan".into())
            }),
        });
        let mut events = state.events.subscribe();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        for (query, body, status, expected) in [
            ("?force=true", "\r\n", 400, "invalid_query"),
            ("", "Content-Length: 16\r\n\r\n{\"unknown\":true}", 400, "invalid_body"),
            ("", "Transfer-Encoding: chunked\r\n\r\n10\r\n{\"unknown\":true}\r\n0\r\n\r\n", 400, "invalid_body"),
            ("", "Transfer-Encoding: chunked\r\n\r\n", 408, "request_timeout"),
        ] {
            let request = format!("POST /api/admin/v1/scan{query} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAuthorization: Bearer 0123456789abcdef0123456789abcdef\r\n{body}");
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.starts_with(&format!("HTTP/1.1 {status}")), "{response}");
            assert!(response.contains(expected), "{response}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(events.try_recv().is_err());
        server.abort();
    });
}

#[test]
fn administrative_scan_requires_a_token_and_an_idle_store() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("aede_api_admin_{}_{}", std::process::id(), nonce));
        let path = store::catalog_path(&dir);
        let mut state = sample_state();
        let scan_path = path.clone();
        state.admin = Some(Admin {
            token: "0123456789abcdef0123456789abcdef".into(),
            data_dir: dir.clone(),
            job: Arc::new(|_, _| Err("unexpected job".into())),
            scan: Arc::new(move |progress| {
                progress(Progress::Discovered(1));
                progress(Progress::Read { done: 1, total: 1 });
                store::save_catalog_only(
                    &Catalog {
                        scanned_at: 1_700_000_100,
                        ..Default::default()
                    },
                    &scan_path,
                )
                .map_err(|error| error.to_string())
            }),
        });
        let mut events = state.events.subscribe();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let route = "/api/admin/v1/scan";
        let denied = request_method(address, "POST", route);
        assert!(denied.starts_with("HTTP/1.1 401"), "{denied}");
        assert!(!path.exists());
        let headers = "Authorization: Bearer 0123456789abcdef0123456789abcdef\r\n";
        let duplicate =
            request_with_headers(address, "POST", route, &format!("{headers}{headers}"));
        assert!(duplicate.starts_with("HTTP/1.1 401"), "{duplicate}");
        assert!(!path.exists());
        assert!(
            events.try_recv().is_err(),
            "ambiguous credentials cannot start a task"
        );
        let browser = request_with_headers(
            address,
            "POST",
            route,
            &format!("{headers}Origin: http://{address}\r\n"),
        );
        assert!(browser.starts_with("HTTP/1.1 401"), "{browser}");
        let lock = StoreLock::acquire(&dir).unwrap();
        let busy = request_with_headers(address, "POST", route, headers);
        assert!(busy.starts_with("HTTP/1.1 409"), "{busy}");
        assert!(busy.contains("store_busy"));
        assert!(!path.exists());
        assert!(events.try_recv().is_err(), "no task was started");
        drop(lock);
        let completed = request_with_headers(address, "POST", route, headers);
        assert!(completed.starts_with("HTTP/1.1 200"), "{completed}");
        assert!(completed.contains("1700000100"));
        assert_eq!(
            json_response(address, "/api/v1/library")["scanned_at"],
            1_700_000_100u64
        );
        let started = events.try_recv().unwrap();
        let task_id = match started {
            CatalogEvent::TaskStarted {
                task_id,
                task_kind: "scan",
            } => task_id,
            other => panic!(
                "unexpected first event: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskProgress {
                task_id: id,
                phase: "discovered",
                done: 1,
                total: 1,
                ..
            } if id == task_id
        ));
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskProgress {
                task_id: id,
                phase: "reading",
                done: 1,
                total: 1,
                ..
            } if id == task_id
        ));
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::CatalogChanged {
                scanned_at: Some(1_700_000_100)
            }
        ));
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskCompleted {
                task_id: id,
                task_kind: "scan",
                scanned_at: Some(1_700_000_100)
            } if id == task_id
        ));
        assert!(events.try_recv().is_err(), "no duplicate task event");
        server.abort();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join(".aede.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    });
}

#[test]
fn a_failed_administrative_scan_emits_a_terminal_error() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "aede_api_failed_scan_{}_{}",
            std::process::id(),
            nonce
        ));
        let mut state = sample_state();
        state.admin = Some(Admin {
            token: "0123456789abcdef0123456789abcdef".into(),
            data_dir: dir.clone(),
            job: Arc::new(|_, _| Err("unexpected job".into())),
            scan: Arc::new(|_| Err("the music folder is unavailable".into())),
        });
        let mut events = state.events.subscribe();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let response = request_with_headers(
            address,
            "POST",
            "/api/admin/v1/scan",
            "Authorization: Bearer 0123456789abcdef0123456789abcdef\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 500"), "{response}");
        assert!(response.contains("scan_failed"));
        let task_id = match events.try_recv().unwrap() {
            CatalogEvent::TaskStarted { task_id, .. } => task_id,
            other => panic!(
                "unexpected first event: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskFailed {
                task_id: id,
                task_kind: "scan",
                code: "scan_failed",
                ..
            } if id == task_id
        ));
        assert!(events.try_recv().is_err(), "a failed task cannot complete");
        server.abort();
        std::fs::remove_file(dir.join(".aede.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    });
}

#[test]
fn an_administrative_scan_keeps_its_writer_lock_until_snapshot_publication() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let dir = std::env::temp_dir().join(format!(
            "aede_scan_publish_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut state = sample_state();
        let path = store::catalog_path(&dir);
        let scan_path = path.clone();
        let (saved, received) = std::sync::mpsc::channel();
        state.admin = Some(Admin {
            token: "0123456789abcdef0123456789abcdef".into(),
            data_dir: dir.clone(),
            job: Arc::new(|_, _| Err("unexpected job".into())),
            scan: Arc::new(move |_| {
                store::save_catalog_only(
                    &Catalog {
                        scanned_at: 1_700_000_101,
                        ..Default::default()
                    },
                    &scan_path,
                )
                .map_err(|error| error.to_string())?;
                saved.send(()).unwrap();
                Ok(())
            }),
        });
        let catalog = state.catalog.clone();
        let blocked_publication = catalog.write().await;
        let mut events = state.events.subscribe();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let response = tokio::task::spawn_blocking(move || {
            request_with_headers(
                address,
                "POST",
                "/api/admin/v1/scan",
                "Authorization: Bearer 0123456789abcdef0123456789abcdef\r\n",
            )
        });
        tokio::task::spawn_blocking(move || received.recv_timeout(Duration::from_secs(3)).unwrap())
            .await
            .unwrap();
        let next_dir = dir.clone();
        let (acquired, acquisition) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let next_writer = std::thread::spawn(move || {
            let _lock = StoreLock::acquire(&next_dir).unwrap();
            acquired.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(3)).unwrap();
        });
        let (acquired_before_publication, _acquisition) = tokio::task::spawn_blocking(move || {
            (
                acquisition.recv_timeout(Duration::from_millis(200)).is_ok(),
                acquisition,
            )
        })
        .await
        .unwrap();
        drop(blocked_publication);
        release.send(()).unwrap();
        let response = response.await.unwrap();
        next_writer.join().unwrap();
        server.abort();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(dir.join(".aede.lock")).unwrap();
        std::fs::remove_dir(dir).unwrap();
        assert!(
            !acquired_before_publication,
            "a competing writer acquired the store before the saved scan snapshot was published"
        );
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskStarted { .. }
        ));
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::CatalogChanged {
                scanned_at: Some(1_700_000_101)
            }
        ));
        assert!(matches!(
            events.try_recv().unwrap(),
            CatalogEvent::TaskCompleted {
                scanned_at: Some(1_700_000_101),
                ..
            }
        ));
        assert!(events.try_recv().is_err());
    });
}

#[test]
fn notification_websockets_close_on_application_data_or_oversized_frames() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });
        for frame in [
            vec![0x81, 0x82, 1, 2, 3, 4, b'x' ^ 1, b'x' ^ 2],
            vec![0x82, 0xfe, 0x08, 0x00, 0, 0, 0, 0],
        ] {
            let mut socket = websocket(address, "/api/v1/activity");
            assert_eq!(websocket_event(&mut socket)["type"], "snapshot");
            socket.stream.write_all(&frame).unwrap();
            let mut header = [0; 2];
            match socket.stream.read(&mut header) {
                Ok(0) => {}
                Ok(_) => assert_eq!(header[0] & 0x0f, 8, "server sends a close frame"),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
                    ) => {}
                Err(error) => panic!("server did not close the forbidden message: {error}"),
            }
        }
        server.abort();
    });
}

#[test]
fn notification_connection_limits_do_not_block_http_and_slots_are_released() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut state = sample_state();
        state.websocket_slots = Arc::new(Semaphore::new(2));
        let slots = state.websocket_slots.clone();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let mut first = websocket(address, "/api/v1/events");
        let mut second = websocket(address, "/api/v1/activity");
        assert_eq!(websocket_event(&mut first)["type"], "snapshot");
        assert_eq!(websocket_event(&mut second)["type"], "snapshot");
        let limited = response_headers(address, &format!("GET /api/v1/events HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"));
        assert!(limited.starts_with("HTTP/1.1 503"), "{limited}");
        assert_eq!(json_response(address, "/api/v1/status")["status"], "ok");
        drop(first);
        tokio::time::timeout(Duration::from_secs(3), async {
            while slots.available_permits() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }).await.expect("closing a socket releases its slot");
        let mut replacement = websocket(address, "/api/v1/events");
        assert_eq!(websocket_event(&mut replacement)["type"], "snapshot");
        drop(second);
        drop(replacement);
        server.abort();
    });
}

#[test]
fn server_exits_after_graceful_shutdown() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        #[cfg(unix)]
        let command_socket = std::env::temp_dir().join(format!(
            "aede-shutdown-{}-{}.sock",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        #[cfg(unix)]
        let command_listener = tokio::net::UnixListener::bind(&command_socket).unwrap();
        let server = tokio::spawn(run_http(
            listener,
            std::env::temp_dir().join("aede-nonexistent-catalog-for-shutdown-test"),
            sample_state(),
            #[cfg(unix)]
            command_listener,
            #[cfg(unix)]
            command_socket,
            #[cfg(unix)]
            Arc::new(|_, _| false),
            async move {
                let _ = stopped.await;
            },
        ));
        assert_eq!(json_response(address, "/api/v1/status")["status"], "ok");
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .expect("server should stop")
            .unwrap()
            .unwrap();
    });
}

#[test]
fn list_search_filters_sort_and_errors_follow_the_v1_contract() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = sample_state();
        let release_ref = {
            let mut guard = state.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            catalog.artists[0].mbid = Some("artist-1".into());
            catalog.artists[0].aliases.push("The Thunder".into());
            catalog.artists.push(Artist {
                id: 1,
                name: "Zebra".into(),
                sort_name: "Zebra".into(),
                key: "zebra".into(),
                ..Default::default()
            });
            catalog.files.extend([
                AudioFile {
                    id: 1,
                    path: "/music/alpha/01.flac".into(),
                    ..Default::default()
                },
                AudioFile {
                    id: 2,
                    path: "/music/zeta/01.flac".into(),
                    ..Default::default()
                },
            ]);
            catalog.releases.extend([
                Release {
                    id: 1,
                    title: "Alpha".into(),
                    year: Some(2000),
                    album_artist_id: Some(1),
                    track_ids: vec![1],
                    ..Default::default()
                },
                Release {
                    id: 2,
                    title: "Zeta".into(),
                    year: Some(1990),
                    album_artist_id: Some(0),
                    track_ids: vec![2],
                    ..Default::default()
                },
            ]);
            catalog.tracks.extend([
                Track {
                    id: 1,
                    file_id: 1,
                    release_id: Some(1),
                    recording_id: 1,
                    title: "À demain".into(),
                    ..Default::default()
                },
                Track {
                    id: 2,
                    file_id: 2,
                    release_id: Some(2),
                    recording_id: 2,
                    title: "Zenith".into(),
                    ..Default::default()
                },
            ]);
            catalog.recordings.extend([
                Recording {
                    id: 1,
                    title: "À demain".into(),
                    mbid: Some("recording-2".into()),
                    track_ids: vec![1],
                    ..Default::default()
                },
                Recording {
                    id: 2,
                    title: "Zenith".into(),
                    mbid: Some("recording-3".into()),
                    track_ids: vec![2],
                    work_ids: vec![0],
                    ..Default::default()
                },
            ]);
            catalog.works[0].recording_ids.push(2);
            reference(catalog, EntityKind::Release, 0).unwrap()
        };
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });

        let artists = json_response(address, "/api/v1/artists?q=thunder&sort=name");
        assert_eq!(artists["total"], 1);
        assert_eq!(artists["items"][0]["name"], "AC/DC");
        let artists = json_response(address, "/api/v1/artists?sort=name&order=desc");
        assert_eq!(artists["items"][0]["name"], "Zebra");
        let artists = json_response(address, "/api/v1/artists?mbid=artist-1");
        assert_eq!(artists["total"], 1);

        let releases = json_response(address, "/api/v1/releases?sort=year&order=desc");
        assert_eq!(releases["items"][0]["title"], "Alpha");
        assert_eq!(releases["items"][1]["title"], "Zeta");
        assert_eq!(releases["items"][2]["title"], "Back in Black");
        let releases = json_response(address, "/api/v1/releases?year=1990&q=zeta");
        assert_eq!(releases["total"], 1);
        let releases = json_response(address, "/api/v1/releases?artist=artist%3Aac%2Fdc&limit=1");
        assert_eq!(releases["total"], 2);
        assert_eq!(releases["items"].as_array().unwrap().len(), 1);

        let tracks = json_response(
            address,
            &format!("/api/v1/tracks?release={}", encoded(&release_ref)),
        );
        assert_eq!(tracks["total"], 1);
        assert_eq!(tracks["items"][0]["title"], "Hells Bells");
        let tracks = json_response(address, "/api/v1/tracks?q=demain&sort=title");
        assert_eq!(tracks["total"], 1);
        assert_eq!(tracks["items"][0]["title"], "À demain");

        let recordings = json_response(address, "/api/v1/recordings?work=work%3Awork-1&sort=title");
        assert_eq!(recordings["total"], 2);
        assert_eq!(recordings["items"][0]["title"], "Hells Bells");
        assert_eq!(recordings["items"][1]["title"], "Zenith");

        for path in [
            "/api/v1/artists?q=%20%20",
            "/api/v1/artists?sort=year",
            "/api/v1/artists?order=up",
            "/api/v1/artists?year=1990",
            "/api/v1/artists?surprise=1",
            "/api/v1/artists?sort=name&sort=catalog",
            "/api/v1/releases?artist=work%3Awork-1",
            "/api/v1/recordings?work=work%3A",
        ] {
            let response = request(address, path);
            assert!(response.starts_with("HTTP/1.1 400"), "{path}: {response}");
            assert!(response.contains("invalid_query"), "{path}: {response}");
        }
        for path in ["/api/v1/artists?offset=-1", "/api/v1/tracks?limit=201"] {
            let response = request(address, path);
            assert!(response.starts_with("HTTP/1.1 400"), "{path}: {response}");
            assert!(
                response.contains("invalid_pagination"),
                "{path}: {response}"
            );
        }
        let missing = request(address, "/api/v1/recordings?work=work%3Amissing");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        assert!(missing.contains("entity_not_found"));
        let empty_reference = request(address, "/api/v1/entities?ref=artist%3A");
        assert!(
            empty_reference.starts_with("HTTP/1.1 400"),
            "{empty_reference}"
        );
        assert!(empty_reference.contains("invalid_reference"));
        let wrong_method = request_method(address, "POST", "/api/v1/library");
        assert!(wrong_method.starts_with("HTTP/1.1 405"), "{wrong_method}");
        assert!(wrong_method.contains("method_not_allowed"));
        assert!(
            wrong_method
                .to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        let head = request_method(address, "HEAD", "/api/v1/status");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(head.ends_with("\r\n\r\n"), "{head}");
        server.abort();
    });
}

#[test]
fn the_http_api_exposes_stable_references_and_rejects_bad_pages() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });

        let response = request(address, "/api/v1/artists?limit=1&offset=0");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        let page: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["reference"], "artist:ac/dc");
        assert_eq!(page["scanned_at"], 1_700_000_000u64);

        let detail = request(address, "/api/v1/entities?ref=artist%3Aac%2Fdc");
        assert!(detail.starts_with("HTTP/1.1 200"), "{detail}");
        let (_, body) = detail.split_once("\r\n\r\n").unwrap();
        let entity: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(entity["kind"], "artist");
        assert_eq!(entity["name"], "AC/DC");

        let invalid = request(address, "/api/v1/artists?limit=0");
        assert!(invalid.starts_with("HTTP/1.1 400"), "{invalid}");
        assert!(invalid.contains("invalid_pagination"));

        let missing = request(address, "/api/v1/entities?ref=artist%3Aunknown");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        assert!(missing.contains("entity_not_found"));

        let unknown = request(address, "/api/v1/elsewhere");
        assert!(unknown.starts_with("HTTP/1.1 404"), "{unknown}");
        assert!(unknown.contains("not_found"));
        let disabled_admin = request_method(address, "POST", "/api/admin/v1/scan");
        assert!(
            disabled_admin.starts_with("HTTP/1.1 404"),
            "{disabled_admin}"
        );
        server.abort();
    });
}

#[test]
fn v1_http_responses_keep_the_required_page_and_nullable_fields() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });

        let status = json_response(address, "/api/v1/status");
        assert_eq!(status["api_version"], 1);
        assert_eq!(status["catalog_loaded"], true);
        let library = json_response(address, "/api/v1/library");
        for field in [
            "scanned_at",
            "files",
            "artists",
            "releases",
            "recordings",
            "tracks",
        ] {
            assert!(library[field].is_number(), "library.{field}");
        }
        for (route, fields) in [
            (
                "artists",
                &["reference", "name", "sort_name", "mbid", "aliases"][..],
            ),
            (
                "releases",
                &[
                    "reference",
                    "title",
                    "year",
                    "album_artist",
                    "track_count",
                    "cover_path",
                ],
            ),
            (
                "tracks",
                &["reference", "title", "release", "recording", "duration_ms"],
            ),
            (
                "recordings",
                &["reference", "title", "mbid", "track_count", "work_count"],
            ),
        ] {
            let response = request(address, &format!("/api/v1/{route}"));
            assert!(
                response
                    .to_ascii_lowercase()
                    .contains("content-type: application/json")
            );
            let (_, body) = response.split_once("\r\n\r\n").unwrap();
            let page: serde_json::Value = serde_json::from_str(body).unwrap();
            for field in ["items", "total", "offset", "limit", "scanned_at"] {
                assert!(page.get(field).is_some(), "{route}.{field}");
            }
            assert_eq!(page["offset"], 0);
            assert_eq!(page["limit"], 50);
            for field in fields {
                assert!(page["items"][0].get(field).is_some(), "{route}.{field}");
            }
        }
        let artist = json_response(address, "/api/v1/artists");
        assert!(artist["items"][0]["mbid"].is_null());
        assert!(artist["items"][0]["aliases"].is_array());

        server.abort();
    });
}

#[test]
fn the_websocket_starts_with_the_current_catalog_snapshot() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(sample_state(), address)).await;
        });
        let mut socket = websocket(address, "/api/v1/events");
        let event = websocket_event(&mut socket);
        assert_eq!(event["type"], "snapshot");
        assert_eq!(event["scanned_at"], 1_700_000_000u64);
        server.abort();
    });
}

#[test]
fn activity_messages_do_not_change_the_frozen_catalog_stream() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = sample_state();
        let events = state.events.clone();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let mut catalog = websocket(address, "/api/v1/events");
        let mut activity = websocket(address, "/api/v1/activity");
        assert_eq!(websocket_event(&mut catalog)["type"], "snapshot");
        assert_eq!(websocket_event(&mut activity)["type"], "snapshot");
        events
            .send(CatalogEvent::TaskStarted {
                task_id: 7,
                task_kind: "scan",
            })
            .unwrap();
        events
            .send(CatalogEvent::CatalogChanged {
                scanned_at: Some(1_700_000_010),
            })
            .unwrap();
        let catalog_event = websocket_event(&mut catalog);
        assert_eq!(catalog_event["type"], "catalog_changed");
        assert_eq!(catalog_event["scanned_at"], 1_700_000_010u64);
        let task_event = websocket_event(&mut activity);
        assert_eq!(task_event["type"], "task_started");
        assert_eq!(task_event["task_id"], 7);
        assert_eq!(task_event["task_kind"], "scan");
        assert_eq!(websocket_event(&mut activity)["type"], "catalog_changed");
        server.abort();
    });
}

#[test]
fn activity_websocket_serializes_progress_and_terminal_events_without_polluting_catalog_events() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = sample_state();
        let events = state.events.clone();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state, address)).await;
        });
        let mut catalog = websocket(address, "/api/v1/events");
        let mut activity = websocket(address, "/api/v1/activity");
        assert_eq!(websocket_event(&mut catalog)["type"], "snapshot");
        assert_eq!(websocket_event(&mut activity)["type"], "snapshot");

        for event in [
            CatalogEvent::TaskProgress {
                task_id: 8,
                task_kind: "scan",
                phase: "reading",
                done: 2,
                total: 3,
            },
            CatalogEvent::TaskCompleted {
                task_id: 8,
                task_kind: "scan",
                scanned_at: Some(1_700_000_011),
            },
            CatalogEvent::TaskFailed {
                task_id: 9,
                task_kind: "identification",
                code: "task_cancelled",
                message: "cancelled".into(),
            },
            CatalogEvent::Error {
                operation: "catalog_reload",
                code: "catalog_reload_failed",
                message: "unreadable catalog".into(),
            },
            CatalogEvent::CatalogChanged { scanned_at: None },
        ] {
            events.send(event).unwrap();
        }

        let progress = websocket_event(&mut activity);
        assert_eq!(progress["type"], "task_progress");
        assert_eq!(progress["task_id"], 8);
        assert_eq!(progress["phase"], "reading");
        assert_eq!(progress["done"], 2);
        assert_eq!(progress["total"], 3);
        let completed = websocket_event(&mut activity);
        assert_eq!(completed["type"], "task_completed");
        assert_eq!(completed["scanned_at"], 1_700_000_011u64);
        let failed = websocket_event(&mut activity);
        assert_eq!(failed["type"], "task_failed");
        assert_eq!(failed["code"], "task_cancelled");
        assert_eq!(failed["task_kind"], "identification");
        let error = websocket_event(&mut activity);
        assert_eq!(error["type"], "error");
        assert_eq!(error["operation"], "catalog_reload");
        assert_eq!(error["code"], "catalog_reload_failed");
        assert_eq!(websocket_event(&mut activity)["type"], "catalog_changed");
        let catalog_change = websocket_event(&mut catalog);
        assert_eq!(catalog_change["type"], "catalog_changed");
        assert!(catalog_change["scanned_at"].is_null());
        server.abort();
    });
}

#[test]
fn replacing_or_removing_the_catalog_updates_the_snapshot_and_announces_it() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("aede_api_reload_{}_{}", std::process::id(), nonce));
        std::fs::create_dir_all(&dir).unwrap();
        let path = store::catalog_path(&dir);
        let state = sample_state();
        let mut receiver = state.events.subscribe();
        store::save_catalog_only(
            &Catalog {
                scanned_at: 1_700_000_001,
                ..Default::default()
            },
            &path,
        )
        .unwrap();
        let mut known = Some((SystemTime::UNIX_EPOCH, 0));
        let mut reported_failure = None;
        let held = StoreLock::acquire(&dir).unwrap();
        refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
        assert!(
            receiver.try_recv().is_err(),
            "a writer's snapshot stays private"
        );
        assert_eq!(
            state.catalog.read().await.as_ref().unwrap().scanned_at,
            1_700_000_000
        );
        drop(held);
        refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
        let changed = receiver.try_recv().unwrap();
        assert!(matches!(
            changed,
            CatalogEvent::CatalogChanged {
                scanned_at: Some(1_700_000_001)
            }
        ));
        assert_eq!(
            state.catalog.read().await.as_ref().unwrap().scanned_at,
            1_700_000_001
        );
        let mut second_watcher = Some((SystemTime::UNIX_EPOCH, 0));
        refresh_catalog(&path, &state, &mut second_watcher, &mut reported_failure).await;
        assert_eq!(second_watcher, known);
        assert!(receiver.try_recv().is_err(), "one change has one event");

        std::fs::write(&path, "not-json").unwrap();
        refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
        assert!(matches!(
            receiver.try_recv().unwrap(),
            CatalogEvent::Error {
                operation: "catalog_reload",
                code: "catalog_reload_failed",
                ..
            }
        ));
        assert_eq!(
            state.catalog.read().await.as_ref().unwrap().scanned_at,
            1_700_000_001,
            "an unreadable replacement must not displace the last good snapshot"
        );
        refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
        assert!(
            receiver.try_recv().is_err(),
            "one event per failed revision"
        );

        std::fs::remove_file(&path).unwrap();
        refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
        let removed = receiver.try_recv().unwrap();
        assert!(matches!(
            removed,
            CatalogEvent::CatalogChanged { scanned_at: None }
        ));
        assert!(state.catalog.read().await.is_none());
        std::fs::remove_file(dir.join(".aede.lock")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    });
}
