use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::playback_test_support::{install_wav, wav_bytes};
use crate::test_support::{start_server, test_runtime};
use aede_core::clock;
use aede_core::store;
use aede_core::store_lock::StoreLock;
use aede_core::user;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

struct Socket {
    stream: TcpStream,
    buffered: Vec<u8>,
}

enum ServerFrame {
    Text(serde_json::Value),
    Binary(Vec<u8>),
    Close,
}

fn upgraded_socket(address: SocketAddr, token: &str) -> Socket {
    let (stream, response, buffered) = upgrade(address, token);
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    Socket { stream, buffered }
}

fn upgrade(address: SocketAddr, token: &str) -> (TcpStream, String, Vec<u8>) {
    let mut stream = TcpStream::connect(address).expect("local server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    write!(
        stream,
        "GET /api/me/v1/playback HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    .expect("upgrade request");
    let mut received = Vec::new();
    let header_end = loop {
        let mut chunk = [0_u8; 512];
        let count = stream.read(&mut chunk).expect("upgrade response");
        assert_ne!(count, 0, "connection closed before response headers");
        received.extend_from_slice(&chunk[..count]);
        if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    (
        stream,
        String::from_utf8(received[..header_end].to_vec()).expect("HTTP headers"),
        received[header_end..].to_vec(),
    )
}

impl Socket {
    fn send_text(&mut self, text: &str) {
        self.send(1, text.as_bytes());
    }

    fn close(&mut self) {
        self.send(8, &[]);
    }

    fn send(&mut self, opcode: u8, payload: &[u8]) {
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
        self.stream.write_all(&header).expect("client frame header");
        let encoded: Vec<u8> = payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % mask.len()])
            .collect();
        self.stream
            .write_all(&encoded)
            .expect("client frame payload");
    }

    fn next(&mut self) -> ServerFrame {
        let header = self.read_exact(2);
        assert_eq!(header[0] & 0x80, 0x80, "server uses final frames");
        assert_eq!(header[1] & 0x80, 0, "server frames are unmasked");
        let payload_size = match header[1] & 0x7f {
            size @ 0..=125 => usize::from(size),
            126 => usize::from(u16::from_be_bytes(self.read_exact(2).try_into().unwrap())),
            127 => usize::try_from(u64::from_be_bytes(self.read_exact(8).try_into().unwrap()))
                .expect("bounded server frame"),
            _ => unreachable!("WebSocket payload-length marker is seven bits"),
        };
        let payload = self.read_exact(payload_size);
        match header[0] & 0x0f {
            1 => ServerFrame::Text(serde_json::from_slice(&payload).expect("JSON server frame")),
            2 => ServerFrame::Binary(payload),
            8 => ServerFrame::Close,
            opcode => panic!("unexpected server WebSocket opcode {opcode}"),
        }
    }

    fn read_exact(&mut self, length: usize) -> Vec<u8> {
        while self.buffered.len() < length {
            let mut chunk = [0_u8; 8 * 1024];
            let count = self.stream.read(&mut chunk).expect("server frame");
            assert_ne!(count, 0, "connection closed before server frame");
            self.buffered.extend_from_slice(&chunk[..count]);
        }
        self.buffered.drain(..length).collect()
    }
}

fn start(socket: &mut Socket, reference: &str) {
    socket.send_text(
        &serde_json::json!({
            "type": "start",
            "track": reference,
            "normalize": "off",
        })
        .to_string(),
    );
}

async fn saved_play(fixture: &Fixture) -> Play {
    for _ in 0..80 {
        if let Ok(Some(data)) = user::load(&user::user_path(&fixture.0.data_dir))
            && let Some(play) = data.plays.last()
        {
            return play.clone();
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("playback history was not saved");
}

async fn assert_no_saved_play(fixture: &Fixture) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        user::load(&user::user_path(&fixture.0.data_dir))
            .expect("read history")
            .is_none()
    );
}

fn assert_guarded_pcm(bytes: &[u8]) {
    assert!(!bytes.is_empty());
    assert!(bytes.len() <= MAX_AUDIO_BYTES);
    assert_eq!(bytes.len() % 4, 0);
    for sample in bytes.as_chunks::<4>().0 {
        let value = f32::from_le_bytes(*sample);
        assert!(value.is_finite());
        assert!((-1.0..=1.0).contains(&value));
    }
}

async fn replace_catalog_source(fixture: &Fixture) {
    let path = fixture.0.data_dir.join("replacement.wav");
    std::fs::write(&path, wav_bytes(400)).expect("write replacement WAV");
    let metadata = std::fs::metadata(&path).expect("replacement WAV metadata");
    let path_text = path.to_string_lossy().into_owned();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().expect("fixture catalog");
    let file = catalog.files.first_mut().expect("sample file");
    let old_path = std::mem::replace(&mut file.path, path_text.clone());
    file.size = metadata.len();
    file.mtime = clock::mtime_seconds(&metadata);
    catalog.file_mtime_subseconds.remove(&old_path);
    catalog
        .file_mtime_subseconds
        .insert(path_text, clock::mtime_subseconds(&metadata));
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir))
        .expect("save replacement catalog");
}

#[test]
fn websocket_streams_guarded_pcm_and_records_only_fully_acknowledged_eof() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        // At 8 kHz mono, a 32 KiB transport chunk would hold 8,192 frames.
        // This fixture crosses the one-second (8,000-frame) acknowledgement
        // window, so a producer must split/back-pressure rather than deadlock.
        let installed = install_wav(&fixture, 16_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let owner = fixture
            .accounts()
            .find("alice")
            .expect("alice account")
            .id
            .clone();
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);

        let format = match socket.next() {
            ServerFrame::Text(frame) => frame,
            _ => panic!("format frame"),
        };
        assert_eq!(format["type"], "format");
        assert_eq!(format["encoding"], "f32le");
        assert_eq!(format["sample_rate"], 8_000);
        assert_eq!(format["channels"], 1);
        assert_eq!(format["max_unacknowledged_frames"], 8_000);
        let mut acknowledged = 0_u64;
        // Do not acknowledge the first full window. The server must not put a
        // 16,001st frame on the socket until this acknowledgement arrives.
        while acknowledged < 8_000 {
            let ServerFrame::Binary(bytes) = socket.next() else {
                panic!("PCM block before acknowledgement window fills");
            };
            assert_guarded_pcm(&bytes);
            let frames = u64::try_from(bytes.len() / 4).expect("PCM frame count");
            assert!(acknowledged + frames <= 8_000);
            if acknowledged == 0 {
                let first = f32::from_le_bytes(bytes[..4].try_into().expect("first sample"));
                assert!((first - (8_000.0 / 32_768.0)).abs() < f32::EPSILON);
            }
            acknowledged += frames;
        }
        assert_eq!(acknowledged, 8_000);
        assert!(socket.buffered.is_empty(), "no PCM may bypass the window");
        let (status, _, _) = http(address, "GET", "/api/auth/v1/session", Some(&token), "");
        assert_eq!(
            status, 200,
            "ordinary requests stay responsive while paused"
        );
        socket
            .stream
            .set_read_timeout(Some(Duration::from_millis(750)))
            .expect("short back-pressure timeout");
        let mut probe = [0_u8; 1];
        let failure = socket
            .stream
            .read(&mut probe)
            .expect_err("server must wait for an acknowledgement");
        assert!(matches!(
            failure.kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        ));
        socket
            .stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("restore read timeout");
        socket.send_text(&serde_json::json!({"type": "ack", "frames": acknowledged}).to_string());
        let mut saw_eof = false;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_guarded_pcm(&bytes);
                    assert!(bytes.len() / 4 <= 8_000);
                    acknowledged += u64::try_from(bytes.len() / 4).unwrap();
                    socket.send_text(
                        &serde_json::json!({"type": "ack", "frames": acknowledged}).to_string(),
                    );
                }
                ServerFrame::Text(frame) if frame["type"] == "eof" => {
                    assert_eq!(frame["frames"], acknowledged);
                    saw_eof = true;
                }
                ServerFrame::Text(frame) if frame["type"] == "recorded" => {
                    assert!(saw_eof);
                    assert_eq!(frame["ms_played"], 2_000);
                    assert_eq!(frame["completed"], true);
                    break;
                }
                ServerFrame::Text(frame) => panic!("unexpected text frame {frame}"),
                ServerFrame::Close => panic!("closed before listen was recorded"),
            }
        }
        let play = saved_play(&fixture).await;
        assert_eq!(play.track.to_token(), installed.reference);
        assert_eq!(play.ms_played, 2_000);
        assert!(play.completed);
        let history = user::load(&user::user_path(&fixture.0.data_dir))
            .expect("read history")
            .expect("saved history");
        assert_eq!(history.plays.len(), 1);
        assert_eq!(history.plays[0].owner, owner);
        assert_eq!(history.play_count(&owner, &play.track), 1);
        server.abort();
    });
}

#[test]
fn invalid_acknowledgement_stops_the_websocket_without_recording_a_listen() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 400).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);
        assert!(matches!(socket.next(), ServerFrame::Text(_)));
        let frames = match socket.next() {
            ServerFrame::Binary(bytes) => u64::try_from(bytes.len() / 4).unwrap(),
            _ => panic!("PCM block"),
        };
        socket.send_text(&serde_json::json!({"type": "ack", "frames": frames + 1}).to_string());
        loop {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "invalid_ack");
                    break;
                }
                ServerFrame::Text(_) | ServerFrame::Binary(_) => {}
                ServerFrame::Close => panic!("closed before invalid acknowledgement error"),
            }
        }
        assert!(
            user::load(&user::user_path(&fixture.0.data_dir))
                .expect("read history")
                .is_none()
        );
        server.abort();
    });
}

#[test]
fn partial_acknowledgement_records_an_incomplete_listen_when_the_client_closes() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 400).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);
        assert!(matches!(socket.next(), ServerFrame::Text(_)));
        assert!(matches!(socket.next(), ServerFrame::Binary(_)));
        socket.send_text(r#"{"type":"ack","frames":8}"#);
        // Let the server consume the ordered acknowledgement before the close
        // is sent on this raw TCP test transport.
        tokio::time::sleep(Duration::from_millis(50)).await;
        // A watcher or a concurrent personal-data write may briefly own the
        // store lock exactly when a client closes. The bounded retry keeps the
        // valid partial listen rather than dropping it immediately.
        let lock = StoreLock::acquire(&fixture.0.data_dir).expect("hold store lock");
        socket.close();
        drop(socket);
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(lock);

        let play = saved_play(&fixture).await;
        assert_eq!(play.track.to_token(), installed.reference);
        assert_eq!(play.ms_played, 1);
        assert!(!play.completed);
        server.abort();
    });
}

#[test]
fn revocation_after_a_partial_acknowledgement_stops_streaming_and_drops_the_listen() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 16_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);
        assert!(matches!(socket.next(), ServerFrame::Text(_)));
        assert!(matches!(socket.next(), ServerFrame::Binary(_)));
        socket.send_text(r#"{"type":"ack","frames":8}"#);
        let (status, _, _) = http(address, "DELETE", "/api/auth/v1/session", Some(&token), "");
        assert_eq!(status, 204);
        loop {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "authentication_expired");
                    break;
                }
                ServerFrame::Text(_) | ServerFrame::Binary(_) => {}
                ServerFrame::Close => panic!("closed before revocation error"),
            }
        }
        assert_no_saved_play(&fixture).await;
        server.abort();
    });
}

#[test]
fn replacement_catalog_source_is_not_attributed_after_a_partial_acknowledgement() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 400).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);
        assert!(matches!(socket.next(), ServerFrame::Text(_)));
        assert!(matches!(socket.next(), ServerFrame::Binary(_)));
        socket.send_text(r#"{"type":"ack","frames":8}"#);
        replace_catalog_source(&fixture).await;
        socket.close();
        drop(socket);

        assert_no_saved_play(&fixture).await;
        server.abort();
    });
}

#[test]
fn auditor_and_changed_sources_are_refused_before_pcm_is_sent() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 400).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let auditor = login(address, "auditor");
        let (_, response, _) = upgrade(address, &auditor);
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");

        std::fs::write(&installed.path, [0_u8; 12]).expect("replace source");
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &installed.reference);
        match socket.next() {
            ServerFrame::Text(frame) => {
                assert_eq!(frame["type"], "error");
                assert_eq!(frame["code"], "source_changed");
            }
            _ => panic!("source change error"),
        }
        assert!(
            user::load(&user::user_path(&fixture.0.data_dir))
                .expect("read history")
                .is_none()
        );
        server.abort();
    });
}

#[test]
fn acknowledgements_cannot_regress_or_exceed_the_sent_window() {
    let now = Instant::now();
    let mut acknowledgements = Acknowledgements::with_window(10);
    acknowledgements.sent(10, now).expect("sent frames");
    assert_eq!(acknowledgements.available(), 0);
    assert!(
        acknowledgements
            .acknowledge(5, now)
            .expect("partial acknowledgement")
    );
    assert!(
        !acknowledgements
            .acknowledge(5, now)
            .expect("duplicate acknowledgement")
    );
    assert_eq!(acknowledgements.available(), 5);
    assert_eq!(
        acknowledgements.acknowledge(4, now).unwrap_err().code,
        "invalid_ack"
    );
    assert_eq!(
        acknowledgements.acknowledge(11, now).unwrap_err().code,
        "invalid_ack"
    );
    assert_eq!(frames_to_milliseconds(7, 8_000), 0);
    assert_eq!(frames_to_milliseconds(8, 8_000), 1);
}
