//! Missing-lyrics commands preserve the literal title when copied to a shell.

#![cfg(unix)]

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

#[test]
fn missing_lyrics_followup_commands_preserve_literal_track_titles() {
    use aede_core::{model, store, tags::RawTags};
    use std::process::Command;

    for title in [
        "Song's $TEST_WORD Title",
        "The \"Quoted\" Song",
        "Ordinary Song",
    ] {
        let library = support::Library::new("lyrics-navigation", false);
        let original = store::load(&store::catalog_path(&library.data))
            .unwrap()
            .unwrap();
        let mut tags = RawTags {
            fields: original.files[0].tags.clone(),
            properties: original.files[0].properties.clone(),
            ..Default::default()
        };
        tags.fields.remove("title");
        tags.insert("title", title);
        let catalog = model::build(
            vec![model::ScannedFile {
                path: library.audio.to_string_lossy().into_owned(),
                size: 24,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            original.roots,
            1,
            &[],
        );
        store::save(&catalog, &store::catalog_path(&library.data)).unwrap();
        let data_before = filesystem::snapshot(&library.data);
        let music_before = filesystem::snapshot(&library.music);
        let page = library.run(&["track", title, "--lyrics"]);
        assert!(page.status.success(), "{page:?}");
        let page = String::from_utf8(page.stdout).unwrap();
        let followup = page
            .lines()
            .find_map(|line| {
                let start = line.find("aede fetch ")?;
                line[start..].strip_suffix(" asks LRCLIB for them")
            })
            .unwrap_or_else(|| panic!("missing lyrics follow-up: {page}"));
        // Only fixed harmless fixture names enter the shell. Capture argv;
        // never run a fetch or contact the lyrics service.
        let captured = Command::new("/bin/sh")
            .args([
                "-c",
                &format!("aede() {{ printf '%s\\n' \"$@\"; }}\n{followup}"),
            ])
            .env_clear()
            .env("TEST_WORD", "expanded")
            .output()
            .unwrap();
        assert!(captured.status.success(), "{followup}: {captured:?}");
        let arguments = String::from_utf8(captured.stdout).unwrap();
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            ["fetch", title, "--lyrics"],
            "{followup}"
        );
        #[cfg(feature = "fetch")]
        {
            let dry_run = library.run(&["fetch", title, "--lyrics", "--dry-run"]);
            assert!(dry_run.status.success(), "{dry_run:?}");
            let dry_run = String::from_utf8(dry_run.stdout).unwrap();
            assert!(dry_run.contains("1 track without words"), "{dry_run}");
            assert!(dry_run.contains(title), "{dry_run}");
        }
        assert_eq!(filesystem::snapshot(&library.data), data_before);
        assert_eq!(filesystem::snapshot(&library.music), music_before);
    }
}
