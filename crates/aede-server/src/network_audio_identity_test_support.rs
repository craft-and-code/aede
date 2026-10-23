//! A synthetic original and the existing scripted UPnP wire profile on loopback.

use super::*;
use axum::response::IntoResponse;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

pub(super) enum Change {
    None,
    BeforeAdmission,
    AfterAdmission,
}

pub(super) struct Observation {
    pub accepted: bool,
    pub catalog_changed: bool,
    pub original: Vec<u8>,
    pub served: Option<Vec<u8>>,
    pub status: Option<&'static str>,
}

pub(super) fn observe(change: Change) -> Observation {
    // A spawned supervisor cannot receive its first poll between route return
    // and synchronous fixture replacement on this current-thread runtime.
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(observe_async(change))
}

async fn observe_async(change: Change) -> Observation {
    let fixture = crate::accounts_test_support::Fixture::new();
    let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
    let reference = aede_core::user::EntityRef::parse_token(&installed.reference).unwrap();
    let captured = {
        let catalog = fixture.0.catalog.read().await;
        crate::playback_api::source_from_catalog(catalog.as_ref().unwrap(), &reference).unwrap()
    };
    assert!(crate::playback_api::validate_source(&captured).is_ok());
    let original = std::fs::read(&installed.path).unwrap();
    let replacement = crate::playback_test_support::wav_bytes(800);
    let replace = || {
        let stage = installed.path.with_extension("replacement.wav");
        std::fs::write(&stage, &replacement).unwrap();
        std::fs::rename(stage, &installed.path).unwrap();
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accounts = fixture.accounts();
    let (principal, _) =
        auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap()).unwrap();
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/admin/v1/network-audio/play")
        .header(
            "authorization",
            format!("Bearer {}", auth::session_token(&principal)),
        )
        .body(axum::body::Body::from(
            serde_json::json!({
                "bind":"127.0.0.1",
                "device":format!("http://127.0.0.1:{port}/description.xml"),
                "tracks":[installed.reference],
            })
            .to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(principal);
    if matches!(change, Change::BeforeAdmission) {
        replace();
    }
    let result = play(State(fixture.0.clone()), request).await;
    if matches!(change, Change::BeforeAdmission) {
        assert_eq!(result.unwrap_err().status, StatusCode::BAD_REQUEST);
        assert!(fixture.0.network_audio.run.lock().unwrap().is_none());
        assert_eq!(
            listener.into_std().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
        );
        return Observation {
            accepted: false,
            catalog_changed: crate::playback_api::validate_source(&captured).is_err(),
            original,
            served: None,
            status: None,
        };
    }
    assert_eq!(result.unwrap().into_response().status(), StatusCode::OK);
    if matches!(change, Change::AfterAdmission) {
        replace();
    }
    let catalog_changed = crate::playback_api::validate_source(&captured).is_err();
    let (send, mut receive) = tokio::sync::oneshot::channel();
    let renderer = tokio::spawn(renderer(listener, send));
    let served = tokio::time::timeout(Duration::from_secs(4), async {
        tokio::select! {
            uri = &mut receive => Some(read_audio(&uri.unwrap()).await),
            _ = terminal_status(&fixture.0) => None,
        }
    })
    .await;
    // Always release the real controller before checking the regression.
    fixture
        .0
        .network_audio
        .run
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .cancelled
        .store(true, Ordering::Release);
    let status = tokio::time::timeout(Duration::from_secs(4), terminal_status(&fixture.0)).await;
    renderer.abort();
    let _ = renderer.await;
    let served = served.expect("neither original delivery nor transport refusal completed");
    if let Some(bytes) = served.as_ref() {
        assert!(bytes == &original || bytes == &replacement);
    }
    Observation {
        accepted: true,
        catalog_changed,
        original,
        served,
        status: Some(status.expect("network controller did not finish cancellation")),
    }
}

async fn terminal_status(state: &ApiState) -> &'static str {
    loop {
        let status = state
            .network_audio
            .run
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .status;
        if !matches!(status, "running" | "stopping") {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

async fn request(socket: tokio::net::TcpStream) -> (tokio::net::TcpStream, String, String) {
    let mut reader = tokio::io::BufReader::new(socket);
    let mut first = String::new();
    reader.read_line(&mut first).await.unwrap();
    let mut length = 0usize;
    let mut action = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').unwrap();
        if key.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().unwrap();
        }
        if key.eq_ignore_ascii_case("soapaction") {
            action = value
                .trim()
                .trim_matches('"')
                .split('#')
                .nth(1)
                .unwrap()
                .into();
        }
    }
    assert!(length < 32768);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.unwrap();
    if first.starts_with("GET ") {
        action = "GET".into();
    }
    (
        reader.into_inner(),
        action,
        String::from_utf8(body).unwrap(),
    )
}

async fn reply(mut socket: tokio::net::TcpStream, body: &str) {
    let bytes = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    socket.write_all(bytes.as_bytes()).await.unwrap();
    socket.shutdown().await.unwrap();
}

async fn renderer(listener: tokio::net::TcpListener, uri: tokio::sync::oneshot::Sender<String>) {
    let port = listener.local_addr().unwrap().port();
    let mut sender = Some(uri);
    let mut started = false;
    let mut owned_uri = String::new();
    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let (socket, action, payload) = request(socket).await;
        let avt = "urn:schemas-upnp-org:service:AVTransport:1";
        let cm = "urn:schemas-upnp-org:service:ConnectionManager:1";
        let (service, fields) = match action.as_str() {
            "GET" => {
                // Same service shape as the retained devices UPnP fixtures.
                let description = format!(
                    "<?xml version=\"1.0\"?><root xmlns=\"urn:schemas-upnp-org:device-1-0\"><URLBase>http://127.0.0.1:{port}/</URLBase><device><friendlyName>Identity Fixture</friendlyName><serviceList><service><serviceType>{avt}</serviceType><controlURL>/control</controlURL></service><service><serviceType>{cm}</serviceType><controlURL>/connection</controlURL></service></serviceList></device></root>"
                );
                reply(socket, &description).await;
                continue;
            }
            "GetProtocolInfo" => (cm, "<Sink>http-get:*:audio/wav:*</Sink>".into()),
            "GetTransportInfo" => (
                avt,
                format!(
                    "<CurrentTransportState>{}</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus>",
                    if started { "PLAYING" } else { "STOPPED" },
                ),
            ),
            "GetPositionInfo" => (avt, format!("<TrackURI>{owned_uri}</TrackURI>")),
            "SetAVTransportURI" => {
                let value = payload
                    .split("<CurrentURI>")
                    .nth(1)
                    .unwrap()
                    .split("</CurrentURI>")
                    .next()
                    .unwrap();
                owned_uri = value.to_owned();
                sender
                    .take()
                    .unwrap()
                    .send(value.replace("&amp;", "&"))
                    .unwrap();
                (avt, String::new())
            }
            "Play" => {
                started = true;
                (avt, String::new())
            }
            "Stop" => (avt, String::new()),
            other => panic!("unexpected fixture action {other}"),
        };
        let response = format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><u:{action}Response xmlns:u=\"{service}\">{fields}</u:{action}Response></s:Body></s:Envelope>"
        );
        reply(socket, &response).await;
        if action == "Stop" {
            return;
        }
    }
}

async fn read_audio(uri: &str) -> Vec<u8> {
    let raw = uri.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, path) = raw.split_once('/').unwrap();
    let mut socket =
        tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port.parse::<u16>().unwrap()))
            .await
            .unwrap();
    socket
        .write_all(
            format!("GET /{path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.unwrap();
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    response[split + 4..].to_vec()
}
