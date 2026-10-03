use super::*;

#[test]
fn imports_keep_distinct_snapshots_with_colliding_origin_sequences() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let make = |note: &str| {
        let mut data = UserData::default();
        data.entry(LOCAL_USER, &from, 1).note = Some(note.into());
        data.relink(LOCAL_USER, &from, &to, &catalog, 2).unwrap();
        data
    };
    let mut local = make("from the first store");
    let incoming = make("from the second store");
    merge(&mut local, incoming.clone());
    assert_eq!(local.relinks.len(), 2);
    let before = to_json(&local);
    merge(&mut local, incoming);
    assert_eq!(to_json(&local), before);
}

#[test]
fn manual_relink_moves_one_owners_data_and_round_trips_undo() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let mut data = UserData::default();
    for owner in [LOCAL_USER, "other"] {
        data.entry(owner, &from, 1).note = Some(owner.into());
    }
    data.record_play(Play {
        owner: LOCAL_USER.into(),
        track: from.clone(),
        at: 1,
        ms_played: 100,
        completed: true,
    });
    let id = data.relink(LOCAL_USER, &from, &to, &catalog, 2).unwrap();
    assert!(data.find(LOCAL_USER, &from).is_none());
    assert_eq!(
        data.find("other", &from).unwrap().note.as_deref(),
        Some("other")
    );
    let mut restored = from_json(&to_json(&data)).unwrap();
    restored.undo_relink(LOCAL_USER, id, 3).unwrap();
    assert_eq!(
        restored.find(LOCAL_USER, &from).unwrap().note.as_deref(),
        Some(LOCAL_USER)
    );
    assert_eq!(restored.plays[0].track, from);
    assert_eq!(restored.counts[0].count, 1);
    assert_eq!(restored.relinks[0].undone_at, Some(3));
    reconcile(&mut restored, &catalog);
    assert!(
        restored.find(LOCAL_USER, &from).is_some(),
        "undo survives a later read"
    );
}

#[test]
fn previews_do_not_change_data_and_undo_refuses_new_listens_or_source_records() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let mut data = UserData::default();
    data.entry(LOCAL_USER, &from, 1).note = Some("original".into());
    reconcile(
        &mut data,
        &super::super::test_support::library(&["/old/a.flac"]),
    );
    let before = to_json(&data);
    assert_eq!(
        data.preview_relink(LOCAL_USER, &from, &to, &catalog)
            .unwrap()
            .annotations,
        1
    );
    assert_eq!(to_json(&data), before);
    let id = data.relink(LOCAL_USER, &from, &to, &catalog, 2).unwrap();
    let before = to_json(&data);
    assert!(data.preview_undo_relink(LOCAL_USER, id).is_ok());
    assert_eq!(to_json(&data), before);
    data.record_play(Play {
        owner: LOCAL_USER.into(),
        track: to.clone(),
        at: 3,
        ms_played: 100,
        completed: true,
    });
    let before = to_json(&data);
    assert!(data.undo_relink(LOCAL_USER, id, 4).is_err());
    assert_eq!(to_json(&data), before);
    data.plays.clear();
    data.counts.clear();
    data.entry(LOCAL_USER, &from, 3).loved = true;
    let before = to_json(&data);
    assert!(data.undo_relink(LOCAL_USER, id, 4).is_err());
    assert_eq!(to_json(&data), before);
}

#[test]
fn importing_relink_history_never_replays_it_and_is_idempotent() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let mut original = UserData::default();
    original.entry(LOCAL_USER, &from, 1).note = Some("keep me".into());
    original
        .relink(LOCAL_USER, &from, &to, &catalog, 2)
        .unwrap();
    let history = UserData {
        relinks: original.relinks.clone(),
        ..Default::default()
    };
    let mut target = UserData::default();
    target.entry(LOCAL_USER, &from, 3).note = Some("local".into());
    merge(&mut target, history.clone());
    assert_eq!(
        target.find(LOCAL_USER, &from).unwrap().note.as_deref(),
        Some("local")
    );
    assert!(target.find(LOCAL_USER, &to).is_none());
    let before = to_json(&target);
    merge(&mut target, history);
    assert_eq!(to_json(&target), before);
    assert!(target.preview_undo_relink(LOCAL_USER, 1).is_err());
}

#[test]
fn relation_endpoints_and_notes_follow_a_manual_relink_and_undo() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let edge = crate::graph::RelationRef {
        source: from.clone(),
        kind: "credit:producer".into(),
        target: EntityRef::new(EntityKind::Artist, "deicide"),
        provenance: "tags".into(),
        source_id: None,
    };
    let mut data = UserData::default();
    data.relation_entry(LOCAL_USER, &edge, 1).note = Some("keep the credit note".into());
    assert_eq!(
        data.preview_relink(LOCAL_USER, &from, &to, &catalog)
            .unwrap()
            .relations,
        1
    );
    let id = data.relink(LOCAL_USER, &from, &to, &catalog, 2).unwrap();
    assert_eq!(data.relation_annotations[0].relation.source, to);
    assert_eq!(
        data.relation_annotations[0].note.as_deref(),
        Some("keep the credit note")
    );
    data.undo_relink(LOCAL_USER, id, 3).unwrap();
    assert_eq!(data.relation_annotations[0].relation, edge);
}

#[test]
fn relink_refuses_destination_aliases_and_different_entity_kinds() {
    let catalog = super::super::test_support::library(&["/m/Legion/Disc 2/02.flac"]);
    let source = EntityRef::new(EntityKind::Release, "deicide|legion|/old/Legion");
    let alias = EntityRef::new(EntityKind::Release, "deicide|legion|/m/Legion/Disc 1");
    let canonical = EntityRef::of(&catalog, EntityKind::Release, 0).unwrap();
    let mut data = UserData::default();
    data.entry(LOCAL_USER, &source, 1).note = Some("old edition".into());
    data.entry(LOCAL_USER, &alias, 1).note = Some("current edition".into());
    let before = to_json(&data);
    assert!(
        data.relink(LOCAL_USER, &source, &canonical, &catalog, 2)
            .is_err()
    );
    assert!(
        data.preview_relink(
            LOCAL_USER,
            &source,
            &EntityRef::of(&catalog, EntityKind::Track, 0).unwrap(),
            &catalog
        )
        .is_err()
    );
    assert_eq!(to_json(&data), before);
}

#[test]
fn separate_same_second_relinks_survive_import_and_local_id_collisions() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let mut incoming = UserData::default();
    incoming.entry(LOCAL_USER, &from, 1).note = Some("keep me".into());
    let first = incoming
        .relink(LOCAL_USER, &from, &to, &catalog, 2)
        .unwrap();
    incoming.undo_relink(LOCAL_USER, first, 2).unwrap();
    incoming
        .relink(LOCAL_USER, &from, &to, &catalog, 2)
        .unwrap();
    let mut target = UserData::default();
    let other = EntityRef::new(EntityKind::Track, "/old/other.flac");
    target.entry(LOCAL_USER, &other, 1).note = Some("another decision".into());
    target.relink(LOCAL_USER, &other, &to, &catalog, 3).unwrap();
    merge(&mut target, incoming.clone());
    assert_eq!(target.relinks.len(), 3);
    assert_eq!(
        target
            .relinks
            .iter()
            .map(|event| event.id)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    let before = to_json(&target);
    merge(&mut target, incoming);
    assert_eq!(to_json(&target), before);
}

#[test]
fn manual_relink_and_undo_refuse_conflicts_without_changing_data() {
    let catalog = super::super::test_support::library(&["/new/a.flac"]);
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
    let mut data = UserData::default();
    data.entry(LOCAL_USER, &from, 1).note = Some("original".into());
    data.entry(LOCAL_USER, &to, 1).loved = true;
    let before = to_json(&data).to_string_pretty();
    assert!(data.relink(LOCAL_USER, &from, &to, &catalog, 2).is_err());
    assert_eq!(to_json(&data).to_string_pretty(), before);
    data.annotations.retain(|row| row.target != to);
    let id = data.relink(LOCAL_USER, &from, &to, &catalog, 2).unwrap();
    data.entry(LOCAL_USER, &to, 3).note = Some("new writing".into());
    let before = to_json(&data).to_string_pretty();
    assert!(data.undo_relink("other", id, 4).is_err());
    assert!(data.undo_relink(LOCAL_USER, id, 4).is_err());
    assert_eq!(to_json(&data).to_string_pretty(), before);
    assert!(data.relink(LOCAL_USER, &to, &from, &catalog, 5).is_err());
}
