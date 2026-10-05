use super::*;
use tokio::time::timeout;

#[test]
fn certificate_pins_use_sha256_and_refuse_noncanonical_lengths() {
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(parse_pin(expected).unwrap(), digest(b"abc").unwrap());
    assert_eq!(
        parse_pin(&expected.to_uppercase()).unwrap(),
        digest(b"abc").unwrap()
    );
    for value in [
        "",
        "sha256:abc",
        &"0".repeat(63),
        &"g".repeat(64),
        &"0".repeat(65),
    ] {
        assert!(parse_pin(value).is_err(), "{value}");
    }
}

#[tokio::test]
async fn only_the_pinned_key_can_complete_tls_and_inspection_never_completes_it() {
    for version in [&rustls::version::TLS12, &rustls::version::TLS13] {
        let (cert, key) = crate::test_support::tls_material();
        let pin = digest(cert.as_ref()).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[version])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        for mode in 0..3 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let acceptor = acceptor.clone();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                timeout(Duration::from_secs(3), acceptor.accept(socket))
                    .await
                    .unwrap()
            });
            match mode {
                0 => {
                    assert!(
                        connect(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, port, pin)
                            .await
                            .is_ok()
                    );
                }
                1 => {
                    let error = connect(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, port, [0; 32])
                        .await
                        .unwrap_err();
                    assert!(error.contains("does not match"), "{error}");
                }
                _ => {
                    let observed = inspect(Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST, port)
                        .await
                        .unwrap();
                    assert_eq!(parse_pin(&observed).unwrap(), pin);
                }
            }
            assert_eq!(server.await.unwrap().is_ok(), mode == 0);
        }
    }
}
