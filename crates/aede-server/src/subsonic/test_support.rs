//! Shared test-only REST requests and isolated account-key fixtures.

use super::*;
use std::io::{Read, Write};

pub(super) fn key(fixture: &crate::accounts_test_support::Fixture, username: &str) -> String {
    let mut accounts = fixture.accounts();
    let (_, token) = accounts
        .create_api_key(username, "test client", 11)
        .unwrap();
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
    )
    .unwrap();
    token
}

pub(super) fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> (u16, String, Vec<u8>) {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}", body.len()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let end = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, headers, bytes[end + 4..].to_vec())
}

pub(super) fn json_request(address: SocketAddr, path: &str) -> Value {
    let (status, headers, body) = request(address, "GET", path, "", "");
    assert_eq!(status, 200, "{headers}");
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("cache-control: no-store")
    );
    serde_json::from_slice(&body).unwrap()
}
