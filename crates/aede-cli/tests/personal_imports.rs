//! Personal interchange refuses data outside the selected bundle contract.

#[path = "annotation_command_support/mod.rs"]
mod support;

use aede_core::json::Json;
use aede_core::user::{
    self, ClientPreference, LOCAL_USER, PlaybackSettings, PlaybackState, Scrobble, UserData,
};
use support::PersonalLibrary;

#[test]
fn personal_import_refuses_raw_bytes_beyond_the_store_budget_before_writing() {
    use std::io::Write;

    let library = PersonalLibrary::new();
    let before = library.bytes();
    let path = library.dir.join("oversized-personal.json");
    let mut file = std::fs::File::create(&path).unwrap();
    let mut incoming = UserData::default();
    incoming.entry(LOCAL_USER, &library.track, 10).note = Some("Do not import".into());
    file.write_all(user::to_json(&incoming).to_string_pretty().as_bytes())
        .unwrap();
    // Valid JSON whitespace must count toward the input budget, without a
    // giant allocation in the test fixture itself.
    let padding = [b' '; 64 * 1024];
    for _ in 0..user::USER_MAX_BYTES / padding.len() {
        file.write_all(&padding).unwrap();
    }
    drop(file);
    let result = library.run(&["notes", "--import", path.to_str().unwrap()]);
    assert!(
        !result.status.success(),
        "accepted an input beyond the personal-store byte budget"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("byte limit"));
    assert_eq!(library.bytes(), before);
}

#[test]
fn rules_import_refuses_private_media_state_without_changing_either_store() {
    for table in [
        "client_preferences",
        "playlists",
        "scrobbles",
        "playback_states",
    ] {
        let library = PersonalLibrary::new();
        let mut existing = library.data();
        let target =
            user::EntityRef::of(&library.catalog, aede_core::model::EntityKind::Track, 0).unwrap();
        existing.entry("another-owner", &target, 1).note = Some("Keep this private".into());
        library.save(&existing);
        let mut incoming = UserData::default();
        match table {
            "client_preferences" => incoming.client_preferences.push(ClientPreference {
                owner: LOCAL_USER.into(),
                key: "desktop.layout".into(),
                value: "{\"mode\":\"list\"}".into(),
            }),
            "playlists" => {
                incoming
                    .create_playlist(LOCAL_USER, "Imported list", vec![library.track.clone()], 1)
                    .unwrap();
            }
            "scrobbles" => incoming.scrobbles.push(Scrobble {
                owner: LOCAL_USER.into(),
                track: library.track.clone(),
                at_ms: 1000,
            }),
            "playback_states" => incoming
                .set_playback_state(PlaybackState {
                    owner: LOCAL_USER.into(),
                    profile: "desktop".into(),
                    session_id: "imported-session".into(),
                    revision: 1,
                    entries: vec![],
                    current_occurrence: None,
                    position_ms: 0,
                    settings: PlaybackSettings::default(),
                    updated_at: 1,
                })
                .unwrap(),
            _ => unreachable!(),
        }
        let personal = user::to_json(&incoming);
        user::from_json(&personal).expect("the injected table is valid personal data");
        let sources = aede_core::sources::Sources::default();
        let sources_path = aede_core::sources::sources_path(&library.dir);
        aede_core::sources::save(&sources, &sources_path).unwrap();
        let before_user = library.bytes();
        let before_sources = std::fs::read(&sources_path).unwrap();
        let mut root = Json::obj();
        root.set("format", "aede-rules".into());
        root.set("format_version", 1u32.into());
        root.set("user", personal);
        root.set("sources", aede_core::sources::to_json(&sources));
        let path = library.dir.join("unexpected-private-data.json");
        std::fs::write(&path, root.to_string_pretty()).unwrap();

        let result = library.run(&["rules", "--import", path.to_str().unwrap()]);
        assert!(
            !result.status.success(),
            "rules imported forbidden table {table}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("use notes or backup"));
        assert_eq!(
            library.bytes(),
            before_user,
            "user store changed for {table}"
        );
        assert_eq!(
            std::fs::read(&sources_path).unwrap(),
            before_sources,
            "source store changed for {table}"
        );
    }
}
