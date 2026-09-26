//! Shared, test-only catalog and HTTP fixtures.

use super::*;
use aede_core::model::{
    Artist, AudioFile, Genre, GenreLink, Label, Recording, Release, ReleaseGroup, Track, Work,
};
use std::io::{Read, Write};

pub(crate) fn sample_state() -> ApiState {
    let catalog = Catalog {
        scanned_at: 1_700_000_000,
        artists: vec![Artist {
            id: 0,
            name: "AC/DC".into(),
            sort_name: "AC/DC".into(),
            key: "ac/dc".into(),
            ..Default::default()
        }],
        files: vec![AudioFile {
            id: 0,
            path: "/music/album/01.flac".into(),
            size: 42,
            ..Default::default()
        }],
        releases: vec![Release {
            id: 0,
            title: "Back in Black".into(),
            album_artist_id: Some(0),
            release_group_id: Some(0),
            label_ids: vec![0],
            track_ids: vec![0],
            ..Default::default()
        }],
        release_groups: vec![ReleaseGroup {
            id: 0,
            title: "Back in Black".into(),
            mbid: "group-1".into(),
            release_ids: vec![0],
            ..Default::default()
        }],
        tracks: vec![Track {
            id: 0,
            file_id: 0,
            release_id: Some(0),
            recording_id: 0,
            title: "Hells Bells".into(),
            ..Default::default()
        }],
        recordings: vec![Recording {
            id: 0,
            title: "Hells Bells".into(),
            mbid: Some("recording-1".into()),
            track_ids: vec![0],
            work_ids: vec![0],
            ..Default::default()
        }],
        works: vec![Work {
            id: 0,
            title: "Hells Bells".into(),
            mbid: "work-1".into(),
            recording_ids: vec![0],
            ..Default::default()
        }],
        labels: vec![Label {
            id: 0,
            name: "Atlantic".into(),
            key: "atlantic".into(),
            ..Default::default()
        }],
        genres: vec![Genre {
            id: 0,
            name: "Rock".into(),
            key: "rock".into(),
        }],
        genre_links: vec![GenreLink {
            genre_id: 0,
            entity_kind: EntityKind::Release,
            entity_id: 0,
        }],
        ..Default::default()
    };
    let (events, _) = broadcast::channel(8);
    let (shutdown, _) = broadcast::channel(1);
    ApiState {
        data_dir: std::env::temp_dir(),
        catalog: Arc::new(RwLock::new(Some(catalog))),
        loaded_stamp: Arc::new(RwLock::new(None)),
        reload_gate: Arc::new(Mutex::new(())),
        events,
        shutdown,
        admin: None,
        next_task_id: Arc::new(AtomicU64::new(1)),
        websocket_slots: Arc::new(Semaphore::new(MAX_WEBSOCKETS)),
        inspection_slots: Arc::new(Semaphore::new(2)),
        jobs: Arc::new(jobs::JobRegistry::default()),
        #[cfg(unix)]
        tasks: Arc::new(delegation::TaskRegistry::default()),
    }
}

pub(crate) fn json_response(address: std::net::SocketAddr, path: &str) -> serde_json::Value {
    let response = request(address, path);
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let (_, body) = response.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(body).unwrap()
}

pub(crate) fn request(address: std::net::SocketAddr, path: &str) -> String {
    request_method(address, "GET", path)
}

pub(crate) fn request_method(address: std::net::SocketAddr, method: &str, path: &str) -> String {
    request_with_headers(address, method, path, "")
}

pub(crate) fn request_with_headers(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &str,
) -> String {
    let mut stream = std::net::TcpStream::connect(address).expect("local server");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{headers}\r\n"
    )
    .expect("request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("response");
    answer
}

pub(crate) fn response_headers(address: SocketAddr, request: &str) -> String {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut received = Vec::new();
    loop {
        let mut chunk = [0; 512];
        let count = stream.read(&mut chunk).unwrap();
        assert_ne!(count, 0, "connection closed before response headers");
        received.extend_from_slice(&chunk[..count]);
        if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
            return String::from_utf8(received[..end].to_vec()).unwrap();
        }
    }
}

pub(crate) struct WebSocketReader {
    pub(crate) stream: std::net::TcpStream,
    buffered: Vec<u8>,
}

pub(crate) fn websocket(address: std::net::SocketAddr, path: &str) -> WebSocketReader {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
    .unwrap();
    let mut received = Vec::new();
    let header_end = loop {
        let mut chunk = [0u8; 512];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "connection closed during handshake");
        received.extend_from_slice(&chunk[..read]);
        if let Some(end) = received.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let header = String::from_utf8_lossy(&received[..header_end]);
    assert!(header.starts_with("HTTP/1.1 101"), "{header}");
    WebSocketReader {
        stream,
        buffered: received[header_end..].to_vec(),
    }
}

pub(crate) fn websocket_event(reader: &mut WebSocketReader) -> serde_json::Value {
    loop {
        if reader.buffered.len() >= 2 {
            assert_eq!(reader.buffered[0] & 0x0f, 1, "a text frame is sent");
            let small_length = reader.buffered[1] & 0x7f;
            assert_eq!(reader.buffered[1] & 0x80, 0, "server frames are unmasked");
            let (header_len, payload_len) = match small_length {
                0..=125 => (2, usize::from(small_length)),
                126 if reader.buffered.len() >= 4 => (
                    4,
                    usize::from(u16::from_be_bytes([reader.buffered[2], reader.buffered[3]])),
                ),
                _ => (0, 0),
            };
            if header_len > 0 && reader.buffered.len() >= header_len + payload_len {
                let payload = reader.buffered[header_len..header_len + payload_len].to_vec();
                reader.buffered.drain(..header_len + payload_len);
                return serde_json::from_slice(&payload).unwrap();
            }
        }
        let mut chunk = [0u8; 512];
        let read = reader.stream.read(&mut chunk).unwrap();
        assert!(read > 0, "connection closed before event");
        reader.buffered.extend_from_slice(&chunk[..read]);
    }
}

pub(crate) fn encoded(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("%{byte:02X}"))
        .collect()
}
