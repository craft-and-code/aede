use std::net::SocketAddr;
use std::path::PathBuf;

use super::*;

fn args(words: &[&str]) -> Args {
    Args::parse(words.iter().map(|word| (*word).into()))
}

fn options(words: &[&str]) -> Result<aede_server::ServerOptions, String> {
    server_options(&args(words))
}

fn refusal(words: &[&str]) -> String {
    match options(words) {
        Ok(_) => panic!("{words:?} must be refused"),
        Err(error) => error,
    }
}

#[test]
fn local_http_defaults_to_ipv4_loopback() {
    let options = options(&["serve"]).expect("default options");
    assert_eq!(
        options.bind,
        "127.0.0.1:8787".parse::<SocketAddr>().unwrap()
    );
    assert!(options.tls.is_none());
}

#[test]
fn local_http_allows_an_explicit_ipv6_loopback_listener() {
    let options =
        options(&["serve", "--bind", "::1", "--port", "9000"]).expect("IPv6 loopback options");
    assert_eq!(options.bind, "[::1]:9000".parse::<SocketAddr>().unwrap());
    assert!(options.tls.is_none());
}

#[test]
fn loopback_tls_keeps_its_explicit_authority() {
    let options = options(&[
        "serve",
        "--bind",
        "::1",
        "--port",
        "8443",
        "--tls-cert",
        "certificate.pem",
        "--tls-key",
        "private-key.pem",
        "--authority",
        "[::1]:8443",
    ])
    .expect("loopback TLS options");
    assert_eq!(options.bind, "[::1]:8443".parse::<SocketAddr>().unwrap());
    let tls = options.tls.expect("TLS options");
    assert_eq!(tls.certificate, PathBuf::from("certificate.pem"));
    assert_eq!(tls.private_key, PathBuf::from("private-key.pem"));
    assert_eq!(tls.authority, "[::1]:8443");
}

#[test]
fn remote_listener_requires_a_complete_tls_configuration() {
    let complete = options(&[
        "serve",
        "--bind=192.0.2.10",
        "--port=8443",
        "--tls-cert=certificate.pem",
        "--tls-key=private-key.pem",
        "--authority=music.example.test:443",
    ])
    .expect("remote TLS options");
    assert_eq!(
        complete.bind,
        "192.0.2.10:8443".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(
        complete.tls.expect("TLS authority").authority,
        "music.example.test:443"
    );

    for words in [
        vec!["serve", "--bind=192.0.2.10"],
        vec!["serve", "--tls-cert=certificate.pem"],
        vec!["serve", "--tls-key=private-key.pem"],
        vec!["serve", "--authority=music.example.test:443"],
        vec![
            "serve",
            "--tls-cert=certificate.pem",
            "--tls-key=private-key.pem",
        ],
        vec![
            "serve",
            "--tls-cert=certificate.pem",
            "--authority=music.example.test:443",
        ],
    ] {
        assert!(options(&words).is_err(), "{words:?} must be refused");
    }
}

#[test]
fn tls_requires_a_fixed_port_and_a_strict_authority() {
    let fixed_port = refusal(&[
        "serve",
        "--port=0",
        "--tls-cert=certificate.pem",
        "--tls-key=private-key.pem",
        "--authority=music.example.test:8443",
    ]);
    assert!(fixed_port.contains("--port=0"));

    for authority in [
        "",
        "music.example.test",
        "https://music.example.test:8443",
        "music.example.test:0",
        "music.example.test:8443/path",
        "user@music.example.test:8443",
        "::1:8443",
        "[::1]8443",
        "[not-an-ip]:8443",
    ] {
        let error = refusal(&[
            "serve",
            "--tls-cert=certificate.pem",
            "--tls-key=private-key.pem",
            "--authority",
            authority,
        ]);
        assert!(error.contains("--authority"), "{authority:?}: {error}");
    }
}

#[test]
fn tls_authority_port_has_a_canonical_decimal_spelling() {
    let error = refusal(&[
        "serve",
        "--tls-cert=certificate.pem",
        "--tls-key=private-key.pem",
        "--authority=aede.test:0443",
    ]);
    assert!(error.contains("--authority"), "{error}");
}

#[test]
fn bind_accepts_only_literal_ip_addresses() {
    for bind in ["localhost", "music.example.test", "127.0.0.1:8787", ""] {
        let error = refusal(&["serve", "--bind", bind]);
        assert!(error.contains("--bind"), "{bind:?}: {error}");
    }
}
