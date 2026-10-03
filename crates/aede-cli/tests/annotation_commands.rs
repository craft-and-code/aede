//! Less common personal commands keep their output and mutation contracts.

#[path = "annotation_command_support/mod.rs"]
mod support;

use aede_core::json::{self, Json};
use aede_core::model::EntityKind;
use aede_core::user::{EntityRef, LOCAL_USER};
use support::PersonalLibrary;

#[test]
fn an_empty_favourites_export_keeps_its_shape_and_excludes_other_owners() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.entry("other", &library.track, 1).loved = true;
    library.save(&data);
    let before = library.bytes();
    for command in ["favourites", "favorites"] {
        let result = library.run(&[command, "--json"]);
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(
            matches!(json::parse(&text), Ok(Json::Arr(rows)) if rows.is_empty()),
            "{text}"
        );
        let result = library.run(&[command, "--csv"]);
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.starts_with("kind,name,reference,"), "{text}");
    }
    let path = library.dir.join("empty.json");
    let result = library.run(&["favourites", "--json", "--output", path.to_str().unwrap()]);
    assert!(result.status.success());
    assert!(
        matches!(json::parse(&std::fs::read_to_string(path).unwrap()), Ok(Json::Arr(rows)) if rows.is_empty())
    );
    assert_eq!(library.bytes(), before);
    let invalid_window = library.run(&["favourites", "--limit=0"]);
    assert!(!invalid_window.status.success());
}

#[test]
fn manual_listens_refuse_non_track_references_before_changing_the_store() {
    let library = PersonalLibrary::new();
    let before = library.bytes();
    for kind in [
        EntityKind::Release,
        EntityKind::Artist,
        EntityKind::Recording,
    ] {
        let reference = EntityRef::of(&library.catalog, kind, 0).unwrap();
        for removal in [false, true] {
            let token = reference.to_token();
            let mut args = vec!["played", &token];
            if removal {
                args.push("--remove");
            }
            let result = library.run(&args);
            assert!(
                !result.status.success(),
                "accepted {:?} as a track",
                reference
            );
            assert_eq!(library.bytes(), before);
        }
    }
    let result = library.run(&["played", &library.track.to_token()]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data = library.data();
    assert_eq!(data.play_count(LOCAL_USER, &library.track), 1);
    assert_eq!(data.plays[0].track.kind, EntityKind::Track);
}

#[test]
fn collection_edits_refuse_listing_options_instead_of_silently_ignoring_them() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.save_collection(LOCAL_USER, "Été — 音楽", "title:Song", 1);
    library.save(&data);
    let before = library.bytes();
    let output = library.dir.join("ignored.json");
    let output_option = format!("--output={}", output.display());
    for option in [
        "--json",
        "--csv",
        "--m3u",
        "--limit=1",
        "--offset=1",
        "--all",
        "--sort=title",
        "--separator=;",
        output_option.as_str(),
    ] {
        for operation in ["--query=title:Other", "--remove"] {
            let result = library.run(&["collection", "Été — 音楽", operation, option]);
            assert!(
                !result.status.success(),
                "ignored {option} during {operation}"
            );
            assert_eq!(library.bytes(), before);
            assert!(!output.exists());
        }
    }
    let result = library.run(&["collection", "été 音楽", "--json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        matches!(json::parse(&String::from_utf8(result.stdout).unwrap()), Ok(Json::Arr(rows)) if rows.len() == 1)
    );
}
