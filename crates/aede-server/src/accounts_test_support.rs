//! Isolated credential stores and real HTTP requests for account tests.

use super::*;
use aede_core::accounts::{self, Accounts, Role};
use std::io::{Read, Write};

pub(super) const PASSWORD: &str = "a long test passphrase";
pub(super) const ADMIN_TOKEN: &str = "0123456789abcdef0123456789abcdef";

pub(super) struct Fixture(pub(super) ApiState);

impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut state = test_support::sample_state();
        state.data_dir = std::env::temp_dir().join(format!(
            "aede_accounts_api_{}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        state.admin = Some(Admin {
            token: ADMIN_TOKEN.into(),
            data_dir: state.data_dir.clone(),
            scan: Arc::new(|_| Ok(())),
            job: Arc::new(|_, _| Ok(JobOutput::default())),
        });
        store::save_catalog_only(
            state.catalog.try_read().unwrap().as_ref().unwrap(),
            &store::catalog_path(&state.data_dir),
        )
        .unwrap();
        let mut accounts = Accounts::bootstrap("operator", PASSWORD, 10).unwrap();
        accounts.create("alice", PASSWORD, Role::User, 10).unwrap();
        accounts.create("bob", PASSWORD, Role::User, 10).unwrap();
        accounts
            .create("auditor", PASSWORD, Role::Auditor, 10)
            .unwrap();
        accounts::save(&accounts, &accounts::accounts_path(&state.data_dir)).unwrap();
        Self(state)
    }

    pub(super) fn accounts(&self) -> Accounts {
        accounts::load(&accounts::accounts_path(&self.0.data_dir))
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0.data_dir);
    }
}

pub(super) fn http(
    address: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> (u16, serde_json::Value, String) {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    let (headers, body) = answer.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    let value = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(body).unwrap()
    };
    (status, value, headers.into())
}

pub(super) fn login(address: SocketAddr, name: &str) -> String {
    let (status, value, headers) = http(
        address,
        "POST",
        "/api/auth/v1/session",
        None,
        &format!(r#"{{"username":"{name}","password":"{PASSWORD}"}}"#),
    );
    assert_eq!(status, 200, "{value}");
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("cache-control: no-store")
    );
    assert!(!value.to_string().contains("argon2"));
    value["token"].as_str().unwrap().into()
}
