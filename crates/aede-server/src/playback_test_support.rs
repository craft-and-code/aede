//! Test-only WAV catalog and WebSocket fixtures shared by playback transport tests.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use aede_core::clock;
use aede_core::model::EntityKind;
use aede_core::store;
use aede_core::tags::AudioProperties;
use aede_core::user::EntityRef;

use crate::accounts_test_support::Fixture;

pub(super) struct InstalledTrack {
    pub(super) reference: String,
    pub(super) path: PathBuf,
}

pub(super) async fn install_wav(fixture: &Fixture, frames: u32) -> InstalledTrack {
    let path = fixture.0.data_dir.join("remote-playback.wav");
    std::fs::write(&path, wav_bytes(frames)).expect("write WAV fixture");
    let metadata = std::fs::metadata(&path).expect("WAV metadata");
    let path_text = path.to_string_lossy().into_owned();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().expect("fixture catalog");
    let file = catalog.files.first_mut().expect("sample file");
    file.path = path_text.clone();
    file.size = metadata.len();
    file.mtime = clock::mtime_seconds(&metadata);
    file.properties = AudioProperties {
        codec: "pcm".into(),
        container: "wav".into(),
        sample_rate: Some(8_000),
        bit_depth: Some(16),
        channels: Some(1),
        duration_ms: Some(u64::from(frames) * 1_000 / 8_000),
        bitrate_kbps: Some(128),
        lossless: true,
    };
    catalog
        .file_mtime_subseconds
        .insert(path_text, clock::mtime_subseconds(&metadata));
    let reference = EntityRef::of(catalog, EntityKind::Track, 0)
        .expect("fixture track reference")
        .to_token();
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir))
        .expect("save fixture catalog");
    InstalledTrack { reference, path }
}

pub(super) fn wav_bytes(frames: u32) -> Vec<u8> {
    let bytes_per_frame = 2_u32;
    let data_size = frames * bytes_per_frame;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&8_000_u32.to_le_bytes());
    bytes.extend_from_slice(&(8_000_u32 * bytes_per_frame).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..frames {
        let sample = if index.is_multiple_of(2) {
            8_000_i16
        } else {
            -8_000_i16
        };
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

pub(super) struct Socket {
    pub(super) stream: TcpStream,
    pub(super) buffered: Vec<u8>,
}

pub(super) enum ServerFrame {
    Text(serde_json::Value),
    Binary(Vec<u8>),
    Close,
}

pub(super) fn upgraded_socket(address: SocketAddr, token: &str) -> Socket {
    let (stream, response, buffered) = upgrade(address, token);
    assert!(response.starts_with("HTTP/1.1 101"), "{response}");
    Socket { stream, buffered }
}

pub(super) fn upgrade(address: SocketAddr, token: &str) -> (TcpStream, String, Vec<u8>) {
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
    pub(super) fn send_text(&mut self, text: &str) {
        self.send(1, text.as_bytes());
    }

    pub(super) fn close(&mut self) {
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

    pub(super) fn next(&mut self) -> ServerFrame {
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

pub(super) fn assert_guarded_pcm(bytes: &[u8]) {
    assert!(!bytes.is_empty());
    assert!(bytes.len() <= 32 * 1024);
    assert_eq!(bytes.len() % 4, 0);
    for sample in bytes.as_chunks::<4>().0 {
        let value = f32::from_le_bytes(*sample);
        assert!(value.is_finite());
        assert!((-1.0..=1.0).contains(&value));
    }
}
