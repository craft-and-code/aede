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

#[test]
fn invalid_preferences_cannot_replace_a_readable_personal_store() {
    use crate::user::test_support::TemporaryUser;
    let temporary = TemporaryUser::new();
    let mut data = UserData {
        client_preferences: vec![ClientPreference {
            owner: "account:one".into(),
            key: "phemios.duration".into(),
            value: "true".into(),
        }],
        ..Default::default()
    };
    crate::user::save(&data, &temporary.path()).unwrap();
    let original = std::fs::read(temporary.path()).unwrap();
    for invalid in [
        ClientPreference {
            owner: "".into(),
            key: "phemios.other".into(),
            value: "false".into(),
        },
        ClientPreference {
            owner: "account:two".into(),
            key: "phemios.other".into(),
            value: "invalid".into(),
        },
        data.client_preferences[0].clone(),
    ] {
        data.client_preferences.push(invalid);
        assert!(crate::user::save(&data, &temporary.path()).is_err());
        assert_eq!(std::fs::read(temporary.path()).unwrap(), original);
        data.client_preferences.pop();
    }
    data.client_preferences = (0..8192)
        .map(|index| ClientPreference {
            owner: format!("account:{}", index / 64),
            key: format!("phemios.setting-{}", index % 64),
            value: "true".into(),
        })
        .collect();
    crate::user::save(&data, &temporary.path()).unwrap();
    let full = std::fs::read(temporary.path()).unwrap();
    assert_eq!(
        crate::user::load(&temporary.path())
            .unwrap()
            .unwrap()
            .client_preferences
            .len(),
        8192
    );
    data.client_preferences.push(ClientPreference {
        owner: "account:new".into(),
        key: "phemios.moods".into(),
        value: "[]".into(),
    });
    assert!(crate::user::save(&data, &temporary.path()).is_err());
    assert_eq!(std::fs::read(temporary.path()).unwrap(), full);
}
