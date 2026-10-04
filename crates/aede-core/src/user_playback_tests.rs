use super::*;

use crate::store_lock::test_support as support;

fn state(owner: &str, profile: &str) -> PlaybackState {
    PlaybackState {
        owner: owner.into(),
        profile: profile.into(),
        session_id: "player-session-one".into(),
        revision: 7,
        entries: vec![
            PlaybackEntry {
                occurrence: 1,
                track: EntityRef::new(EntityKind::Track, "/music/first.flac"),
                source: PlaybackSource {
                    size: 2048,
                    mtime: 10,
                    mtime_ns: Some(123),
                },
            },
            PlaybackEntry {
                occurrence: 2,
                track: EntityRef::new(EntityKind::Track, "/music/first.flac"),
                source: PlaybackSource {
                    size: 2048,
                    mtime: 10,
                    mtime_ns: Some(123),
                },
            },
        ],
        current_occurrence: Some(2),
        position_ms: 1234,
        settings: PlaybackSettings {
            normalize: Mode::Album,
            sample_rate: Some(48_000),
            bass_db: 1.5,
            treble_db: -2.5,
        },
        updated_at: 20,
    }
}

#[test]
fn checkpoints_roundtrip_duplicate_tracks_owner_profiles_and_processing_settings() {
    let mut data = UserData::default();
    data.set_playback_state(state("owner-one", "desktop"))
        .unwrap();
    data.set_playback_state(state("owner-one", "phone"))
        .unwrap();
    data.set_playback_state(state("owner-two", "desktop"))
        .unwrap();
    let document = crate::user::to_json(&data).to_string_pretty();
    let loaded = crate::user::from_json(&crate::json::parse(&document).unwrap()).unwrap();
    assert_eq!(loaded.playback_states, data.playback_states);
    assert_eq!(
        loaded
            .playback_state("owner-one", "desktop")
            .unwrap()
            .current_occurrence,
        Some(2)
    );
    assert!(loaded.playback_state("other-owner", "desktop").is_none());
    assert!(loaded.plays.is_empty());
    assert!(loaded.counts.is_empty());

    let mut completed = state("owner-one", "desktop");
    completed.current_occurrence = None;
    completed.position_ms = 0;
    data.set_playback_state(completed).unwrap();
    let loaded = crate::user::from_json(&crate::user::to_json(&data)).unwrap();
    assert_eq!(
        loaded
            .playback_state("owner-one", "desktop")
            .unwrap()
            .current_occurrence,
        None
    );
}

#[test]
fn editing_and_clearing_a_profile_never_changes_another_owner_or_device() {
    let mut data = UserData::default();
    for (owner, profile) in [
        ("owner-one", "desktop"),
        ("owner-one", "phone"),
        ("owner-two", "desktop"),
    ] {
        data.set_playback_state(state(owner, profile)).unwrap();
    }
    let mut updated = state("owner-one", "desktop");
    updated.session_id = "replacement-session".into();
    updated.position_ms = 5000;
    updated.revision = 8;
    data.set_playback_state(updated.clone()).unwrap();
    assert_eq!(data.playback_state("owner-one", "desktop"), Some(&updated));
    assert_eq!(
        data.playback_state("owner-two", "desktop"),
        Some(&state("owner-two", "desktop"))
    );
    assert!(data.clear_playback_state("owner-one", "desktop"));
    assert!(!data.clear_playback_state("owner-one", "desktop"));
    assert!(data.playback_state("owner-one", "phone").is_some());
    assert!(data.playback_state("owner-two", "desktop").is_some());
}

#[test]
fn malformed_checkpoints_are_refused_without_mutating_the_original() {
    let mut data = UserData::default();
    data.set_playback_state(state("owner-one", "desktop"))
        .unwrap();
    let before = data.playback_states.clone();
    let mut invalids = Vec::new();
    let mut invalid = state("owner-one", "desktop");
    invalid.profile = "../desktop".into();
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.session_id = "contains spaces".into();
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.revision = 0;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.revision = MAX_EXACT_INTEGER + 1;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.entries[1].occurrence = 1;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.current_occurrence = Some(3);
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.current_occurrence = None;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.entries[0].track.kind = EntityKind::Artist;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.entries[0].source.mtime_ns = Some(1_000_000_000);
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.settings.bass_db = f32::NAN;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.settings.treble_db = 12.01;
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.settings.sample_rate = Some(192_001);
    invalids.push(invalid);
    let mut invalid = state("owner-one", "desktop");
    invalid.owner = " \t".into();
    invalids.push(invalid);
    for invalid in invalids {
        assert!(data.set_playback_state(invalid).is_err());
        assert_eq!(data.playback_states, before);
    }
}

#[test]
fn exact_integer_extremes_survive_json_and_nonintegral_optional_fields_are_refused() {
    let mut checkpoint = state("owner-one", "desktop");
    checkpoint.revision = MAX_EXACT_INTEGER;
    checkpoint.entries[1].occurrence = MAX_EXACT_INTEGER;
    checkpoint.current_occurrence = Some(MAX_EXACT_INTEGER);
    checkpoint.position_ms = MAX_EXACT_INTEGER;
    let mut data = UserData::default();
    data.set_playback_state(checkpoint).unwrap();
    let mut document = crate::user::to_json(&data);
    let roundtrip =
        crate::user::from_json(&crate::json::parse(&document.to_string_compact()).unwrap())
            .unwrap();
    assert_eq!(roundtrip.playback_states, data.playback_states);
    let Json::Arr(rows) = document.get("playback_states").unwrap().clone() else {
        panic!("states array");
    };
    for (field, value) in [
        ("current_occurrence", Json::Null),
        ("current_occurrence", 1.5f64.into()),
        ("revision", (MAX_EXACT_INTEGER + 1).into()),
        ("position_ms", "1234".into()),
    ] {
        let mut row = rows[0].clone();
        row.set(field, value);
        document.set("playback_states", Json::Arr(vec![row]));
        assert!(crate::user::from_json(&document).is_err(), "{field}");
    }
}

#[test]
fn legacy_personal_stores_need_no_checkpoint_and_malformed_tables_fail_closed() {
    let data = UserData::default();
    let mut document = crate::user::to_json(&data);
    assert!(document.get("playback_states").is_none());
    for version in [1, crate::user::USER_FORMAT_VERSION] {
        document.set("format_version", version.into());
        assert!(
            crate::user::from_json(&document)
                .unwrap()
                .playback_states
                .is_empty()
        );
    }
    document.set("playback_states", Json::Null);
    assert!(crate::user::from_json(&document).is_err());
    document.set("playback_states", Json::Arr(vec![Json::obj()]));
    assert!(crate::user::from_json(&document).is_err());
    let checkpoint = to_json(&state("owner-one", "desktop"));
    document.set(
        "playback_states",
        Json::Arr(vec![checkpoint.clone(), checkpoint]),
    );
    assert!(crate::user::from_json(&document).is_err());
}

#[test]
fn profile_and_queue_limits_refuse_growth_but_allow_updating_an_existing_profile() {
    let mut data = UserData::default();
    for index in 0..PLAYBACK_PROFILE_LIMIT {
        data.set_playback_state(state("owner-one", &format!("device-{index}")))
            .unwrap();
    }
    assert!(
        data.set_playback_state(state("owner-one", "extra"))
            .is_err()
    );
    let mut updated = state("owner-one", "device-0");
    updated.position_ms = 5000;
    data.set_playback_state(updated).unwrap();
    data.set_playback_state(state("owner-two", "extra"))
        .unwrap();
    let mut expanded = state("owner-two", "queue");
    expanded.entries = (1..=PLAYBACK_ENTRY_LIMIT as u64)
        .map(|occurrence| {
            let mut entry = expanded.entries[0].clone();
            entry.occurrence = occurrence;
            entry
        })
        .collect();
    data.set_playback_state(expanded.clone()).unwrap();
    let mut extra = expanded.entries[0].clone();
    extra.occurrence = PLAYBACK_ENTRY_LIMIT as u64 + 1;
    expanded.entries.push(extra);
    assert!(data.set_playback_state(expanded).is_err());
    assert!(valid_playback_profile(&"a".repeat(64)));
    assert!(!valid_playback_profile(&"a".repeat(65)));
    assert!(!valid_playback_profile(""));
    assert!(!valid_playback_profile("téléphone"));
}

#[test]
fn a_full_checkpoint_store_refuses_new_devices_and_imports_preserve_the_old_file_on_overflow() {
    let directory = support::Directory::new("playback_full_store");
    let path = crate::user::user_path(directory.path());
    let mut data = UserData::default();
    for index in 0..PLAYBACK_STATE_LIMIT {
        data.playback_states
            .push(state(&format!("owner-{index}"), "desktop"));
    }
    crate::user::save(&data, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        data.set_playback_state(state("extra-owner", "desktop"))
            .is_err()
    );
    let mut updated = state("owner-0", "desktop");
    updated.position_ms = 2000;
    data.set_playback_state(updated).unwrap();
    assert_eq!(data.playback_states.len(), PLAYBACK_STATE_LIMIT);
    let mut incoming = UserData::default();
    incoming
        .set_playback_state(state("extra-owner", "desktop"))
        .unwrap();
    let report = crate::user::merge(&mut data, incoming);
    assert_eq!(
        report.added, 1,
        "an import must not silently discard an incoming profile"
    );
    assert!(crate::user::save(&data, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn imports_keep_current_profiles_and_add_missing_devices_without_replaying_audio() {
    let mut data = UserData::default();
    data.set_playback_state(state("owner-one", "desktop"))
        .unwrap();
    let mut incoming = UserData::default();
    let mut imported = state("owner-one", "desktop");
    imported.updated_at = 100;
    imported.position_ms = 5;
    incoming.set_playback_state(imported).unwrap();
    incoming
        .set_playback_state(state("owner-one", "phone"))
        .unwrap();
    let report = crate::user::merge(&mut data, incoming.clone());
    assert_eq!(report.kept, 1);
    assert_eq!(report.added, 1);
    assert_eq!(
        data.playback_state("owner-one", "desktop")
            .unwrap()
            .position_ms,
        1234
    );
    let repeated = crate::user::merge(&mut data, incoming);
    assert_eq!(repeated.kept, 2);
    assert_eq!(repeated.added, 0);
    assert!(data.plays.is_empty());
    assert!(data.counts.is_empty());
}

#[test]
fn rescanning_does_not_relocate_or_drop_a_missing_native_player_source() {
    let mut data = UserData::default();
    let checkpoint = state("owner-one", "desktop");
    data.set_playback_state(checkpoint.clone()).unwrap();
    crate::user::reconcile(&mut data, &Catalog::default());
    assert_eq!(
        data.playback_state("owner-one", "desktop"),
        Some(&checkpoint)
    );
}

#[test]
fn source_identity_rechecks_precise_catalog_and_opened_file_evidence() {
    let directory = support::Directory::new("playback_source_identity");
    let path = directory.path().join("track.flac");
    std::fs::write(&path, b"original encoded bytes").unwrap();
    let metadata = std::fs::File::open(&path).unwrap().metadata().unwrap();
    let source = PlaybackSource::from_metadata(&metadata).unwrap();
    assert!(source.matches_metadata(&metadata));
    let file = AudioFile {
        path: path.to_string_lossy().into_owned(),
        size: source.size,
        mtime: source.mtime,
        ..Default::default()
    };
    let mut catalog = Catalog::default();
    catalog
        .file_mtime_subseconds
        .insert(file.path.clone(), source.mtime_ns.unwrap());
    assert_eq!(PlaybackSource::from_catalog(&catalog, &file), source);
    assert!(source.matches_catalog(&catalog, &file));
    catalog.file_mtime_subseconds.insert(
        file.path.clone(),
        (source.mtime_ns.unwrap() + 1) % 1_000_000_000,
    );
    assert!(!source.matches_catalog(&catalog, &file));
    let mut second_only = source.clone();
    second_only.mtime_ns = None;
    assert!(second_only.matches_catalog(&catalog, &file));
    std::fs::write(&path, b"replacement with a different size").unwrap();
    assert!(!source.matches_metadata(&std::fs::metadata(&path).unwrap()));
    assert!(PlaybackSource::from_metadata(&std::fs::metadata(directory.path()).unwrap()).is_err());
}

#[test]
fn personal_store_saves_checkpoints_atomically_and_backup_restores_the_nested_state() {
    let directory = support::Directory::new("playback_checkpoint_store");
    let path = crate::user::user_path(directory.path());
    let mut data = UserData::default();
    data.set_playback_state(state("owner-one", "desktop"))
        .unwrap();
    crate::user::save(&data, &path).unwrap();
    assert_eq!(
        crate::user::load(&path).unwrap().unwrap().playback_states,
        data.playback_states
    );
    let before = std::fs::read(&path).unwrap();
    data.playback_states[0].current_occurrence = Some(999);
    assert!(crate::user::save(&data, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let data = crate::user::load(&path).unwrap().unwrap();
    let backup = crate::backup::Backup {
        made_at: 30,
        made_by: "test".into(),
        catalog: crate::backup::Part::Empty,
        conclusions: crate::backup::Part::Empty,
        user: crate::backup::Part::Held(data.clone()),
        sources: crate::backup::Part::Empty,
        accounts: crate::backup::Part::Empty,
    };
    let restored = crate::backup::from_json(&crate::backup::to_json(&backup)).unwrap();
    assert_eq!(
        restored.user.held().unwrap().playback_states,
        data.playback_states
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}
