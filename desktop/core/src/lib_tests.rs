use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(std::env::temp_dir().join(format!(
            "aede-desktop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn host(&self) -> Host {
        Host::new(self.0.clone(), PathBuf::from("missing-test-engine")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn empty_profile_stays_stopped_and_discloses_no_secret() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let status = serde_json::to_value(host.status().unwrap()).unwrap();
    assert_eq!(status["running"], false);
    assert_eq!(status["port"], 8787);
    assert_eq!(status["roots"], json!([]));
    assert!(status.get("password").is_none());
    assert!(host.password().is_err());
}
#[test]
fn another_launcher_cannot_modify_the_same_profile() {
    let fixture = Fixture::new();
    let host = fixture.host();
    assert!(Host::new(fixture.0.clone(), PathBuf::new()).is_err());
    drop(host);
    assert!(Host::new(fixture.0.clone(), PathBuf::new()).is_ok());
}
#[test]
fn folders_and_port_survive_restart_without_starting_the_server() {
    let fixture = Fixture::new();
    let folder = fixture
        .0
        .join("absent-volume")
        .to_string_lossy()
        .into_owned();
    {
        let mut host = fixture.host();
        host.configure(18877, vec![folder.clone()]).unwrap();
    }
    let mut host = fixture.host();
    let status = host.status().unwrap();
    assert_eq!(status.port, 18877);
    assert_eq!(status.roots[0].path, folder);
    assert!(!status.roots[0].available);
    assert!(!status.running);
}
#[test]
fn invalid_configuration_is_refused_without_replacing_saved_data() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    for (port, roots) in [
        (0, vec![]),
        (8787, vec!["relative".into()]),
        (8787, vec![fixture.0.display().to_string(); 65]),
    ] {
        assert!(host.configure(port, roots).is_err());
        assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    }
}
#[test]
fn corrupt_or_unknown_profiles_are_never_reset() {
    let fixture = Fixture::new();
    {
        fixture.host();
    }
    let path = fixture.0.join("server.json");
    for text in [
        "broken",
        r#"{"version":99,"port":8787,"roots":[],"identity":null}"#,
        r#"{"version":1,"port":8787,"roots":[],"identity":null,"unexpected":true}"#,
    ] {
        fs::write(&path, text).unwrap();
        assert!(Host::new(fixture.0.clone(), PathBuf::new()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}
#[test]
fn generated_access_is_retained_but_public_export_has_no_private_material() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let mut config = read(&fixture.0).unwrap();
    config.identity = Some(configuration::Identity::generate().unwrap());
    save(&fixture.0, &config).unwrap();
    let secret = host.password().unwrap();
    assert_eq!(secret.len(), 64);
    assert_eq!(host.password().unwrap(), secret);
    assert!(host.connection_profile().is_err());
}
#[test]
fn missing_bundled_binary_is_an_error_without_running_state() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    assert!(host.start().is_err());
    assert!(!host.status().unwrap().running);
}
#[cfg(unix)]
#[test]
fn symlink_and_world_readable_profiles_are_refused() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let fixture = Fixture::new();
    {
        fixture.host();
    }
    let path = fixture.0.join("server.json");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Host::new(fixture.0.clone(), PathBuf::new()).is_err());
    fs::remove_file(&path).unwrap();
    let target = fixture.0.join("target");
    fs::write(&target, "retained").unwrap();
    symlink(&target, &path).unwrap();
    assert!(Host::new(fixture.0.clone(), PathBuf::new()).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "retained");
}
#[cfg(feature = "engine-tests")]
#[path = "engine_tests.rs"]
mod engine;

#[path = "profile_creation_tests.rs"]
mod profile_creation;
#[path = "selection_tests.rs"]
mod selection;

#[path = "legacy_selection_tests.rs"]
mod legacy_selection;

#[path = "maintenance_tests.rs"]
mod maintenance;
