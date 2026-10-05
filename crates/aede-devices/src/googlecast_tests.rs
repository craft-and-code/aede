use super::*;

#[path = "cast_test_support.rs"]
mod fixtures;
use fixtures::{Scenario, receiver};

#[tokio::test]
async fn a_pinned_tls_receiver_fetches_exact_original_bytes_without_touching_the_source() {
    use std::sync::Arc;
    let file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/playback_fixtures/flac/playback-stereo.flac")
        .canonicalize()
        .unwrap();
    let original = std::fs::read(&file).unwrap();
    let sources = crate::media::prepare(vec![file.clone()]).unwrap();
    let tracks = sources
        .iter()
        .map(|source| source.track.clone())
        .collect::<Vec<_>>();
    let media = crate::media::MediaServer::start(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, sources)
        .await
        .unwrap();
    let (cert, key) = crate::test_support::tls_material();
    let hash = rustls::crypto::ring::cipher_suite::TLS13_AES_128_GCM_SHA256
        .tls13()
        .unwrap()
        .common
        .hash_provider
        .hash(cert.as_ref());
    let pin = hash
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let expected = original.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(stream).await.unwrap();
        fixtures::receiver_with_original(tls, Scenario::Finish, None, Some(expected)).await
    });
    let (_stop, signal) = watch::channel(false);
    run(
        Ipv4Addr::LOCALHOST,
        &endpoint,
        &pin,
        &tracks,
        media.urls(),
        false,
        signal,
    )
    .await
    .unwrap();
    assert!(
        server
            .await
            .unwrap()
            .iter()
            .any(|event| event.starts_with("LOAD:"))
    );
    media.stop().await;
    assert_eq!(std::fs::read(file).unwrap(), original);
}

#[tokio::test]
async fn partial_frames_survive_poll_deadlines() {
    use tokio::io::AsyncWriteExt;
    let (stream, mut peer) = tokio::io::duplex(4096);
    let mut controller = Controller::new(stream, "sender-test".into());
    controller.application = Some("web-test".into());
    let packet = cast_wire::encode(&Message {
        source: "web-test".into(),
        destination: "sender-test".into(),
        namespace: MEDIA.into(),
        payload: json!({"type":"MEDIA_STATUS", "status":[]}),
    })
    .unwrap();
    peer.write_u32(packet.len() as u32).await.unwrap();
    peer.write_all(&packet[..1]).await.unwrap();
    assert!(
        controller
            .next(Instant::now() + Duration::from_millis(20))
            .await
            .unwrap()
            .is_none()
    );
    peer.write_all(&packet[1..]).await.unwrap();
    assert_eq!(
        controller
            .next(Instant::now() + Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap()
            .payload["type"],
        "MEDIA_STATUS"
    );
}

#[tokio::test]
async fn cancelling_a_send_never_interleaves_its_partial_frame_with_stop() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (stream, mut peer) = tokio::io::duplex(32);
    let mut controller = Controller::new(stream, "sender-test".into());
    let (prefix, ready) = tokio::sync::oneshot::channel();
    let (resume, proceed) = tokio::sync::oneshot::channel();
    let receiver = tokio::spawn(async move {
        let length = peer.read_u32().await.unwrap();
        prefix.send(()).unwrap();
        proceed.await.unwrap();
        let mut bytes = vec![0; length as usize];
        peer.read_exact(&mut bytes).await.unwrap();
        assert_eq!(cast_wire::decode(&bytes).unwrap().payload["type"], "LOAD");
        assert_eq!(
            cast_wire::read(&mut peer).await.unwrap().payload["type"],
            "STOP"
        );
        peer.flush().await.unwrap();
    });
    {
        let sending = controller.send(
            "web-test",
            MEDIA,
            json!({"type":"LOAD", "large":"x".repeat(1000)}),
        );
        tokio::pin!(sending);
        tokio::select! { result = &mut sending => panic!("frame unexpectedly completed: {result:?}"), _ = ready => {} }
    }
    resume.send(()).unwrap();
    controller
        .send("web-test", MEDIA, json!({"type":"STOP"}))
        .await
        .unwrap();
    receiver.await.unwrap();
}

#[test]
fn cast_endpoints_and_original_format_limits_are_explicit() {
    assert_eq!(endpoint("127.0.0.1").unwrap().port(), 8009);
    assert_eq!(endpoint("192.168.1.20:8010").unwrap().port(), 8010);
    for bad in [
        "localhost",
        "http://127.0.0.1",
        "8.8.8.8",
        "0.0.0.0",
        "127.0.0.1:0",
        "[::1]:8009",
    ] {
        assert!(endpoint(bad).is_err());
    }
    let track = crate::test_support::track();
    assert!(preflight(std::slice::from_ref(&track)).is_ok());
    for (container, codec) in [
        ("mp3", "mp3"),
        ("ogg", "opus"),
        ("ogg", "vorbis"),
        ("mp4", "aac"),
    ] {
        let mut accepted = track.clone();
        accepted.properties.container = container.into();
        accepted.properties.codec = codec.into();
        accepted.properties.lossless = false;
        accepted.properties.bit_depth = None;
        assert!(preflight(&[accepted]).is_ok());
    }
    for change in 0..6 {
        let mut rejected = track.clone();
        match change {
            0 => rejected.properties.channels = Some(6),
            1 => rejected.properties.channels = None,
            2 => {
                rejected.properties.lossless = true;
                rejected.properties.sample_rate = Some(192_000);
            }
            3 => {
                rejected.properties.lossless = true;
                rejected.properties.bit_depth = Some(32);
            }
            4 => rejected.properties.codec = "alac".into(),
            _ => rejected.properties.sample_rate = Some(0),
        }
        assert!(preflight(&[rejected]).is_err(), "{change}");
    }
}

#[tokio::test]
async fn finite_cast_waits_for_each_finish_and_preserves_duplicate_occurrences() {
    let (sender, peer) = tokio::io::duplex(4096);
    let task = tokio::spawn(receiver(peer, Scenario::Finish, None));
    let (_stop, signal) = watch::channel(false);
    let urls = vec![
        "http://127.0.0.1:1234/capability/0".into(),
        "http://127.0.0.1:1234/capability/1".into(),
    ];
    run_stream(
        sender,
        &vec![crate::test_support::track(); 2],
        &urls,
        false,
        signal,
    )
    .await
    .unwrap();
    let events = task.await.unwrap();
    assert_eq!(
        events,
        [
            "CONNECT:receiver-0",
            "LAUNCH",
            "CONNECT:web-test",
            &format!("LOAD:{}", urls[0]),
            &format!("LOAD:{}", urls[1])
        ]
    );
}

#[tokio::test]
async fn an_existing_receiver_session_requires_explicit_replacement() {
    for replace in [false, true] {
        let (sender, peer) = tokio::io::duplex(4096);
        let task = tokio::spawn(receiver(peer, Scenario::Busy, None));
        let (_stop, signal) = watch::channel(false);
        let result = run_stream(
            sender,
            &[crate::test_support::track()],
            &["http://127.0.0.1/cap/0".into()],
            replace,
            signal,
        )
        .await;
        let events = task.await.unwrap();
        if replace {
            assert!(result.is_ok());
            assert!(events.iter().any(|event| event == "LAUNCH"));
        } else {
            assert!(result.unwrap_err().contains("--replace"));
            assert_eq!(events, ["CONNECT:receiver-0"]);
        }
    }
}

#[tokio::test]
async fn load_failure_and_external_takeover_never_finish_or_stop_a_foreign_session() {
    for scenario in [Scenario::Refuse, Scenario::Foreign] {
        let (sender, peer) = tokio::io::duplex(4096);
        let task = tokio::spawn(receiver(peer, scenario, None));
        let (_stop, signal) = watch::channel(false);
        assert!(
            run_stream(
                sender,
                &[crate::test_support::track()],
                &["http://127.0.0.1/cap/0".into()],
                false,
                signal
            )
            .await
            .is_err()
        );
        assert!(!task.await.unwrap().iter().any(|event| event == "STOP"));
    }
}

#[tokio::test]
async fn cancellation_stops_only_the_owned_media_session() {
    let (sender, peer) = tokio::io::duplex(4096);
    let (loaded, ready) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(receiver(peer, Scenario::Wait, Some(loaded)));
    let (stop, signal) = watch::channel(false);
    let playing = tokio::spawn(async move {
        run_stream(
            sender,
            &[crate::test_support::track()],
            &["http://127.0.0.1/cap/0".into()],
            false,
            signal,
        )
        .await
    });
    timeout(Duration::from_secs(8), ready)
        .await
        .unwrap()
        .unwrap();
    stop.send(true).unwrap();
    timeout(Duration::from_secs(4), playing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(task.await.unwrap().last().unwrap(), "STOP");
}

#[test]
fn malformed_status_and_cancelled_playback_are_never_successful_completion() {
    for payload in [json!({}), json!({"status":[]}), json!({"status":[{},{}]})] {
        assert!(media_status(&payload).is_err());
    }
    for state in [
        json!({"playerState":"IDLE"}),
        json!({"playerState":"IDLE", "idleReason":"ERROR"}),
        json!({"playerState":"IDLE", "idleReason":"CANCELLED"}),
        json!({"playerState":"unknown"}),
    ] {
        assert!(playback_state(&state).is_err());
    }
    assert!(playback_state(&json!({"playerState":"IDLE", "idleReason":"FINISHED"})).unwrap());
    for value in ["", "receiver-0", "*", "bad\nroute", "x/x"] {
        assert!(routing_id(&json!(value)).is_err());
    }
}
