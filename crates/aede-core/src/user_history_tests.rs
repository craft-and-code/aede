use super::test_support::{TemporaryUser, library};
use super::*;

fn play(owner: &str, at: u64) -> Play {
    Play {
        owner: owner.into(),
        track: EntityRef::new(EntityKind::Track, "/music/01.flac"),
        at,
        ms_played: 1000,
        completed: true,
    }
}
fn scrobble(owner: &str, at_ms: u64) -> Scrobble {
    Scrobble {
        owner: owner.into(),
        track: EntityRef::new(EntityKind::Track, "/music/01.flac"),
        at_ms,
    }
}

#[test]
fn another_accounts_native_listens_never_evict_the_owners_recent_history() {
    let mut data = UserData::default();
    let alice = play("alice", 1);
    data.record_play(alice.clone());
    for at in 2..HISTORY_LIMIT as u64 + 42 {
        data.record_play(play("bob", at));
    }
    assert_eq!(
        data.plays
            .iter()
            .filter(|row| row.owner == "alice")
            .collect::<Vec<_>>(),
        vec![&alice]
    );
    assert_eq!(
        data.plays.iter().filter(|row| row.owner == "bob").count(),
        HISTORY_LIMIT
    );
    assert_eq!(data.play_count("alice", &alice.track), 1);
    assert_eq!(
        data.play_count("bob", &alice.track),
        HISTORY_LIMIT as u32 + 40
    );
    let restored = from_json(&to_json(&data)).unwrap();
    assert_eq!(restored.plays, data.plays);
}

#[test]
fn another_accounts_client_scrobbles_never_evict_the_owners_recent_declarations() {
    let mut data = UserData::default();
    let alice = scrobble("alice", 1);
    data.record_scrobble(alice.clone()).unwrap();
    for at in 2..HISTORY_LIMIT as u64 + 42 {
        data.record_scrobble(scrobble("bob", at)).unwrap();
    }
    assert_eq!(
        data.scrobbles
            .iter()
            .filter(|row| row.owner == "alice")
            .collect::<Vec<_>>(),
        vec![&alice]
    );
    assert_eq!(
        data.scrobbles
            .iter()
            .filter(|row| row.owner == "bob")
            .count(),
        HISTORY_LIMIT
    );
    assert_eq!(data.play_count("alice", &alice.track), 1);
    assert_eq!(
        data.play_count("bob", &alice.track),
        HISTORY_LIMIT as u32 + 40
    );
    let restored = from_json(&to_json(&data)).unwrap();
    assert_eq!(restored.scrobbles, data.scrobbles);
}

#[test]
fn merging_one_accounts_history_preserves_other_owners_and_per_owner_retention() {
    let mut data = UserData::default();
    let alice = play("alice", 1);
    let declaration = scrobble("alice", 1);
    data.record_play(alice.clone());
    data.record_scrobble(declaration.clone()).unwrap();
    let mut incoming = UserData::default();
    for at in 2..HISTORY_LIMIT as u64 + 42 {
        incoming.record_play(play("bob", at));
        incoming.record_scrobble(scrobble("bob", at)).unwrap();
    }
    merge(&mut data, incoming);
    assert_eq!(
        data.plays
            .iter()
            .filter(|row| row.owner == "alice")
            .collect::<Vec<_>>(),
        vec![&alice]
    );
    assert_eq!(
        data.scrobbles
            .iter()
            .filter(|row| row.owner == "alice")
            .collect::<Vec<_>>(),
        vec![&declaration]
    );
    assert_eq!(
        data.plays.iter().filter(|row| row.owner == "bob").count(),
        HISTORY_LIMIT
    );
    assert_eq!(
        data.scrobbles
            .iter()
            .filter(|row| row.owner == "bob")
            .count(),
        HISTORY_LIMIT
    );
    let restored = from_json(&to_json(&data)).unwrap();
    assert_eq!(restored.plays, data.plays);
    assert_eq!(restored.scrobbles, data.scrobbles);
}

#[test]
fn embedded_personal_documents_refuse_the_same_byte_budget_as_disk_stores() {
    let mut document = to_json(&UserData::default());
    document.set(
        "future_field",
        crate::json::Json::Str("x".repeat(USER_MAX_BYTES)),
    );
    assert!(matches!(
        from_json(&document),
        Err(crate::store::StoreError::Invalid(
            "personal store byte limit exceeded"
        ))
    ));
    let mut envelope = crate::json::Json::obj();
    envelope.set(
        "format_version",
        crate::backup::BACKUP_FORMAT_VERSION.into(),
    );
    envelope.set(
        "catalog",
        crate::store::to_json(&library(&["/music/01.flac"])),
    );
    envelope.set("user", document);
    let backup = crate::backup::from_json(&envelope).unwrap();
    assert!(
        matches!(backup.user, crate::backup::Part::Unreadable(ref reason) if reason.contains("personal store byte limit exceeded"))
    );
    assert!(backup.catalog.held().is_some());
}

#[test]
fn scan_reconciliation_and_backup_roundtrip_preserve_both_owners_retained_journals() {
    let fixture = TemporaryUser::new();
    let catalog = library(&["/music/01.flac"]);
    let mut data = UserData::default();
    for at in 1..=HISTORY_LIMIT as u64 {
        data.record_play(play("alice", at));
        data.record_scrobble(scrobble("bob", at * 1000)).unwrap();
    }
    save(&data, &fixture.path()).unwrap();
    let mut loaded = load(&fixture.path()).unwrap().unwrap();
    reconcile(&mut loaded, &catalog);
    save(&loaded, &fixture.path()).unwrap();
    let reread = load(&fixture.path()).unwrap().unwrap();
    assert_eq!(reread.plays, data.plays);
    assert_eq!(reread.scrobbles, data.scrobbles);
    let backup = crate::backup::Backup {
        made_at: 100,
        made_by: "test".into(),
        catalog: crate::backup::Part::Held(catalog),
        conclusions: crate::backup::Part::Empty,
        user: crate::backup::Part::Held(reread),
        sources: crate::backup::Part::Empty,
        accounts: crate::backup::Part::Empty,
        parental: crate::backup::Part::Empty,
    };
    let restored = crate::backup::from_json(&crate::backup::to_json(&backup)).unwrap();
    let user = restored.user.held().unwrap();
    assert_eq!(user.plays, data.plays);
    assert_eq!(user.scrobbles, data.scrobbles);
    assert_eq!(
        user.play_count("alice", &data.plays[0].track),
        HISTORY_LIMIT as u32
    );
    assert_eq!(
        user.play_count("bob", &data.plays[0].track),
        HISTORY_LIMIT as u32
    );
}

#[test]
fn oversized_owner_journals_are_refused_without_trimming_or_replacing_the_store() {
    let fixture = TemporaryUser::new();
    let mut original = UserData::default();
    original.record_play(play("alice", 1));
    save(&original, &fixture.path()).unwrap();
    let bytes = std::fs::read(fixture.path()).unwrap();
    for scrobbles in [false, true] {
        let mut oversized = original.clone();
        for at in 1..=HISTORY_LIMIT as u64 + 1 {
            if scrobbles {
                oversized.scrobbles.push(scrobble("bob", at));
            } else {
                oversized.plays.push(play("bob", at));
            }
        }
        assert!(from_json(&to_json(&oversized)).is_err());
        assert!(save(&oversized, &fixture.path()).is_err());
        assert_eq!(std::fs::read(fixture.path()).unwrap(), bytes);
        assert_eq!(
            load(&fixture.path()).unwrap().unwrap().plays,
            original.plays
        );
    }
}

#[test]
fn global_journal_capacity_refuses_publication_instead_of_evicting_another_owner() {
    let fixture = TemporaryUser::new();
    let mut data = UserData::default();
    for owner in 0..aede_accounts::MAX_ACCOUNTS {
        for at in 1..=HISTORY_LIMIT as u64 {
            data.plays.push(play(&format!("owner-{owner}"), at));
        }
    }
    assert_eq!(data.plays.len(), HISTORY_STORE_LIMIT);
    data.plays.sort_by_key(|event| event.at);
    save(&data, &fixture.path()).unwrap();
    let before = std::fs::read(fixture.path()).unwrap();
    let restored = load(&fixture.path()).unwrap().unwrap();
    assert_eq!(restored.plays, data.plays);
    data.record_play(play("extra-owner", 1));
    assert_eq!(data.plays.len(), HISTORY_STORE_LIMIT + 1);
    assert!(save(&data, &fixture.path()).is_err());
    assert!(from_json(&to_json(&data)).is_err());
    assert_eq!(std::fs::read(fixture.path()).unwrap(), before);
    // The second journal has the same independent global bound.
    data.plays.clear();
    for owner in 0..aede_accounts::MAX_ACCOUNTS {
        for at in 1..=HISTORY_LIMIT as u64 {
            data.scrobbles.push(scrobble(&format!("owner-{owner}"), at));
        }
    }
    assert!(history::validate(&data).is_ok());
    data.record_scrobble(scrobble("extra-owner", 1)).unwrap();
    assert!(history::validate(&data).is_err());
    assert!(save(&data, &fixture.path()).is_err());
    assert!(from_json(&to_json(&data)).is_err());
    assert_eq!(std::fs::read(fixture.path()).unwrap(), before);
}

#[test]
fn oversized_user_input_and_output_are_refused_with_existing_bytes_intact() {
    use std::io::Write;
    let fixture = TemporaryUser::new();
    let mut data = UserData::default();
    data.record_play(play("alice", 1));
    save(&data, &fixture.path()).unwrap();
    let before = std::fs::read(fixture.path()).unwrap();
    data.entry(
        "bob",
        &EntityRef::new(EntityKind::Track, "/music/01.flac"),
        1,
    )
    .note = Some("x\n".repeat(USER_MAX_BYTES / 3 + 1));
    assert!(save(&data, &fixture.path()).is_err());
    assert_eq!(std::fs::read(fixture.path()).unwrap(), before);
    let mut file = std::fs::File::create(fixture.path()).unwrap();
    file.write_all(&before).unwrap();
    let padding = [b' '; 64 * 1024];
    let mut remaining = USER_MAX_BYTES - before.len();
    while remaining > 0 {
        let length = remaining.min(padding.len());
        file.write_all(&padding[..length]).unwrap();
        remaining -= length;
    }
    file.write_all("💿".as_bytes()).unwrap();
    drop(file);
    assert!(matches!(
        load(&fixture.path()),
        Err(crate::store::StoreError::Invalid(
            "personal store byte limit exceeded"
        ))
    ));
    assert_eq!(
        std::fs::metadata(fixture.path()).unwrap().len(),
        USER_MAX_BYTES as u64 + 4
    );
}

#[test]
fn confirmed_completion_survives_retention_and_roundtrip_without_crossing_owners() {
    let mut data = UserData::default();
    let first = play("alice", 1);
    data.record_play(first.clone());
    for at in 2..HISTORY_LIMIT as u64 + 3 {
        let mut next = play("alice", at);
        next.track = EntityRef::new(EntityKind::Track, format!("/music/{at}.flac"));
        next.completed = false;
        data.record_play(next);
    }
    assert!(!data.plays.iter().any(|p| p.track == first.track));
    let restored = from_json(&to_json(&data)).unwrap();
    let count = restored
        .counts
        .iter()
        .find(|p| p.owner == "alice" && p.track == first.track)
        .unwrap();
    assert_eq!(count.completed_count, 1);
    assert!(!restored.counts.iter().any(|p| p.owner == "bob"));
}

#[test]
fn legacy_completed_visits_seed_counters_but_partial_visits_and_scrobbles_do_not() {
    let mut data = UserData::default();
    data.record_play(play("alice", 1));
    data.record_scrobble(scrobble("bob", 1)).unwrap();
    let mut legacy = to_json(&data);
    if let crate::json::Json::Obj(root) = &mut legacy
        && let Some(crate::json::Json::Arr(rows)) = root.get_mut("counts")
    {
        for row in rows {
            if let crate::json::Json::Obj(fields) = row {
                fields.remove("completed_count");
            }
        }
    }
    let restored = from_json(&legacy).unwrap();
    assert_eq!(
        restored
            .counts
            .iter()
            .find(|p| p.owner == "alice")
            .unwrap()
            .completed_count,
        1
    );
    assert_eq!(
        restored
            .counts
            .iter()
            .find(|p| p.owner == "bob")
            .unwrap()
            .completed_count,
        0
    );
}
