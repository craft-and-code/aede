use super::*;

#[path = "export_test_support.rs"]
mod test_support;
use test_support::Outputs;

#[test]
fn an_export_cannot_replace_active_stores_or_writer_locks() {
    let fixture = Outputs::new();
    for name in [
        "catalog.json",
        "user.json",
        "sources.json",
        "conclusions.json",
        ".aede.lock",
        ".aede-server.lock",
    ] {
        let path = fixture.directory.join(name);
        std::fs::write(&path, "preserve this local data").unwrap();
        assert!(
            emit(&fixture.args(&path), "partial export").is_err(),
            "replaced {name}"
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "preserve this local data"
        );
    }
    let case_alias = fixture.directory.join("USER.JSON");
    if case_alias.exists() {
        assert!(emit(&fixture.args(&case_alias), "partial export").is_err());
    }
    let missing_store = fixture.directory.join("not-created/../user.json");
    assert!(emit(&fixture.args(&missing_store), "partial export").is_err());
    assert!(
        !fixture.directory.join("not-created").exists(),
        "preflight creates no parents"
    );
    let future = fixture.directory.join("future-data");
    let args = Args::parse([
        "notes".into(),
        format!("--data={}", future.display()),
        format!("--output={}", future.join("user.json").display()),
    ]);
    assert!(emit(&args, "partial export").is_err());
    assert!(
        !future.exists(),
        "absent stores are reserved without creating the data folder"
    );
}

#[test]
fn an_export_cannot_replace_existing_audio_even_with_a_different_extension() {
    let fixture = Outputs::new();
    for name in ["music.flac", "renamed.json"] {
        let path = fixture.flac(name);
        let before = std::fs::read(&path).unwrap();
        assert!(
            emit(&fixture.args(&path), "export").is_err(),
            "replaced {name}"
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[cfg(unix)]
#[test]
fn export_refuses_live_and_dangling_symbolic_links() {
    use std::os::unix::fs::symlink;
    let fixture = Outputs::new();
    let target = fixture.directory.join("original.txt");
    std::fs::write(&target, "preserve").unwrap();
    let link = fixture.directory.join("result.json");
    symlink(&target, &link).unwrap();
    assert!(emit(&fixture.args(&link), "export").is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "preserve");
    std::fs::remove_file(&target).unwrap();
    assert!(emit(&fixture.args(&link), "export").is_err());
    assert!(!target.exists());
}

#[cfg(unix)]
#[test]
fn an_active_store_reached_through_a_folder_alias_is_protected() {
    use std::os::unix::fs::symlink;
    let fixture = Outputs::new();
    let data = fixture.directory.join("data");
    let alias = fixture.directory.join("alias");
    std::fs::create_dir(&data).unwrap();
    symlink(&data, &alias).unwrap();
    let store = data.join("user.json");
    std::fs::write(&store, "preserve").unwrap();
    let args = Args::parse([
        "notes".into(),
        format!("--data={}", data.display()),
        format!("--output={}", alias.join("user.json").display()),
    ]);
    assert!(emit(&args, "partial export").is_err());
    assert_eq!(std::fs::read_to_string(store).unwrap(), "preserve");
}

#[test]
fn replacing_an_export_preserves_a_hard_linked_store_and_allows_music_sidecars() {
    let fixture = Outputs::new();
    let store = fixture.directory.join("user.json");
    let output = fixture.directory.join("copy.json");
    std::fs::write(&store, "preserve").unwrap();
    std::fs::hard_link(&store, &output).unwrap();
    emit(&fixture.args(&output), "export").unwrap();
    assert_eq!(std::fs::read_to_string(&store).unwrap(), "preserve");
    assert_eq!(std::fs::read_to_string(output).unwrap(), "export");
    let music = fixture.directory.join("music");
    std::fs::create_dir(&music).unwrap();
    let sidecar = music.join("tracks.m3u");
    std::fs::write(&sidecar, "#EXTM3U\nprevious.flac\n").unwrap();
    emit(&fixture.args(&sidecar), "#EXTM3U\nnext.flac\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(sidecar).unwrap(),
        "#EXTM3U\nnext.flac\n"
    );
    let nested = music.join("new/report.json");
    emit(&fixture.args(&nested), "export").unwrap();
    assert_eq!(std::fs::read_to_string(nested).unwrap(), "export");
}

#[test]
fn an_m3u_export_refuses_a_path_that_would_inject_an_entry() {
    let catalog = aede_core::model::build(
        vec![aede_core::model::ScannedFile {
            path: "/music/track.flac\nhttps://unwanted.invalid/track".into(),
            size: 1,
            mtime: 0,
            tags: aede_core::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![],
        0,
        &[],
    );
    assert!(m3u(&catalog, &[0]).is_err());
}

#[test]
fn a_field_is_quoted_only_when_it_has_to_be() {
    assert_eq!(escape("So What", ','), "So What");
    assert_eq!(escape("Freedom, Pt. 2", ','), "\"Freedom, Pt. 2\"");
    // The separator decides: the same title needs nothing under `;`.
    assert_eq!(escape("Freedom, Pt. 2", ';'), "Freedom, Pt. 2");
    // Quotes are doubled, not escaped with a backslash.
    assert_eq!(escape("Say \"Hello\"", ','), "\"Say \"\"Hello\"\"\"");
    assert_eq!(escape("Two\nlines", ','), "\"Two\nlines\"");
}
