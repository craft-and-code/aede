use super::*;
#[test]
fn optional_preferences_roundtrip_preserve_other_owners_and_refuse_corruption() {
    let mut data = UserData::default();
    data.client_preferences.push(ClientPreference {
        owner: "account:one".into(),
        key: "phemios.duration".into(),
        value: "true".into(),
    });
    data.client_preferences.push(ClientPreference {
        owner: "account:two".into(),
        key: "phemios.duration".into(),
        value: "false".into(),
    });
    let raw = crate::user::to_json(&data);
    let restored = crate::user::from_json(&raw).unwrap();
    assert_eq!(restored.client_preferences.len(), 2);
    assert_eq!(restored.client_preferences[1].value, "false");
    let mut invalid = raw.clone();
    invalid.set("client_preferences", Json::Bool(false));
    assert!(crate::user::from_json(&invalid).is_err());
    assert!(!valid_preference("../../secret", "true"));
    assert!(!valid_preference("phemios.duration", "invalid"));
}
#[test]
fn imports_do_not_replace_local_preferences() {
    let row = |value: &str| ClientPreference {
        owner: "account:one".into(),
        key: "phemios.duration".into(),
        value: value.into(),
    };
    let mut existing = UserData {
        client_preferences: vec![row("true")],
        ..Default::default()
    };
    crate::user::merge(
        &mut existing,
        UserData {
            client_preferences: vec![row("false")],
            ..Default::default()
        },
    );
    assert_eq!(existing.client_preferences[0].value, "true");
}
