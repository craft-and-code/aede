use super::super::Media;
use super::*;

#[test]
fn supervision_lock_survives_backup_and_can_only_be_released_by_its_administrator() {
    let mut store = ParentalStore::default();
    let rules = Policy {
        enabled: true,
        ..Policy::default()
    };
    let claimed = store
        .set_policy_by("listener", Some("parent-one"), rules.clone(), 0)
        .unwrap();
    assert_eq!(claimed.locked_by.as_deref(), Some("parent-one"));
    let encoded = crate::parental::to_json(&store);
    let mut restored = crate::parental::from_json(&encoded).unwrap();
    assert!(restored.supervises("parent-one"));
    assert!(
        restored
            .set_policy_by(
                "listener",
                Some("parent-two"),
                Policy::default(),
                claimed.revision
            )
            .is_err()
    );
    assert!(
        restored
            .set_policy("listener", Policy::default(), claimed.revision)
            .is_err()
    );
    assert!(
        restored
            .set_policy_by("listener", Some("parent-one"), Policy::default(), 0)
            .is_err()
    );
    let disabled = restored
        .set_policy_by(
            "listener",
            Some("parent-one"),
            Policy::default(),
            claimed.revision,
        )
        .unwrap();
    assert!(disabled.locked_by.is_none());
    assert!(!restored.supervises("parent-one"));
    assert_eq!(
        restored
            .set_policy_by("listener", Some("parent-two"), rules, disabled.revision)
            .unwrap()
            .locked_by
            .as_deref(),
        Some("parent-two")
    );
}

#[test]
fn legacy_rules_allow_podcasts_and_have_no_implicit_supervisor() {
    let mut legacy = serde_json::to_value(ParentalStore::default()).unwrap();
    let mut policy = serde_json::to_value(Policy::default()).unwrap();
    policy.as_object_mut().unwrap().remove("podcasts_allowed");
    legacy["owners"] = serde_json::json!({"listener":{"revision":1,"policy":policy,"settled":[],"reservations":[]}});
    let store: ParentalStore = serde_json::from_value(legacy).unwrap();
    store.validate().unwrap();
    assert!(store.policy("listener").policy.podcasts_allowed);
    assert!(store.policy("listener").locked_by.is_none());
}
struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aede-parental-{}",
            crate::accounts::random_token().unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> PathBuf {
        path(&self.0)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn restart_keeps_reservations_and_unreadable_stores_never_become_defaults() {
    let folder = Folder::new();
    let file = folder.file();
    let mut store = ParentalStore::load(&file).unwrap();
    let mut policy = Policy {
        enabled: true,
        ..Policy::default()
    };
    policy.days[0].limit_seconds = Some(15);
    store.set_policy("listener", policy, 0).unwrap();
    let now = "2026-10-12T12:00:00Z"
        .parse::<jiff::Timestamp>()
        .unwrap()
        .as_second() as u64;
    store
        .reserve("listener", &"a".repeat(64), Media::Radio, 10, now)
        .unwrap();
    store.save(&file).unwrap();
    let mut loaded = ParentalStore::load(&file).unwrap();
    assert_eq!(
        loaded
            .status("listener", now + 20, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        10
    );
    assert_eq!(
        loaded
            .reserve("listener", &"b".repeat(64), Media::Radio, 10, now + 20)
            .unwrap()
            .granted_seconds,
        5
    );
    std::fs::write(&file, b"{truncated").unwrap();
    assert!(ParentalStore::load(&file).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"{truncated");
}
#[test]
fn owner_policies_and_usage_cannot_replace_each_other_or_ignore_revisions() {
    let mut store = ParentalStore::default();
    store
        .set_policy(
            "alice",
            Policy {
                enabled: true,
                ..Policy::default()
            },
            0,
        )
        .unwrap();
    store
        .set_policy(
            "bob",
            Policy {
                radio_allowed: false,
                ..Policy::default()
            },
            0,
        )
        .unwrap();
    assert!(store.set_policy("alice", Policy::default(), 0).is_err());
    assert!(store.policy("alice").policy.enabled);
    assert!(!store.policy("bob").policy.radio_allowed);
    assert!(!store.policy("unknown").policy.enabled);
    assert_eq!(store.policy("unknown").revision, 0);
}
#[cfg(unix)]
#[test]
fn parental_store_refuses_symlinks_and_public_permissions() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let folder = Folder::new();
    let file = folder.file();
    ParentalStore::default().save(&file).unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = folder.0.join("link.json");
    symlink(&file, &link).unwrap();
    assert!(ParentalStore::load(&link).is_err());
    assert!(ParentalStore::default().save(&link).is_err());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(ParentalStore::load(&file).is_err());
    assert!(ParentalStore::default().save(&file).is_err());
}
#[test]
fn duplicate_owner_documents_and_overlong_intervals_are_refused() {
    let owner = serde_json::to_string(&Owner::default()).unwrap();
    let duplicate = format!(
        "{{\"format_version\":1,\"owners\":{{\"listener\":{owner},\"listener\":{owner}}}}}"
    );
    assert!(serde_json::from_str::<ParentalStore>(&duplicate).is_err());
    let folder = Folder::new();
    let file = folder.file();
    let mut store = ParentalStore::default();
    store.set_policy("listener", Policy::default(), 0).unwrap();
    store.save(&file).unwrap();
    let before = std::fs::read(&file).unwrap();
    store
        .owners
        .get_mut("listener")
        .unwrap()
        .settled
        .push(Interval {
            start_ms: 0,
            end_ms: RETAIN_MS + 1,
        });
    assert!(store.save(&file).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
}
