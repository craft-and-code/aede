use super::*;

#[test]
fn a_printed_name_is_one_safe_shell_argument() {
    assert_eq!(shell_arg("Guns N' Roses"), "'Guns N'\\'' Roses'");
}

#[test]
fn graph_pages_prefer_identifiers_that_remove_title_ambiguity() {
    let mut catalog = Catalog::default();
    catalog.recordings.push(aede_core::model::Recording {
        id: 0,
        title: "Same Title".into(),
        mbid: Some("recording-id".into()),
        ..Default::default()
    });
    let recording = &catalog.recordings[0];
    let command = open_command(&catalog, EntityKind::Recording, recording.id).expect("command");
    assert_eq!(command, "aede recording 'recording-id'");
}

#[test]
fn local_recording_commands_keep_same_title_performances_reachable() {
    let mut catalog = Catalog::default();
    let paths = [
        "/music/Studio/01.flac".to_string(),
        format!(
            "/music/{}/Live's edition/01.flac",
            "long-folder/".repeat(12)
        ),
    ];
    for (id, path) in paths.iter().enumerate() {
        let id = id as Id;
        catalog.files.push(aede_core::model::AudioFile {
            id,
            path: path.clone(),
            ..Default::default()
        });
        catalog.tracks.push(aede_core::model::Track {
            id,
            file_id: id,
            recording_id: id,
            ..Default::default()
        });
        catalog.recordings.push(aede_core::model::Recording {
            id,
            title: "Same Song".into(),
            key: "same song".into(),
            track_ids: vec![id],
            ..Default::default()
        });
    }
    let mut navigation = Navigation::default();
    for (id, path) in paths.iter().enumerate() {
        let id = id as Id;
        let reference = format!("local:{path}");
        let command = open_command(&catalog, EntityKind::Recording, id).expect("command");
        assert_eq!(command, format!("aede recording {}", shell_arg(&reference)));
        assert_eq!(
            catalog
                .find_recordings(&reference)
                .iter()
                .map(|recording| recording.id)
                .collect::<Vec<_>>(),
            [id]
        );
        navigation.entity(&catalog, "Recording", EntityKind::Recording, id);
    }
    assert_eq!(navigation.rows.len(), 2, "each performance stays reachable");
    let rendered = navigation.render();
    for path in paths {
        let command = format!("aede recording {}", shell_arg(&format!("local:{path}")));
        assert!(rendered.contains(&command), "complete command: {rendered}");
    }
}

#[test]
fn credited_artists_open_by_musicbrainz_id_even_when_absent_from_tags() {
    let mut navigation = Navigation::default();
    navigation.source_artist("artist-id");
    assert_eq!(
        navigation.rows,
        vec![("Credited artist".into(), "aede artist 'artist-id'".into())]
    );
    navigation.source_artist("artist-id");
    assert_eq!(navigation.rows.len(), 1);
}
