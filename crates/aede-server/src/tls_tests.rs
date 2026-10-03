use super::*;
use crate::playback_test_support::install_wav;
use aede_core::user;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName};
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::runtime::serve_tls;

const AUTHORITY: &str = "aede.test:443";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct CertificateFixture {
    directory: PathBuf,
    certificate: PathBuf,
    private_key: PathBuf,
}

impl CertificateFixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "aede-tls-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, AtomicOrdering::Relaxed),
        ));
        fs::create_dir_all(&directory).unwrap();
        let certificate = directory.join("certificate.pem");
        let private_key = directory.join("private-key.pem");
        fs::write(&certificate, include_bytes!("../testdata/tls-cert.pem")).unwrap();
        fs::write(&private_key, include_bytes!("../testdata/tls-key.pem")).unwrap();
        #[cfg(unix)]
        fs::set_permissions(
            &private_key,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .unwrap();
        Self {
            directory,
            certificate,
            private_key,
        }
    }

    fn options(&self, authority: &str) -> TlsOptions {
        TlsOptions {
            certificate: self.certificate.clone(),
            private_key: self.private_key.clone(),
            authority: authority.into(),
        }
    }
}

impl Drop for CertificateFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn options(fixture: &CertificateFixture, authority: &str) -> ServerOptions {
    ServerOptions {
        // The public authority is allowed to differ from this private bind
        // port when a TLS-preserving NAT forwards the service.
        bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8443),
        tls: Some(fixture.options(authority)),
    }
}

#[test]
fn tls_options_refuse_unsafe_authorities_and_unprotected_key_material() {
    let fixture = CertificateFixture::new();
    assert!(configure(&options(&fixture, AUTHORITY)).unwrap().is_some());
    assert!(
        configure(&options(&fixture, "[::1]:443"))
            .unwrap()
            .is_some()
    );
    let zero_bind = ServerOptions {
        bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        tls: Some(fixture.options(AUTHORITY)),
    };
    assert!(
        configure(&zero_bind)
            .err()
            .unwrap()
            .to_string()
            .contains("explicit port")
    );
    for authority in [
        "aede.test",
        "aede.test:0",
        "aede.test:0443",
        "https://aede.test:443",
        "aede.test:443/path",
        "user@aede.test:443",
        "aede.test:443?query",
        "*:443",
        "-aede.test:443",
        "aede-.test:443",
    ] {
        let error = configure(&options(&fixture, authority)).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{authority}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&fixture.private_key, fs::Permissions::from_mode(0o644)).unwrap();
        let error = configure(&options(&fixture, AUTHORITY)).err().unwrap();
        assert!(error.to_string().contains("group or other"));
        fs::set_permissions(&fixture.private_key, fs::Permissions::from_mode(0o600)).unwrap();

        let link = fixture.directory.join("linked-private-key.pem");
        std::os::unix::fs::symlink(&fixture.private_key, &link).unwrap();
        let mut tls = fixture.options(AUTHORITY);
        tls.private_key = link;
        let error = configure(&ServerOptions {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8443),
            tls: Some(tls),
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("regular file"));
    }

    fs::write(&fixture.private_key, "not a private key").unwrap();
    let error = configure(&options(&fixture, AUTHORITY)).err().unwrap();
    assert!(error.to_string().contains("private key"));
}

#[test]
fn tls_options_bound_certificate_input_before_parsing() {
    let fixture = CertificateFixture::new();
    fs::write(
        &fixture.certificate,
        vec![b'x'; super::tls::MAX_PEM_BYTES as usize + 1],
    )
    .unwrap();
    let error = configure(&options(&fixture, AUTHORITY)).err().unwrap();
    assert!(error.to_string().contains("256 KiB"));
}

#[test]
fn plaintext_options_allow_each_loopback_family_only() {
    let localhost_v6 = ServerOptions {
        bind: SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 1234),
        tls: None,
    };
    assert!(configure(&localhost_v6).unwrap().is_none());
    let public = ServerOptions {
        bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 1234),
        tls: None,
    };
    let error = configure(&public).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn remote_start_refuses_to_publish_without_initialized_accounts() {
    let fixture = CertificateFixture::new();
    let state = test_support::sample_state();
    let data_dir = std::env::temp_dir().join(format!(
        "aede-remote-without-accounts-{}",
        NEXT_FIXTURE.fetch_add(1, AtomicOrdering::Relaxed),
    ));
    store::save_catalog_only(
        state.catalog.try_read().unwrap().as_ref().unwrap(),
        &store::catalog_path(&data_dir),
    )
    .unwrap();
    let announced = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = announced.clone();
    let result = serve_with_options(
        &data_dir,
        options(&fixture, AUTHORITY),
        Some(accounts_test_support::ADMIN_TOKEN.into()),
        |_| Ok(()),
        |_, _| false,
        |_, _| Ok(JobOutput::default()),
        move |_| observed.store(true, AtomicOrdering::Release),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("initialized accounts")
    );
    assert!(!announced.load(AtomicOrdering::Acquire));
    let _ = fs::remove_dir_all(data_dir);
}

#[test]
fn remote_https_enforces_the_configured_origin_and_keeps_wss_working() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let account_fixture = accounts_test_support::Fixture::new();
        let state = account_fixture.0.clone();
        let (address, server) = start_tls(state.clone(), &fixture).await;

        let unauthorized = https_request(
            address,
            &fixture,
            &request("GET", "/api/v1/status", AUTHORITY, None, None, ""),
        )
        .await;
        assert!(
            status(&unauthorized).starts_with("HTTP/1.1 401"),
            "{unauthorized}"
        );

        let login_body = format!(
            r#"{{"username":"operator","password":"{}"}}"#,
            accounts_test_support::PASSWORD
        );
        let login = https_request(
            address,
            &fixture,
            &request(
                "POST",
                "/api/auth/v1/session",
                AUTHORITY,
                Some(&format!("https://{AUTHORITY}")),
                Some("application/json"),
                &login_body,
            ),
        )
        .await;
        assert!(status(&login).starts_with("HTTP/1.1 200"), "{login}");
        let (_, login_body) = login.split_once("\r\n\r\n").unwrap();
        let token = serde_json::from_str::<serde_json::Value>(login_body).unwrap()["token"]
            .as_str()
            .unwrap()
            .to_owned();

        let accepted = https_request(
            address,
            &fixture,
            &request(
                "GET",
                "/api/v1/status",
                AUTHORITY,
                Some(&format!("https://{AUTHORITY}")),
                None,
                "",
            )
            .replace(
                "Connection: close",
                &format!("Authorization: Bearer {token}\r\nConnection: close"),
            ),
        )
        .await;
        assert!(status(&accepted).starts_with("HTTP/1.1 200"), "{accepted}");

        for (host, origin, target) in [
            (
                "aede.test:8443",
                Some("https://aede.test:8443"),
                "/api/v1/status",
            ),
            (AUTHORITY, Some("http://aede.test:443"), "/api/v1/status"),
            (AUTHORITY, Some("https://aede.test:443/"), "/api/v1/status"),
            (
                AUTHORITY,
                Some("https://attacker.invalid:443"),
                "/api/v1/status",
            ),
            (
                AUTHORITY,
                None,
                "https://attacker.invalid:443/api/v1/status",
            ),
        ] {
            let mut rejected = request("GET", target, host, origin, None, "");
            rejected = rejected.replace(
                "Connection: close",
                &format!("Authorization: Bearer {token}\r\nConnection: close"),
            );
            let response = https_request(address, &fixture, &rejected).await;
            assert!(status(&response).starts_with("HTTP/1.1 403"), "{response}");
        }

        let mut legacy = request("POST", "/api/admin/v1/scan", AUTHORITY, None, None, "");
        legacy = legacy.replace(
            "Connection: close",
            &format!(
                "Authorization: Bearer {}\r\nConnection: close",
                accounts_test_support::ADMIN_TOKEN
            ),
        );
        let legacy = https_request(address, &fixture, &legacy).await;
        assert!(status(&legacy).starts_with("HTTP/1.1 401"), "{legacy}");

        let mut absent_admin = request("POST", "/api/admin/v1/scan", AUTHORITY, None, None, "");
        absent_admin = absent_admin.replace(
            "Connection: close",
            &format!("Authorization: Bearer {token}\r\nConnection: close"),
        );
        let absent_admin = https_request(address, &fixture, &absent_admin).await;
        assert!(
            status(&absent_admin).starts_with("HTTP/1.1 404"),
            "{absent_admin}"
        );

        let (_websocket, websocket) = open_websocket(address, &fixture, &token).await;
        assert!(
            websocket
                .windows(b"HTTP/1.1 101".len())
                .any(|part| part == b"HTTP/1.1 101")
        );
        assert!(
            websocket
                .windows(b"snapshot".len())
                .any(|part| part == b"snapshot")
        );

        let mut plaintext = tokio::net::TcpStream::connect(address).await.unwrap();
        plaintext
            .write_all(b"GET /api/v1/status HTTP/1.1\r\nHost: aede.test:443\r\n\r\n")
            .await
            .unwrap();
        let mut byte = [0_u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(2), plaintext.read(&mut byte)).await;
        assert!(
            matches!(read, Ok(Ok(0)) | Ok(Err(_)))
                // Rustls may send an encrypted-record alert before it closes.
                // An HTTP response would begin with `H` and prove plaintext
                // reached the API instead of the TLS boundary.
                || matches!(read, Ok(Ok(1)) if byte[0] != b'H'),
            "plaintext reached HTTP handling: {byte:?}"
        );

        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    });
}

#[test]
fn remote_https_refuses_work_when_the_admission_budget_is_full() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let account_fixture = accounts_test_support::Fixture::new();
        let mut state = account_fixture.0.clone();
        let accounts = account_fixture.accounts();
        let (principal, _) =
            auth::start_session(&state, &accounts, accounts.find("operator").unwrap()).unwrap();
        let token = auth::session_token(&principal).to_owned();
        state.remote_request_slots = Arc::new(Semaphore::new(0));
        let (address, server) = start_tls(state.clone(), &fixture).await;

        let response = https_request(
            address,
            &fixture,
            &request("GET", "/api/v1/status", AUTHORITY, None, None, "").replace(
                "Connection: close",
                &format!("Authorization: Bearer {token}\r\nConnection: close"),
            ),
        )
        .await;
        assert!(status(&response).starts_with("HTTP/1.1 503"), "{response}");
        assert!(response.contains("request_limit"), "{response}");

        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    });
}

#[test]
fn remote_tls_playback_streams_acknowledged_pcm_and_records_the_listen() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let account_fixture = accounts_test_support::Fixture::new();
        let installed = install_wav(&account_fixture, 400).await;
        let state = account_fixture.0.clone();
        let owner = account_fixture
            .accounts()
            .find("alice")
            .expect("alice account")
            .id
            .clone();
        let (address, server) = start_tls(state.clone(), &fixture).await;
        let token = remote_session(address, &fixture).await;
        let mut socket = open_playback_websocket(address, &fixture, &token).await;
        socket
            .send_text(
                &serde_json::json!({
                    "type": "start",
                    "track": installed.reference,
                    "normalize": "off",
                })
                .to_string(),
            )
            .await;

        let TlsWebSocketFrame::Text(format) = socket.next().await else {
            panic!("playback format frame");
        };
        assert_eq!(format["type"], "format");
        assert_eq!(format["encoding"], "f32le");
        assert_eq!(format["sample_rate"], 8_000);
        assert_eq!(format["channels"], 1);

        let channels = format["channels"].as_u64().expect("channel count") as usize;
        let mut acknowledged = 0_u64;
        let mut eof = None;
        loop {
            match socket.next().await {
                TlsWebSocketFrame::Binary(bytes) => {
                    assert!(!bytes.is_empty());
                    assert!(bytes.len() <= 32 * 1024);
                    assert_eq!(bytes.len() % (channels * std::mem::size_of::<f32>()), 0);
                    let first = f32::from_le_bytes(bytes[..4].try_into().expect("PCM sample"));
                    assert!((first - (8_000.0 / 32_768.0)).abs() < f32::EPSILON);
                    let (samples, remaining) = bytes.as_chunks::<4>();
                    assert!(remaining.is_empty());
                    assert!(samples.iter().all(|sample| {
                        let value = f32::from_le_bytes(*sample);
                        value.is_finite() && (-1.0..=1.0).contains(&value)
                    }));
                    acknowledged += u64::try_from(bytes.len() / (channels * 4)).unwrap();
                    socket
                        .send_text(
                            &serde_json::json!({ "type": "ack", "frames": acknowledged })
                                .to_string(),
                        )
                        .await;
                }
                TlsWebSocketFrame::Text(frame) if frame["type"] == "eof" => {
                    assert_eq!(frame["frames"], acknowledged);
                    eof = Some(acknowledged);
                }
                TlsWebSocketFrame::Text(frame) if frame["type"] == "recorded" => {
                    assert_eq!(eof, Some(acknowledged));
                    assert_eq!(frame["ms_played"], 50);
                    assert_eq!(frame["completed"], true);
                    break;
                }
                TlsWebSocketFrame::Text(frame) => panic!("unexpected playback frame {frame}"),
                TlsWebSocketFrame::Close => panic!("playback closed before recording"),
            }
        }

        let history = user::load(&user::user_path(&account_fixture.0.data_dir))
            .expect("read recorded listen")
            .expect("recorded listen");
        let play = history.plays.last().expect("recorded play");
        assert_eq!(play.owner, owner);
        assert_eq!(play.track.to_token(), installed.reference);
        assert_eq!(play.ms_played, 50);
        assert!(play.completed);

        drop(socket);
        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    });
}

#[test]
fn remote_tls_playback_refuses_a_revoked_session_before_recording_history() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let account_fixture = accounts_test_support::Fixture::new();
        let installed = install_wav(&account_fixture, 16_000).await;
        let state = account_fixture.0.clone();
        let (address, server) = start_tls(state.clone(), &fixture).await;
        let token = remote_session(address, &fixture).await;
        let mut socket = open_playback_websocket(address, &fixture, &token).await;
        socket
            .send_text(
                &serde_json::json!({
                    "type": "start",
                    "track": installed.reference,
                    "normalize": "off",
                })
                .to_string(),
            )
            .await;
        let TlsWebSocketFrame::Text(format) = socket.next().await else {
            panic!("playback format frame");
        };
        let channels = format["channels"].as_u64().expect("channel count") as usize;
        let TlsWebSocketFrame::Binary(bytes) = socket.next().await else {
            panic!("playback PCM frame");
        };
        let frames = u64::try_from(bytes.len() / (channels * std::mem::size_of::<f32>())).unwrap();
        socket
            .send_text(&serde_json::json!({ "type": "ack", "frames": frames }).to_string())
            .await;

        let origin = format!("https://{AUTHORITY}");
        let revoked = https_request(
            address,
            &fixture,
            &request(
                "DELETE",
                "/api/auth/v1/session",
                AUTHORITY,
                Some(&origin),
                None,
                "",
            )
            .replace(
                "Connection: close",
                &format!("Authorization: Bearer {token}\r\nConnection: close"),
            ),
        )
        .await;
        assert!(status(&revoked).starts_with("HTTP/1.1 204"), "{revoked}");

        let failure = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match socket.next().await {
                    TlsWebSocketFrame::Text(frame) if frame["type"] == "error" => break frame,
                    TlsWebSocketFrame::Text(_) | TlsWebSocketFrame::Binary(_) => {}
                    TlsWebSocketFrame::Close => panic!("closed before revocation error"),
                }
            }
        })
        .await
        .expect("revocation response");
        assert_eq!(failure["code"], "authentication_expired");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            user::load(&user::user_path(&account_fixture.0.data_dir))
                .expect("read history")
                .is_none()
        );

        drop(socket);
        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    });
}

#[test]
fn tls_connection_admission_is_bounded_and_shutdown_stops_stalled_handshakes() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let mut state = test_support::sample_state();
        state.connection_slots = Arc::new(Semaphore::new(1));
        let (address, server) = start_tls(state.clone(), &fixture).await;

        let held = tokio::net::TcpStream::connect(address).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.connection_slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let mut refused = tokio::net::TcpStream::connect(address).await.unwrap();
        if refused.write_all(&[0x16]).await.is_ok() {
            let mut byte = [0_u8; 1];
            let read = tokio::time::timeout(Duration::from_secs(2), refused.read(&mut byte)).await;
            assert!(
                matches!(read, Ok(Ok(0)) | Ok(Err(_))),
                "connection above the limit stayed open"
            );
        }

        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(held);
    });
}

#[test]
fn tls_response_without_write_progress_releases_its_admission_slot() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let mut state = test_support::sample_state();
        state.connection_slots = Arc::new(Semaphore::new(1));
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let tls = configure(&options(&fixture, AUTHORITY)).unwrap().unwrap();
        let application = Router::new().route(
            "/large",
            axum::routing::get(|| async { vec![0_u8; 32 * 1024 * 1024] }),
        );
        let server = tokio::spawn(serve_tls(
            listener,
            tls.acceptor,
            application,
            state.clone(),
        ));
        let mut client = tls_stream(address, &fixture).await;
        client
            .write_all(b"GET /large HTTP/1.1\r\nHost: aede.test:443\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.connection_slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        // The client deliberately never reads the response. A completed
        // response would return this only after every byte was handed to the
        // kernel; a congested writer must instead be cut off by the transport
        // progress deadline.
        let waiting = Instant::now();
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(state.connection_slots.available_permits(), 0);
        tokio::time::timeout(Duration::from_secs(15), async {
            while state.connection_slots.available_permits() != 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("blocked TLS response is released");
        assert!(
            waiting.elapsed() >= Duration::from_secs(8),
            "the fixture body did not exercise a blocked writer"
        );

        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(client);
    });
}

#[test]
fn upgraded_websocket_keeps_its_tcp_admission_slot_until_it_closes() {
    let runtime = test_support::test_runtime();
    runtime.block_on(async {
        let fixture = CertificateFixture::new();
        let account_fixture = accounts_test_support::Fixture::new();
        let mut state = account_fixture.0.clone();
        let accounts = account_fixture.accounts();
        let (principal, _) =
            auth::start_session(&state, &accounts, accounts.find("operator").unwrap()).unwrap();
        let token = auth::session_token(&principal).to_owned();
        state.connection_slots = Arc::new(Semaphore::new(1));
        let (address, server) = start_tls(state.clone(), &fixture).await;

        let (websocket, _) = open_websocket(address, &fixture, &token).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.connection_slots.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let mut refused = tokio::net::TcpStream::connect(address).await.unwrap();
        if refused.write_all(&[0x16]).await.is_ok() {
            let mut byte = [0_u8; 1];
            let read = tokio::time::timeout(Duration::from_secs(2), refused.read(&mut byte)).await;
            assert!(
                matches!(read, Ok(Ok(0)) | Ok(Err(_))),
                "connection above the live WebSocket cap stayed open"
            );
        }

        state.shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(state.connection_slots.available_permits(), 1);
        drop(websocket);
    });
}

async fn start_tls(
    state: ApiState,
    fixture: &CertificateFixture,
) -> (SocketAddr, tokio::task::JoinHandle<Result<(), io::Error>>) {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let tls = configure(&options(fixture, AUTHORITY)).unwrap().unwrap();
    let application = remote_router(state.clone(), tls.authority.clone());
    let server = tokio::spawn(serve_tls(listener, tls.acceptor, application, state));
    (address, server)
}

async fn tls_stream(
    address: SocketAddr,
    fixture: &CertificateFixture,
) -> tokio_rustls::client::TlsStream<tokio::net::TcpStream> {
    let certificate =
        CertificateDer::from_pem_slice(&fs::read(&fixture.certificate).unwrap()).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate).unwrap();
    let configuration = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let stream = tokio::net::TcpStream::connect(address).await.unwrap();
    tokio_rustls::TlsConnector::from(Arc::new(configuration))
        .connect(
            ServerName::try_from("aede.test").unwrap().to_owned(),
            stream,
        )
        .await
        .unwrap()
}

async fn https_request(address: SocketAddr, fixture: &CertificateFixture, request: &str) -> String {
    let mut stream = tls_stream(address, fixture).await;
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

async fn remote_session(address: SocketAddr, fixture: &CertificateFixture) -> String {
    let origin = format!("https://{AUTHORITY}");
    let body = format!(
        r#"{{"username":"alice","password":"{}"}}"#,
        accounts_test_support::PASSWORD
    );
    let response = https_request(
        address,
        fixture,
        &request(
            "POST",
            "/api/auth/v1/session",
            AUTHORITY,
            Some(&origin),
            Some("application/json"),
            &body,
        ),
    )
    .await;
    assert!(status(&response).starts_with("HTTP/1.1 200"), "{response}");
    let (_, body) = response.split_once("\r\n\r\n").expect("response body");
    serde_json::from_str::<serde_json::Value>(body).expect("session JSON")["token"]
        .as_str()
        .expect("session token")
        .to_owned()
}

enum TlsWebSocketFrame {
    Text(serde_json::Value),
    Binary(Vec<u8>),
    Close,
}

struct TlsWebSocket {
    stream: tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    buffered: Vec<u8>,
}

impl TlsWebSocket {
    async fn send_text(&mut self, text: &str) {
        self.send(1, text.as_bytes()).await;
    }

    async fn send(&mut self, opcode: u8, payload: &[u8]) {
        let mut header = vec![0x80 | opcode];
        match payload.len() {
            0..=125 => header.push(0x80 | payload.len() as u8),
            126..=65_535 => {
                header.push(0x80 | 126);
                header.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            }
            _ => {
                header.push(0x80 | 127);
                header.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            }
        }
        let mask = [0x4a_u8, 0x93, 0x11, 0xce];
        header.extend_from_slice(&mask);
        self.stream.write_all(&header).await.expect("frame header");
        let encoded: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % mask.len()])
            .collect();
        self.stream
            .write_all(&encoded)
            .await
            .expect("frame payload");
    }

    async fn next(&mut self) -> TlsWebSocketFrame {
        let header = self.read_bytes(2).await;
        assert_eq!(header[0] & 0x80, 0x80, "server sends final frames");
        assert_eq!(header[1] & 0x80, 0, "server frames are unmasked");
        let length = match header[1] & 0x7f {
            length @ 0..=125 => usize::from(length),
            126 => usize::from(u16::from_be_bytes(
                self.read_bytes(2).await.try_into().expect("16-bit length"),
            )),
            127 => usize::try_from(u64::from_be_bytes(
                self.read_bytes(8).await.try_into().expect("64-bit length"),
            ))
            .expect("bounded server frame"),
            _ => unreachable!("WebSocket payload length is seven bits"),
        };
        let payload = self.read_bytes(length).await;
        match header[0] & 0x0f {
            1 => TlsWebSocketFrame::Text(
                serde_json::from_slice(&payload).expect("JSON server frame"),
            ),
            2 => TlsWebSocketFrame::Binary(payload),
            8 => TlsWebSocketFrame::Close,
            opcode => panic!("unexpected server WebSocket opcode {opcode}"),
        }
    }

    async fn read_bytes(&mut self, length: usize) -> Vec<u8> {
        while self.buffered.len() < length {
            let mut chunk = [0_u8; 8 * 1024];
            let count = tokio::time::timeout(Duration::from_secs(3), self.stream.read(&mut chunk))
                .await
                .expect("server frame timeout")
                .expect("server frame read");
            assert_ne!(count, 0, "connection closed before server frame");
            self.buffered.extend_from_slice(&chunk[..count]);
        }
        self.buffered.drain(..length).collect()
    }
}

async fn open_playback_websocket(
    address: SocketAddr,
    fixture: &CertificateFixture,
    token: &str,
) -> TlsWebSocket {
    let mut stream = tls_stream(address, fixture).await;
    let request = format!(
        "GET /api/me/v1/playback HTTP/1.1\r\nHost: {AUTHORITY}\r\nOrigin: https://{AUTHORITY}\r\nAuthorization: Bearer {token}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut received = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 1024];
        let count = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut chunk))
            .await
            .expect("upgrade response timeout")
            .expect("upgrade response read");
        assert_ne!(count, 0, "connection closed before upgrade response");
        received.extend_from_slice(&chunk[..count]);
        if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&received[..header_end]);
    assert!(headers.starts_with("HTTP/1.1 101"), "{headers}");
    TlsWebSocket {
        stream,
        buffered: received[header_end..].to_vec(),
    }
}

async fn open_websocket(
    address: SocketAddr,
    fixture: &CertificateFixture,
    token: &str,
) -> (
    tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    Vec<u8>,
) {
    let mut stream = tls_stream(address, fixture).await;
    let request = format!(
        "GET /api/v1/events HTTP/1.1\r\nHost: {AUTHORITY}\r\nOrigin: https://{AUTHORITY}\r\nAuthorization: Bearer {token}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let mut chunk = [0_u8; 1024];
            let read = stream.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "websocket closed before its snapshot");
            response.extend_from_slice(&chunk[..read]);
            if response
                .windows(b"snapshot".len())
                .any(|part| part == b"snapshot")
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    (stream, response)
}

fn request(
    method: &str,
    path: &str,
    host: &str,
    origin: Option<&str>,
    content_type: Option<&str>,
    body: &str,
) -> String {
    let origin = origin
        .map(|origin| format!("Origin: {origin}\r\n"))
        .unwrap_or_default();
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\n{origin}{content_type}Connection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn status(response: &str) -> &str {
    response.lines().next().unwrap_or_default()
}
