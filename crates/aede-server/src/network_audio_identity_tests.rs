use super::*;

#[path = "network_audio_identity_test_support.rs"]
mod fixtures;
use fixtures::{Change, observe};

#[test]
fn unchanged_catalog_original_is_transferred_after_network_acceptance() {
    let observed = observe(Change::None);
    assert!(observed.accepted);
    assert!(!observed.catalog_changed);
    assert_eq!(
        observed.served.as_deref(),
        Some(observed.original.as_slice())
    );
}

#[test]
fn changed_catalog_original_is_refused_before_network_acceptance() {
    let observed = observe(Change::BeforeAdmission);
    assert!(!observed.accepted);
    assert!(observed.catalog_changed);
    assert!(observed.served.is_none());
    assert_eq!(observed.status, None);
}

#[test]
fn changed_catalog_original_after_acceptance_is_refused_before_transfer() {
    let observed = observe(Change::AfterAdmission);
    assert!(observed.accepted);
    assert!(observed.catalog_changed);
    assert!(
        observed.served.is_none(),
        "changed original reached the renderer: {} replacement bytes served",
        observed.served.as_ref().map_or(0, Vec::len),
    );
    assert_eq!(observed.status, Some("failed"));
}
