use super::super::test_support::NotesStore;
use super::*;

#[test]
fn notes_relink_preview_apply_and_undo_preserve_user_words() {
    let store = NotesStore::new();
    let options = [
        format!("--relink={}", store.from.to_token()),
        format!("--to={}", store.to.to_token()),
    ];
    let before = store.bytes();
    let mut preview = options.to_vec();
    preview.push("--dry-run".into());
    notes(&store.args(&preview)).unwrap();
    assert_eq!(store.bytes(), before);
    notes(&store.args(&options)).unwrap();
    let moved = store.data();
    assert_eq!(
        moved.find(LOCAL_USER, &store.to).unwrap().note.as_deref(),
        Some("original wording\n\n")
    );
    let id = moved.relinks[0].id;
    let before = store.bytes();
    notes(&store.args(&[format!("--undo-relink={id}"), "--dry-run".into()])).unwrap();
    assert_eq!(store.bytes(), before);
    notes(&store.args(&[format!("--undo-relink={id}")])).unwrap();
    assert_eq!(
        store
            .data()
            .find(LOCAL_USER, &store.from)
            .unwrap()
            .note
            .as_deref(),
        Some("original wording\n\n")
    );
    assert!(store.data().find(LOCAL_USER, &store.to).is_none());
}

#[test]
fn notes_relink_conflicts_leave_the_persisted_file_unchanged() {
    let store = NotesStore::new();
    let mut data = store.data();
    data.entry(LOCAL_USER, &store.to, 2).loved = true;
    user::save(&data, &user::user_path(&store.path)).unwrap();
    let before = store.bytes();
    assert!(
        notes(&store.args(&[
            format!("--relink={}", store.from.to_token()),
            format!("--to={}", store.to.to_token())
        ]))
        .is_err()
    );
    assert_eq!(store.bytes(), before);
}

#[test]
fn waiting_lists_history_only_and_relationship_references_for_one_owner() {
    let store = NotesStore::new();
    let catalog = store::load(&store::catalog_path(&store.path))
        .unwrap()
        .unwrap();
    let mut data = UserData::default();
    data.record_play(Play {
        owner: LOCAL_USER.into(),
        track: store.from.clone(),
        at: 1,
        ms_played: 100,
        completed: true,
    });
    let relation_source = EntityRef::new(EntityKind::Release, "artist|missing album|/old");
    let relation = aede_core::graph::RelationRef {
        source: relation_source.clone(),
        target: store.to.clone(),
        kind: "contains".into(),
        provenance: "tags".into(),
        source_id: None,
    };
    data.relation_entry(LOCAL_USER, &relation, 1)
        .tags
        .insert("recover".into());
    data.entry(
        "other",
        &EntityRef::new(EntityKind::Artist, "other artist"),
        1,
    )
    .note = Some("private".into());
    let rows = waiting_references(&catalog, &data, LOCAL_USER, None);
    assert_eq!(rows.len(), 2);
    let history = rows.iter().find(|row| row.reference == store.from).unwrap();
    assert_eq!(history.annotations, 0);
    assert_eq!(history.plays, 1);
    assert_eq!(history.counts, 1);
    assert_eq!(
        rows.iter()
            .find(|row| row.reference == relation_source)
            .unwrap()
            .relations,
        1
    );
    let filtered = waiting_references(&catalog, &data, LOCAL_USER, Some("recover"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].reference, relation_source);
    assert!(waiting_references(&catalog, &data, "unrelated", None).is_empty());
    user::save(&data, &user::user_path(&store.path)).unwrap();
    let before = store.bytes();
    notes(&store.args(&["--waiting".into(), "--json".into()])).unwrap();
    assert_eq!(store.bytes(), before);
}

#[test]
fn waiting_includes_private_playlist_and_client_listen_references() {
    let store = NotesStore::new();
    let catalog = store::load(&store::catalog_path(&store.path))
        .unwrap()
        .unwrap();
    let mut data = UserData::default();
    data.playlists.push(user::Playlist {
        id: format!("playlist-{}", "a".repeat(64)),
        owner: LOCAL_USER.into(),
        name: "Waiting".into(),
        comment: None,
        tracks: vec![store.from.clone(), store.from.clone()],
        created_at: 1,
        updated_at: 1,
    });
    data.scrobbles.push(user::Scrobble {
        owner: LOCAL_USER.into(),
        track: store.from.clone(),
        at_ms: 1000,
    });
    data.scrobbles.push(user::Scrobble {
        owner: "other".into(),
        track: EntityRef::new(EntityKind::Track, "/private/missing.flac"),
        at_ms: 1000,
    });
    let rows = waiting_references(&catalog, &data, LOCAL_USER, None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reference, store.from);
    assert_eq!(rows[0].playlists, 1);
    assert_eq!(rows[0].scrobbles, 1);
    assert!(waiting_references(&catalog, &data, LOCAL_USER, Some("tag")).is_empty());
}

#[test]
fn relink_destination_accepts_a_unique_name_or_an_exact_reference() {
    let store = NotesStore::new();
    let mut catalog = store::load(&store::catalog_path(&store.path))
        .unwrap()
        .unwrap();
    catalog.tracks[0].title = "One Track".into();
    assert_eq!(
        relink_destination("track:One Track", &catalog).unwrap(),
        store.to
    );
    assert_eq!(
        relink_destination(&store.to.to_token(), &catalog).unwrap(),
        store.to
    );
    let mut other = catalog.tracks[0].clone();
    other.id = 1;
    let mut file = catalog.files[0].clone();
    file.id = 1;
    file.path = "/other/a.flac".into();
    other.file_id = 1;
    catalog.tracks.push(other);
    catalog.files.push(file);
    assert!(relink_destination("track:One Track", &catalog).is_err());
    assert!(relink_destination("One Track", &catalog).is_err());
}

#[test]
fn notes_refuses_incompatible_operations_and_unused_options() {
    for options in [
        vec![
            "--relink=track:/old/a.flac",
            "--to=track:/new/a.flac",
            "--export",
        ],
        vec!["--to=track:/new/a.flac"],
        vec!["--dry-run"],
        vec![
            "--relink=track:/old/a.flac",
            "--to=track:/new/a.flac",
            "--tag=vinyl",
        ],
        vec!["--relinks", "--waiting"],
        vec![
            "--relink=track:/old/a.flac",
            "--to=track:/new/a.flac",
            "--json",
            "--csv",
        ],
        vec!["--output=/unused.txt"],
        vec!["--export", "--tag=vinyl"],
        vec!["--import=/unused.json", "--json"],
        vec!["--relinks", "--tag=vinyl"],
        vec!["--waiting", "--search=word"],
        vec!["--relinks", "--search=word"],
        vec![
            "--relink=track:/old/a.flac",
            "--to=track:/new/a.flac",
            "--csv",
            "--separator=too-long",
        ],
    ] {
        let args = Args::parse(
            ["notes".to_string()]
                .into_iter()
                .chain(options.into_iter().map(str::to_string)),
        );
        assert!(validate_notes_options(&args).is_err());
    }
}
