use super::*;

#[test]
fn supervised_policy_refuses_overlapping_windows_and_unknown_timezone() {
    let mut policy = Policy {
        enabled: true,
        ..Policy::default()
    };
    policy.days[0].windows = vec![
        Window {
            start_minute: 60,
            end_minute: 120,
        },
        Window {
            start_minute: 90,
            end_minute: 180,
        },
    ];
    assert!(policy.validate().is_err());
    policy.days[0].windows.clear();
    policy.timezone = "Unknown/Nowhere".into();
    assert!(policy.validate().is_err());
}

#[test]
fn defaults_are_unrestricted_and_unknown_fields_are_rejected() {
    let policy = Policy::default();
    assert!(!policy.enabled);
    assert!(policy.radio_allowed);
    assert_eq!(policy.days.len(), 7);
    assert!(policy.validate().is_ok());
    let mut json = serde_json::to_value(&policy).unwrap();
    json["surprise"] = true.into();
    assert!(serde_json::from_value::<Policy>(json).is_err());
}
