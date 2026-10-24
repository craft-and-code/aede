use super::*;
use crate::user::test_support::TemporaryUser;

#[test]
fn legacy_identity_is_repeatable_without_writes_then_persisted_across_rename() {
    for version in [1, 2] {
        let file = TemporaryUser::new();
        let original = format!(
            r#"{{"format_version":{version},"collections":[{{"name":"Jazz","query":"genre:Jazz","created_at":10}}]}}"#
        );
        std::fs::write(file.path(), &original).unwrap();
        let mut data = load(&file.path()).unwrap().unwrap();
        let id = data.collections[0].id.clone();
        assert!(valid_collection_id(&id));
        assert_eq!(load(&file.path()).unwrap().unwrap().collections[0].id, id);
        assert_eq!(std::fs::read_to_string(file.path()).unwrap(), original);
        data.collections[0].name = "Renamed".into();
        save(&data, &file.path()).unwrap();
        assert_eq!(
            load(&file.path())
                .unwrap()
                .unwrap()
                .collection_by_id(LOCAL_USER, &id)
                .unwrap()
                .name,
            "Renamed"
        );
        assert_eq!(to_json(&data).field_u32("format_version"), Some(3));
    }
}

#[test]
fn replacements_keep_identity_but_delete_and_recreate_does_not_rebind() {
    let mut data = UserData::default();
    data.save_collection(LOCAL_USER, "Jazz", "genre:Jazz", 10)
        .unwrap();
    let id = data.collections[0].id.clone();
    data.save_collection(LOCAL_USER, "jazz", "loved", 20)
        .unwrap();
    assert_eq!(data.collections[0].id, id);
    assert!(data.collection_by_id("someone-else", &id).is_none());
    assert!(data.forget_collection(LOCAL_USER, "Jazz"));
    data.save_collection(LOCAL_USER, "Jazz", "loved", 10)
        .unwrap();
    assert_ne!(data.collections[0].id, id);
    assert!(data.collection_by_id(LOCAL_USER, &id).is_none());
}

#[test]
fn malformed_and_duplicate_identities_preserve_the_original_document() {
    for id in ["null", "42", "\"\"", "\"collection-wrong\""] {
        let file = TemporaryUser::new();
        let text = format!(
            r#"{{"format_version":2,"collections":[{{"id":{id},"name":"Jazz","query":"loved"}}]}}"#
        );
        std::fs::write(file.path(), &text).unwrap();
        assert!(load(&file.path()).is_err());
        assert_eq!(std::fs::read_to_string(file.path()).unwrap(), text);
    }
    let mut data = UserData::default();
    data.save_collection(LOCAL_USER, "Jazz", "loved", 1)
        .unwrap();
    let mut duplicate = data.collections[0].clone();
    duplicate.name = "Other".into();
    data.collections.push(duplicate);
    assert!(from_json(&to_json(&data)).is_err());
    let file = TemporaryUser::new();
    std::fs::write(file.path(), "original").unwrap();
    assert!(save(&data, &file.path()).is_err());
    assert_eq!(std::fs::read_to_string(file.path()).unwrap(), "original");
}

#[test]
fn merging_renamed_backups_keeps_identity_and_refuses_conflicting_names() {
    let mut data = UserData::default();
    data.save_collection(LOCAL_USER, "Jazz", "loved", 1)
        .unwrap();
    let mut incoming = data.clone();
    incoming.collections[0].name = "New".into();
    incoming.collections[0].updated_at = 2;
    crate::user::merge(&mut data, incoming.clone());
    assert_eq!(data.collections, incoming.collections);
    data.save_collection(LOCAL_USER, "Taken", "loved", 3)
        .unwrap();
    incoming.collections[0].name = "Taken".into();
    incoming.collections[0].updated_at = 4;
    let before = data.clone();
    assert_eq!(crate::user::merge(&mut data, incoming).kept, 1);
    assert_eq!(data.collections, before.collections);
}

#[test]
fn format_three_requires_persisted_ids_and_old_writers_cannot_read_it() {
    let document = crate::json::parse(
        r#"{"format_version":3,"collections":[{"name":"Jazz","query":"loved"}]}"#,
    )
    .unwrap();
    assert!(from_json(&document).is_err());
    let legacy = crate::json::parse(
        r#"{"format_version":2,"collections":[{"name":"Jazz","query":"loved"}]}"#,
    )
    .unwrap();
    let data = from_json(&legacy).unwrap();
    let saved = to_json(&data);
    assert_eq!(saved.field_u32("format_version"), Some(3));
    assert!(
        saved.get("collections").unwrap().as_arr().unwrap()[0]
            .field_str("id")
            .is_some()
    );
    // The previous reader accepts only 1/2, so the new writer's version is essential.
    assert!(!matches!(saved.field_u32("format_version").unwrap(), 1 | 2));
}
