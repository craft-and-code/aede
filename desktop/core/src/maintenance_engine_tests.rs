use super::*;

#[test]
fn renewal_changes_the_real_account_preserves_tls_and_refuses_running_maintenance() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    host.start().unwrap();
    let old = read(&fixture.0).unwrap();
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(host.regenerate_password().is_err());
    assert!(host.new_server().is_err());
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    host.stop().unwrap();
    host.regenerate_password().unwrap();
    let renewed = read(&fixture.0).unwrap();
    assert!(renewed.pending_password.is_none());
    assert_ne!(
        renewed.identity.as_ref().unwrap().password,
        old.identity.as_ref().unwrap().password
    );
    assert_eq!(
        renewed.identity.as_ref().unwrap().certificate_der,
        old.identity.as_ref().unwrap().certificate_der
    );
    assert_eq!(
        renewed.data_directory(&fixture.0),
        old.data_directory(&fixture.0)
    );
    host.start().unwrap();
    assert!(
        host.process
            .as_mut()
            .unwrap()
            .authenticate(old.identity.as_ref().unwrap())
            .is_err()
    );
    assert!(
        host.process
            .as_mut()
            .unwrap()
            .authenticate(renewed.identity.as_ref().unwrap())
            .is_ok()
    );
    host.stop().unwrap();
}

#[test]
fn restart_finishes_a_recorded_password_change_and_new_profile_bootstraps_independently() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    host.start().unwrap();
    host.stop().unwrap();
    let previous = read(&fixture.0).unwrap();
    let old_accounts = fs::read(previous.data_directory(&fixture.0).join("accounts.json")).unwrap();
    let mut interrupted = read(&fixture.0).unwrap();
    let requested = configuration::random().unwrap();
    interrupted.pending_password = Some(requested.clone());
    save(&fixture.0, &interrupted).unwrap();
    drop(host);
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.start().unwrap();
    assert_eq!(host.password().unwrap(), requested);
    assert!(read(&fixture.0).unwrap().pending_password.is_none());
    host.stop().unwrap();
    // Simulate account publication followed by interruption before profile publication.
    let mut interrupted_after_account_write = read(&fixture.0).unwrap();
    interrupted_after_account_write
        .identity
        .as_mut()
        .unwrap()
        .password = previous.identity.as_ref().unwrap().password.clone();
    interrupted_after_account_write.pending_password = Some(requested.clone());
    save(&fixture.0, &interrupted_after_account_write).unwrap();
    drop(host);
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.start().unwrap();
    assert_eq!(host.password().unwrap(), requested);
    assert!(read(&fixture.0).unwrap().pending_password.is_none());
    host.stop().unwrap();
    let retained_accounts =
        fs::read(previous.data_directory(&fixture.0).join("accounts.json")).unwrap();
    assert_ne!(retained_accounts, old_accounts);
    let archive = host.new_server().unwrap();
    assert!(archive.join("server.json").is_file());
    assert_eq!(
        fs::read(previous.data_directory(&fixture.0).join("accounts.json")).unwrap(),
        retained_accounts
    );
    host.start().unwrap();
    assert_ne!(host.password().unwrap(), requested);
    assert_ne!(
        read(&fixture.0).unwrap().identity.unwrap().certificate_der,
        previous.identity.as_ref().unwrap().certificate_der
    );
    assert_eq!(
        serde_json::to_value(host.diagnostics().unwrap()).unwrap()["apiResponding"],
        true
    );
    host.stop().unwrap();
    assert_eq!(
        fs::read(previous.data_directory(&fixture.0).join("accounts.json")).unwrap(),
        retained_accounts
    );
}
