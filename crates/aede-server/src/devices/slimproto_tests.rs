use super::*;

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;

use aede_core::tags::AudioProperties;
use tokio::io::duplex;
use tokio::task::JoinHandle;

fn check(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}

fn track(codec: &str, container: &str, mime: &'static str) -> DeviceTrack {
    DeviceTrack {
        path: PathBuf::from("/library/example.flac"),
        title: "Example".into(),
        mime,
        properties: AudioProperties {
            codec: codec.into(),
            container: container.into(),
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            duration_ms: Some(1000),
            lossless: codec != "mp3",
            ..AudioProperties::default()
        },
    }
}

fn hello(capabilities: &[u8]) -> Packet {
    let mut payload = vec![0; 36];
    payload[0] = 12;
    payload.extend_from_slice(capabilities);
    Packet {
        opcode: *b"HELO",
        payload,
    }
}

fn stat(event: &[u8; 4], output_bytes: u32) -> Packet {
    let mut payload = vec![0; STAT_BYTES];
    payload[..4].copy_from_slice(event);
    payload[33..37].copy_from_slice(&output_bytes.to_be_bytes());
    Packet {
        opcode: *b"STAT",
        payload,
    }
}

fn encoded(packet: &Packet) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&packet.opcode);
    bytes.extend_from_slice(&(packet.payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&packet.payload);
    bytes
}

async fn send_packet(client: &mut TcpStream, packet: Packet) {
    client.write_all(&encoded(&packet)).await.unwrap();
}

async fn server_frame(client: &mut TcpStream) -> ([u8; 4], Vec<u8>) {
    let mut length = [0; 2];
    timeout(Duration::from_secs(2), client.read_exact(&mut length))
        .await
        .unwrap()
        .unwrap();
    let mut bytes = vec![0; u16::from_be_bytes(length) as usize];
    client.read_exact(&mut bytes).await.unwrap();
    assert!(bytes.len() >= 4);
    (
        [bytes[0], bytes[1], bytes[2], bytes[3]],
        bytes[4..].to_vec(),
    )
}

async fn start(
    tracks: Vec<DeviceTrack>,
    limits: Limits,
) -> (
    TcpStream,
    watch::Sender<bool>,
    JoinHandle<Result<(), String>>,
) {
    start_with_volume(tracks, None, limits).await
}

async fn start_with_volume(
    tracks: Vec<DeviceTrack>,
    volume: Option<u8>,
    limits: Limits,
) -> (
    TcpStream,
    watch::Sender<bool>,
    JoinHandle<Result<(), String>>,
) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, receiver) = watch::channel(false);
    let urls = tracks
        .iter()
        .enumerate()
        .map(|(index, _)| format!("http://127.0.0.1:8123/private-token/{index}"))
        .collect::<Vec<_>>();
    let server = tokio::spawn(async move {
        run_with_limits(
            listener,
            Ipv4Addr::LOCALHOST,
            &tracks,
            &urls,
            receiver,
            SessionOptions { volume, limits },
        )
        .await
    });
    let client = TcpStream::connect(address).await.unwrap();
    (client, shutdown, server)
}

async fn ready(client: &mut TcpStream) -> Vec<u8> {
    send_packet(
        client,
        hello(b"Model=squeezelite,flc,mp3,wav,MaxSampleRate=192000"),
    )
    .await;
    assert_eq!(
        server_frame(client).await,
        (*b"strm", control_payload(b'q'))
    );
    assert_eq!(server_frame(client).await, (*b"aude", vec![1, 1]));
    let (opcode, payload) = server_frame(client).await;
    assert_eq!(opcode, *b"strm");
    assert_eq!(payload[0], b's');
    payload
}

async fn complete(client: &mut TcpStream) {
    send_packet(client, stat(b"STMs", 100)).await;
    send_packet(client, stat(b"STMd", 100)).await;
    send_packet(client, stat(b"STMu", 0)).await;
}

#[test]
fn fragmented_hello_starts_original_audio_and_queue_waits_for_drained_output() {
    check(async {
        let (mut client, _shutdown, server) = start(
            vec![
                track("flac", "flac", "audio/flac"),
                track("mp3", "mp3", "audio/mpeg"),
            ],
            Limits::default(),
        )
        .await;
        let bytes = encoded(&hello(b"Model=squeezelite,flc,mp3,MaxSampleRate=96000"));
        for fragment in bytes.chunks(3) {
            client.write_all(fragment).await.unwrap();
            tokio::task::yield_now().await;
        }
        assert_eq!(
            server_frame(&mut client).await,
            (*b"strm", control_payload(b'q'))
        );
        assert_eq!(server_frame(&mut client).await, (*b"aude", vec![1, 1]));
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(&payload[..7], b"s1f????");
        assert_eq!(payload[7], 1);
        assert_eq!(payload[10], b'0');
        assert_eq!(
            &payload[14..18],
            &[0; 4],
            "no replay gain or volume override"
        );
        assert_eq!(u16::from_be_bytes([payload[18], payload[19]]), 8123);
        assert_eq!(&payload[20..24], &[127, 0, 0, 1]);
        assert_eq!(
            &payload[24..],
            b"GET /private-token/0 HTTP/1.0\r\nHost: 127.0.0.1:8123\r\nConnection: close\r\n\r\n"
        );

        send_packet(&mut client, stat(b"STMs", 100)).await;
        send_packet(
            &mut client,
            Packet {
                opcode: *b"DSCO",
                payload: vec![0],
            },
        )
        .await;
        send_packet(&mut client, stat(b"STMd", 100)).await;
        send_packet(&mut client, stat(b"STMu", 100)).await;
        let mut next = [0; 1];
        assert!(
            timeout(Duration::from_millis(40), client.read(&mut next))
                .await
                .is_err(),
            "network or decoder EOF must not skip buffered audio"
        );
        send_packet(&mut client, stat(b"STMu", 0)).await;
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(&payload[..7], b"s1m????");
        assert!(payload[24..].starts_with(b"GET /private-token/1 HTTP/1.0\r\n"));
        complete(&mut client).await;
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(payload[0], b'q');
        assert!(server.await.unwrap().is_ok());
    });
}

#[test]
fn wav_parameters_are_exact_and_unadvertised_or_incompatible_sources_are_refused() {
    let capabilities = Capabilities::parse(&hello(b"flc,mp3,wav,MaxSampleRate=96000")).unwrap();
    let mut source = track("pcm", "wav", "audio/wav");
    source.properties.sample_rate = Some(96_000);
    source.properties.channels = Some(1);
    source.properties.bit_depth = Some(24);
    let payload = start_payload(&source, "http://192.168.1.2:8123/audio", &capabilities).unwrap();
    assert_eq!(&payload[..7], b"s1p2911");
    assert_eq!(&payload[20..24], &[192, 168, 1, 2]);

    for (codec, container, mime) in [
        ("pcm_float", "wav", "audio/wav"),
        ("flac", "ogg", "audio/flac"),
        ("aac", "mp4", "audio/mp4"),
        ("mp3", "mp3", "audio/flac"),
    ] {
        assert!(
            start_payload(
                &track(codec, container, mime),
                "http://127.0.0.1:8123/audio",
                &capabilities
            )
            .is_err()
        );
    }
    source.properties.sample_rate = Some(192_000);
    assert!(
        start_payload(&source, "http://127.0.0.1:8123/audio", &capabilities)
            .unwrap_err()
            .contains("advertised limit")
    );
    let standard = Capabilities::parse(&hello(b"wav")).unwrap();
    source.properties.sample_rate = Some(96_000);
    assert!(start_payload(&source, "http://127.0.0.1:8123/audio", &standard).is_err());
    source.properties.sample_rate = Some(44_100);
    source.properties.bit_depth = Some(20);
    assert!(start_payload(&source, "http://127.0.0.1:8123/audio", &capabilities).is_err());
    let mut high_precision = track("flac", "flac", "audio/flac");
    high_precision.properties.bit_depth = Some(32);
    assert!(
        start_payload(
            &high_precision,
            "http://127.0.0.1:8123/audio",
            &capabilities
        )
        .unwrap_err()
        .contains("at most 24 bits")
    );
}

#[test]
fn whole_queue_codec_preflight_prevents_partial_playback() {
    check(async {
        let (mut client, _shutdown, server) = start_with_volume(
            vec![
                track("flac", "flac", "audio/flac"),
                track("mp3", "mp3", "audio/mpeg"),
            ],
            Some(20),
            Limits::default(),
        )
        .await;
        send_packet(&mut client, hello(b"flc,MaxSampleRate=96000")).await;
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(
            payload[0], b'q',
            "no earlier occurrence may start before all codecs are accepted"
        );
        assert!(
            server
                .await
                .unwrap()
                .unwrap_err()
                .contains("not advertised")
        );
    });
}

#[test]
fn explicit_linear_volume_covers_mute_and_unity_without_changing_unselected_volume() {
    check(async {
        for (volume, expected_gain, legacy_gain) in
            [(0, 0_u32, 0_u32), (20, 13_107, 26), (100, 65_536, 128)]
        {
            let (mut client, _shutdown, server) = start_with_volume(
                vec![track("flac", "flac", "audio/flac")],
                Some(volume),
                Limits::default(),
            )
            .await;
            send_packet(&mut client, hello(b"flc,MaxSampleRate=96000")).await;
            assert_eq!(
                server_frame(&mut client).await,
                (*b"strm", control_payload(b'q'))
            );
            let (opcode, payload) = server_frame(&mut client).await;
            assert_eq!(opcode, *b"audg");
            assert_eq!(payload.len(), 18);
            assert_eq!(&payload[..4], &legacy_gain.to_be_bytes());
            assert_eq!(&payload[4..8], &legacy_gain.to_be_bytes());
            assert_eq!(&payload[8..10], &[1, 0]);
            assert_eq!(&payload[10..14], &expected_gain.to_be_bytes());
            assert_eq!(&payload[14..18], &expected_gain.to_be_bytes());
            assert_eq!(server_frame(&mut client).await, (*b"aude", vec![1, 1]));
            let (opcode, payload) = server_frame(&mut client).await;
            assert_eq!(opcode, *b"strm");
            assert_eq!(payload[0], b's');
            complete(&mut client).await;
            let (_, payload) = server_frame(&mut client).await;
            assert_eq!(payload[0], b'q');
            assert!(server.await.unwrap().is_ok());
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (_shutdown, receiver) = watch::channel(false);
        let result = run_with_limits(
            listener,
            Ipv4Addr::LOCALHOST,
            &[track("flac", "flac", "audio/flac")],
            &["http://127.0.0.1:8123/audio".into()],
            receiver,
            SessionOptions {
                volume: Some(101),
                limits: Limits::default(),
            },
        )
        .await;
        assert!(result.unwrap_err().contains("between 0 and 100"));
    });
}

#[test]
fn malformed_caps_and_media_urls_do_not_become_protocol_instructions() {
    for caps in [
        b"flc,MaxSampleRate=not-a-rate".as_slice(),
        b"flc,MaxSampleRate=96000,MaxSampleRate=48000",
        b"flc\nModel=spoofed",
        b"flc\0",
        b"flc,\xff",
    ] {
        assert!(Capabilities::parse(&hello(caps)).is_err());
    }
    assert!(
        Capabilities::parse(&Packet {
            opcode: *b"HELO",
            payload: vec![0; 35]
        })
        .is_err()
    );
    assert!(
        Capabilities::parse(&Packet {
            opcode: *b"STAT",
            payload: vec![0; 53]
        })
        .is_err()
    );
    for url in [
        "https://127.0.0.1:8123/audio",
        "http://host.invalid:8123/audio",
        "http://127.0.0.1/audio",
        "http://127.0.0.1:0/audio",
        "http://127.0.0.1:8123/audio\r\nInjected: true",
        "http://127.0.0.1:8123/audio#fragment",
    ] {
        assert!(media_address(url).is_err(), "{url:?}");
    }
    assert!(media_address(&format!("http://127.0.0.1:8123/{}", "x".repeat(2049))).is_err());
}

#[test]
fn incoming_payload_bounds_and_truncated_frames_are_refused_before_use() {
    check(async {
        let (mut sender, mut receiver) = duplex(32);
        let mut oversized = b"HELO".to_vec();
        oversized.extend_from_slice(&(MAX_PAYLOAD as u32 + 1).to_be_bytes());
        sender.write_all(&oversized).await.unwrap();
        assert!(
            read_packet(&mut receiver)
                .await
                .unwrap_err()
                .contains("exceeds 4096")
        );
        let (mut sender, mut receiver) = duplex(32);
        sender.write_all(b"STAT\0\0\0\x35STMs").await.unwrap();
        drop(sender);
        assert!(
            read_packet(&mut receiver)
                .await
                .unwrap_err()
                .contains("truncated")
        );
    });
}

#[test]
fn heartbeat_does_not_discard_a_partially_received_status_packet() {
    check(async {
        let limits = Limits {
            heartbeat: Duration::from_millis(15),
            ..Limits::default()
        };
        let (mut client, _shutdown, server) =
            start(vec![track("flac", "flac", "audio/flac")], limits).await;
        ready(&mut client).await;
        let bytes = encoded(&stat(b"STMs", 100));
        client.write_all(&bytes[..6]).await.unwrap();
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(payload[0], b't');
        client.write_all(&bytes[6..]).await.unwrap();
        send_packet(&mut client, stat(b"STMd", 100)).await;
        send_packet(&mut client, stat(b"STMu", 0)).await;
        loop {
            let (opcode, payload) = server_frame(&mut client).await;
            assert_eq!(opcode, *b"strm");
            if payload[0] == b'q' {
                break;
            }
            assert_eq!(payload[0], b't');
        }
        assert!(server.await.unwrap().is_ok());
    });
}

#[test]
fn ready_or_fully_decoded_short_sources_are_started_without_skipping_their_output() {
    check(async {
        for ready_event in [b"STMl", b"STMd"] {
            let (mut client, _shutdown, server) =
                start(vec![track("flac", "flac", "audio/flac")], Limits::default()).await;
            ready(&mut client).await;
            send_packet(&mut client, stat(ready_event, 100)).await;
            let (opcode, payload) = server_frame(&mut client).await;
            assert_eq!(opcode, *b"strm");
            assert_eq!(payload[0], b'u');
            send_packet(&mut client, stat(b"STMs", 100)).await;
            if ready_event == b"STMl" {
                send_packet(&mut client, stat(b"STMd", 100)).await;
            }
            send_packet(&mut client, stat(b"STMu", 0)).await;
            let (_, payload) = server_frame(&mut client).await;
            assert_eq!(payload[0], b'q');
            assert!(server.await.unwrap().is_ok());
        }
    });
}

#[test]
fn cancellation_sends_stop_instead_of_leaving_player_output_running() {
    check(async {
        let (mut client, shutdown, server) =
            start(vec![track("flac", "flac", "audio/flac")], Limits::default()).await;
        ready(&mut client).await;
        shutdown.send(true).unwrap();
        let (opcode, payload) = server_frame(&mut client).await;
        assert_eq!(opcode, *b"strm");
        assert_eq!(payload[0], b'q');
        assert!(server.await.unwrap().is_ok());
    });
}

#[test]
fn failed_decode_download_and_short_status_stop_the_finite_queue() {
    check(async {
        for packet in [
            stat(b"STMn", 0),
            stat(b"STMo", 0),
            Packet {
                opcode: *b"DSCO",
                payload: vec![3],
            },
            Packet {
                opcode: *b"RESP",
                payload: b"HTTP/1.0 403 Forbidden\r\n\r\n".to_vec(),
            },
            Packet {
                opcode: *b"STAT",
                payload: b"STMu".to_vec(),
            },
        ] {
            let (mut client, _shutdown, server) =
                start(vec![track("flac", "flac", "audio/flac")], Limits::default()).await;
            ready(&mut client).await;
            send_packet(&mut client, packet).await;
            let (opcode, payload) = server_frame(&mut client).await;
            assert_eq!(opcode, *b"strm");
            assert_eq!(payload[0], b'q');
            assert!(server.await.unwrap().is_err());
        }
    });
}

#[test]
fn player_disconnect_and_missing_status_fail_without_claiming_completion() {
    check(async {
        let (mut client, _shutdown, server) =
            start(vec![track("flac", "flac", "audio/flac")], Limits::default()).await;
        ready(&mut client).await;
        drop(client);
        assert!(
            timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .contains("disconnected")
        );

        let limits = Limits {
            packet: Duration::from_millis(200),
            ..Limits::default()
        };
        let (mut client, _shutdown, server) =
            start(vec![track("flac", "flac", "audio/flac")], limits).await;
        ready(&mut client).await;
        let (_, payload) = server_frame(&mut client).await;
        assert_eq!(payload[0], b'q');
        assert!(
            server
                .await
                .unwrap()
                .unwrap_err()
                .contains("stopped responding")
        );
    });
}

#[test]
fn handshakes_connections_and_completion_all_have_independent_deadlines() {
    check(async {
        let limits = Limits {
            hello: Duration::from_millis(150),
            ..Limits::default()
        };
        let (mut client, _shutdown, server) =
            start(vec![track("flac", "flac", "audio/flac")], limits).await;
        let (_, payload) = server_frame(&mut client).await;
        assert_eq!(payload[0], b'q');
        assert!(
            server
                .await
                .unwrap()
                .unwrap_err()
                .contains("HELO timed out")
        );

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let (_shutdown, receiver) = watch::channel(false);
        let limits = Limits {
            connection: Duration::from_millis(150),
            ..Limits::default()
        };
        let result = run_with_limits(
            listener,
            Ipv4Addr::LOCALHOST,
            &[track("flac", "flac", "audio/flac")],
            &["http://127.0.0.1:8123/audio".into()],
            receiver,
            SessionOptions {
                volume: None,
                limits,
            },
        )
        .await;
        assert!(result.unwrap_err().contains("connection timed out"));

        let mut short = track("flac", "flac", "audio/flac");
        short.properties.duration_ms = Some(1);
        let limits = Limits {
            completion_grace: Duration::from_millis(200),
            ..Limits::default()
        };
        let (mut client, _shutdown, server) = start(vec![short], limits).await;
        ready(&mut client).await;
        let (_, payload) = server_frame(&mut client).await;
        assert_eq!(payload[0], b'q');
        assert!(
            server
                .await
                .unwrap()
                .unwrap_err()
                .contains("playback budget")
        );
    });
}

#[test]
fn another_address_cannot_take_over_the_selected_player() {
    check(async {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, receiver) = watch::channel(false);
        let server = tokio::spawn(async move {
            run_with_limits(
                listener,
                Ipv4Addr::new(127, 0, 0, 2),
                &[track("flac", "flac", "audio/flac")],
                &["http://127.0.0.1:8123/audio".into()],
                receiver,
                SessionOptions {
                    volume: None,
                    limits: Limits::default(),
                },
            )
            .await
        });
        let mut wrong = TcpStream::connect(address).await.unwrap();
        assert_eq!(
            wrong.local_addr().unwrap().ip(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)).ip()
        );
        let mut byte = [0; 1];
        assert_eq!(
            timeout(Duration::from_secs(2), wrong.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        shutdown.send(true).unwrap();
        assert!(server.await.unwrap().is_ok());
    });
}
