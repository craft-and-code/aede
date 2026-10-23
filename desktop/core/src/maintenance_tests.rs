use super::*;

#[test]
fn renewal_before_first_start_does_not_record_an_unfulfillable_password_change() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let original = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(host.regenerate_password().is_err());
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), original);
    let mut config = read(&fixture.0).unwrap();
    config.identity = Some(configuration::Identity::generate().unwrap());
    save(&fixture.0, &config).unwrap();
    let initialized_identity = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(host.regenerate_password().is_err());
    assert_eq!(
        fs::read(fixture.0.join("server.json")).unwrap(),
        initialized_identity
    );
    assert!(read(&fixture.0).unwrap().pending_password.is_none());
}

#[test]
fn new_server_retains_the_previous_store_and_archives_its_private_configuration() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let mut config = read(&fixture.0).unwrap();
    config.port = 19876;
    config.roots = vec![fixture.0.join("music").display().to_string()];
    config.identity = Some(configuration::Identity::generate().unwrap());
    save(&fixture.0, &config).unwrap();
    let old = fs::read(fixture.0.join("server.json")).unwrap();
    let data = fixture.0.join("engine");
    fs::create_dir(&data).unwrap();
    fs::write(data.join("accounts.json"), b"retained accounts").unwrap();
    fs::write(data.join("catalog.json"), b"retained catalog").unwrap();
    let archive = host.new_server().unwrap();
    assert_eq!(fs::read(archive.join("server.json")).unwrap(), old);
    assert_eq!(
        fs::read(data.join("accounts.json")).unwrap(),
        b"retained accounts"
    );
    assert_eq!(
        fs::read(data.join("catalog.json")).unwrap(),
        b"retained catalog"
    );
    let fresh = read(&fixture.0).unwrap();
    assert!(fresh.identity.is_none());
    assert!(fresh.roots.is_empty());
    assert_eq!(fresh.port, 19876);
    assert_ne!(fresh.data_directory(&fixture.0), data);
    assert!(!fresh.data_directory(&fixture.0).exists());
    drop(host);
    let mut reopened = fixture.host();
    assert_eq!(
        reopened.status().unwrap().data_directory,
        fresh.data_directory(&fixture.0).display().to_string()
    );
    assert!(!reopened.status().unwrap().running);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&archive).unwrap().permissions().mode() & 0o077,
            0
        );
        assert_eq!(
            fs::metadata(archive.join("server.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
}

#[test]
fn failed_password_change_retains_a_recoverable_intent_without_revealing_stale_credentials() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let mut config = read(&fixture.0).unwrap();
    config.identity = Some(configuration::Identity::generate().unwrap());
    let old_password = config.identity.as_ref().unwrap().password.clone();
    save(&fixture.0, &config).unwrap();
    fs::create_dir(fixture.0.join("engine")).unwrap();
    fs::write(fixture.0.join("engine/accounts.json"), b"retained accounts").unwrap();
    assert!(host.regenerate_password().is_err());
    let pending = read(&fixture.0).unwrap();
    assert_eq!(pending.identity.unwrap().password, old_password);
    assert_ne!(pending.pending_password.as_ref().unwrap(), &old_password);
    assert!(host.password().is_err());
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(host.regenerate_password().is_err());
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    let archive = host.new_server().unwrap();
    assert_eq!(fs::read(archive.join("server.json")).unwrap(), before);
    assert_eq!(
        fs::read(fixture.0.join("engine/accounts.json")).unwrap(),
        b"retained accounts"
    );
}

#[test]
fn maintenance_identifiers_cannot_select_external_directories_or_invalid_passwords() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let original = fs::read(fixture.0.join("server.json")).unwrap();
    for value in ["../other", "/other", "short", &"x".repeat(64)] {
        let mut config = read(&fixture.0).unwrap();
        config.data_profile = Some(value.into());
        assert!(save(&fixture.0, &config).is_err());
        assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), original);
    }
    assert_eq!(
        host.profile_directory().unwrap(),
        fs::canonicalize(&fixture.0).unwrap()
    );
    let mut config = read(&fixture.0).unwrap();
    config.pending_password = Some("a".repeat(64));
    assert!(save(&fixture.0, &config).is_err());
}

#[cfg(unix)]
#[test]
fn a_redirected_archive_directory_cannot_publish_a_new_profile() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let other = Fixture::new();
    fs::create_dir(&other.0).unwrap();
    symlink(&other.0, fixture.0.join("archives")).unwrap();
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(host.new_server().is_err());
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    assert_eq!(fs::read_dir(&other.0).unwrap().count(), 0);
}
