//! Search continuations remain complete commands, including long local paths.

#[path = "annotation_command_support/mod.rs"]
#[allow(dead_code)]
mod support;

use aede_core::model::{self, ScannedFile};
use aede_core::{store, tags};
use support::PersonalLibrary;

#[test]
fn search_continuations_keep_long_local_recording_paths_and_distinct_destinations() {
    let library = PersonalLibrary::new();
    let paths: Vec<_> = ["studio", "live"]
        .into_iter()
        .map(|edition| {
            library
                .dir
                .join("long music directory ".repeat(6))
                .join(edition)
                .join("track.flac")
        })
        .collect();
    let files = paths
        .iter()
        .map(|path| {
            let mut tags = tags::RawTags::default();
            tags.insert("title", "Shared Title");
            tags.insert("artist", "Artist");
            ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = model::build(
        files,
        vec![library.dir.to_string_lossy().into_owned()],
        1,
        &[],
    );
    let path = store::catalog_path(&library.dir);
    store::save(&catalog, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let personal_before = library.bytes();
    let result = library.run(&["search", "Shared Title"]);
    assert!(result.status.success(), "{result:?}");
    let output = String::from_utf8(result.stdout).unwrap();
    let commands: Vec<_> = output
        .lines()
        .filter_map(|line| line.find("aede recording ").map(|start| &line[start..]))
        .collect();
    assert_eq!(commands.len(), 2, "{output}");
    for path in &paths {
        let argument = format!("local:{}", path.display());
        let expected = format!("aede recording '{argument}'");
        assert!(commands.contains(&expected.as_str()), "{output}");
        let result = library.run(&["recording", &argument]);
        assert!(result.status.success(), "{result:?}");
        assert!(
            String::from_utf8(result.stdout)
                .unwrap()
                .contains("Shared Title")
        );
    }
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(library.bytes(), personal_before);
}
