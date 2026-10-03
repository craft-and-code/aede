//! Credentials for disposable test accounts, never real installation secrets.

pub(super) const PASSWORD: &str = "a long test passphrase";

pub(super) fn accounts() -> crate::Accounts {
    crate::Accounts::bootstrap("Operator", PASSWORD, 10).expect("test accounts")
}
