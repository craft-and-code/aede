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
