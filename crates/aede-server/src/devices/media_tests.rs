use super::*;

#[path = "media_test_support.rs"]
mod fixtures;
use fixtures::{Fixture, get};

#[test]
fn temporary_media_authorizes_only_canonical_selection_requests() {
    let request = parse_request(
        b"GET /secret/0 HTTP/1.1\r\nHost: localhost\r\nRange: bytes=1-3\r\n\r\n",
        "secret",
        1,
    )
    .unwrap();
    assert_eq!(request.index, 0);
    assert!(!request.head);
    assert_eq!(request.headers["range"], "bytes=1-3");
    for raw in [
        "GET /wrong/0 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /secret/01 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /secret/1 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /secret/0?key=x HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "POST /secret/0 HTTP/1.1\r\nHost: localhost\r\n\r\n",
        "GET /secret/0 HTTP/1.1\r\nHost: localhost\r\nHost: other\r\n\r\n",
        "GET /secret/0 HTTP/1.1\r\nHost: localhost\r\nContent-Length: 1\r\n\r\n",
        "GET /secret/0 HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n",
    ] {
        assert!(parse_request(raw.as_bytes(), "secret", 1).is_err(), "{raw}");
    }
}

#[test]
fn media_preserves_duplicate_occurrences_and_refuses_source_replacement() {
    let fixture = Fixture::new();
    assert!(prepare(vec![]).is_err());
    assert!(prepare(vec![fixture.path.clone(); 65]).is_err());
    let sources = prepare(vec![fixture.path.clone(), fixture.path.clone()]).unwrap();
    assert_eq!(sources.len(), 2);
    assert!(fixture.path.is_absolute());
    assert_eq!(sources[0].track.path, fixture.path);
    assert_eq!(sources[1].track.path, fixture.path);
    assert!(sources[0].open().is_ok());
    fs::write(&fixture.path, b"changed").unwrap();
    assert!(sources[0].open().is_err());
    assert!(sources[1].open().is_err());
}

#[cfg(unix)]
#[test]
fn opening_a_fifo_or_symbolic_link_never_waits_for_a_media_writer() {
    let fixture = Fixture::new();
    let pipe = fixture.folder.join("raced-source.flac");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .unwrap()
            .success()
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let opening_pipe = pipe.clone();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(open_regular(&opening_pipe).map(|_| ()));
    });
    let result = receiver.recv_timeout(Duration::from_secs(2));
    if result.is_err() {
        // Release an old blocking implementation before reporting its failure.
        let writer = OpenOptions::new().write(true).open(&pipe).unwrap();
        let _ = receiver.recv_timeout(Duration::from_secs(1));
        drop(writer);
    }
    worker.join().unwrap();
    assert!(
        matches!(result, Ok(Err(_))),
        "a FIFO must be refused without a writer"
    );
    let link = fixture.folder.join("raced-link.flac");
    std::os::unix::fs::symlink(&fixture.path, &link).unwrap();
    assert!(
        open_regular(&link).is_err(),
        "an opened source must not follow a final symlink"
    );
}

#[test]
fn original_http_supports_ranges_head_and_capability_refusal() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let original = fs::read(&fixture.path).unwrap();
        let media = MediaServer::start(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, fixture.sources())
            .await
            .unwrap();
        let complete = get(&media.urls()[0], "GET", "").await;
        let complete_header = complete
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        assert!(complete.starts_with(b"HTTP/1.1 200"));
        assert_eq!(
            &complete[complete_header..],
            original,
            "all original blocks are transferred unchanged"
        );
        let ranged = get(&media.urls()[0], "GET", "Range: bytes=1-9\r\n").await;
        let boundary = ranged
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        assert!(ranged.starts_with(b"HTTP/1.1 206"));
        assert_eq!(&ranged[boundary..], &original[1..10]);
        let head = get(&media.urls()[0], "HEAD", "").await;
        assert!(head.starts_with(b"HTTP/1.1 200"));
        assert!(head.ends_with(b"\r\n\r\n"));
        for range in ["bytes=1-9", "bytes=999999999-", "invalid"] {
            let head = get(&media.urls()[0], "HEAD", &format!("Range: {range}\r\n")).await;
            assert!(
                head.starts_with(b"HTTP/1.1 200"),
                "HEAD ignores Range: {range}"
            );
            let header = std::str::from_utf8(&head).unwrap();
            assert!(header.contains(&format!("Content-Length: {}\r\n", original.len())));
            assert!(!header.contains("Content-Range:"));
            assert!(head.ends_with(b"\r\n\r\n"));
        }
        let bad_range = get(&media.urls()[0], "GET", "Range: bytes=999999999-\r\n").await;
        assert!(bad_range.starts_with(b"HTTP/1.1 416"));
        let wrong = media.urls()[0]
            .rsplit_once('/')
            .unwrap()
            .0
            .rsplit_once('/')
            .unwrap()
            .0
            .to_string()
            + "/wrong/0";
        assert!(get(&wrong, "GET", "").await.starts_with(b"HTTP/1.1 404"));
        assert_eq!(fs::read(&fixture.path).unwrap(), original);
        fs::write(&fixture.path, b"changed").unwrap();
        assert!(
            get(&media.urls()[0], "GET", "")
                .await
                .starts_with(b"HTTP/1.1 409")
        );
        let address = media.address();
        media.stop().await;
        assert!(TcpStream::connect(address).await.is_err());
    });
}

#[test]
fn aborting_a_slow_media_consumer_releases_its_real_worker_slot() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        OpenOptions::new()
            .write(true)
            .open(&fixture.path)
            .unwrap()
            .set_len(32 * 1024 * 1024)
            .unwrap();
        let sources = Arc::new(fixture.sources());
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let slots = Arc::new(Semaphore::new(4));
        let permit = slots.clone().acquire_owned().await.unwrap();
        let task = tokio::spawn(async move { response(server, sources, "session", permit).await });
        client
            .write_all(b"GET /session/0 HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        // Read exactly the headers and one body byte. The sparse trailing data
        // exceeds the socket buffers, so the producer cannot finish beforehand.
        let mut header = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !header.ends_with(b"\r\n\r\n") {
                header.push(client.read_u8().await.unwrap());
            }
            assert!(header.starts_with(b"HTTP/1.1 200"));
            let _ = client.read_u8().await.unwrap();
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let restored =
            tokio::time::timeout(Duration::from_secs(2), slots.clone().acquire_many_owned(4))
                .await
                .unwrap()
                .unwrap();
        drop(restored);
        assert_eq!(slots.available_permits(), 4);
    });
}

#[test]
fn temporary_media_refuses_a_different_device_peer() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let media = MediaServer::start(
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::new(127, 0, 0, 2),
            fixture.sources(),
        )
        .await
        .unwrap();
        let mut stream = TcpStream::connect(media.address()).await.unwrap();
        let mut bytes = [0; 1];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), stream.read(&mut bytes))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        media.stop().await;
    });
}
