use super::*;
struct Data(std::path::PathBuf);
impl Data {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("aede-desktop-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Data {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const INPUT: &str = r#"{"username":"desktop-test","password":"desktop-test-password-123"}"#;
#[test]
fn bootstrap_retains_account_identity_and_catalog() {
    let data = Data::new("retained");
    bootstrap(&data.0, INPUT).unwrap();
    let before = std::fs::read(accounts::accounts_path(&data.0)).unwrap();
    bootstrap(&data.0, INPUT).unwrap();
    assert_eq!(
        before,
        std::fs::read(accounts::accounts_path(&data.0)).unwrap()
    );
    assert!(
        store::load(&store::catalog_path(&data.0))
            .unwrap()
            .is_some()
    );
    assert!(
        bootstrap(
            &data.0,
            r#"{"username":"other","password":"other-long-password-123"}"#
        )
        .is_err()
    );
    assert_eq!(
        before,
        std::fs::read(accounts::accounts_path(&data.0)).unwrap()
    );
}
#[test]
fn bootstrap_refuses_corruption_and_unknown_fields() {
    let data = Data::new("invalid");
    assert!(
        bootstrap(
            &data.0,
            r#"{"username":"x","password":"long-enough-password","extra":true}"#
        )
        .is_err()
    );
    assert!(!accounts::accounts_path(&data.0).exists());
    std::fs::write(store::catalog_path(&data.0), b"broken").unwrap();
    assert!(bootstrap(&data.0, INPUT).is_err());
    assert_eq!(
        std::fs::read(store::catalog_path(&data.0)).unwrap(),
        b"broken"
    );
    assert!(!accounts::accounts_path(&data.0).exists());
}

#[test]
fn desktop_tls_bootstrap_rejects_bad_identity_before_creating_stores() {
    let data = Data::new("bad-tls");
    let input = INPUT.trim_end_matches('}').to_owned()
        + r#","tls":{"certificate":"invalid","private_key":"invalid"}}"#;
    assert!(bootstrap(&data.0, &input).is_err());
    assert!(!accounts::accounts_path(&data.0).exists());
    assert!(!store::catalog_path(&data.0).exists());
}
