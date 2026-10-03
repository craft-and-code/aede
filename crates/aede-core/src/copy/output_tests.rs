use super::*;

#[test]
fn new_publication_preflight_leaves_existing_files_and_no_probe_behind() {
    let directory = std::env::temp_dir().join(format!(
        "aede_copy_publication_probe_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("user-file"), b"keep me").unwrap();
    validate_new_publication(&directory).unwrap();
    assert_eq!(
        std::fs::read(directory.join("user-file")).unwrap(),
        b"keep me"
    );
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn bare_relative_destinations_publish_in_the_current_folder() {
    let target = PathBuf::from(format!(
        "aede-bare-relative-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = TemporaryOutput::new(&target).unwrap();
    std::fs::write(temporary.path(), b"complete").unwrap();
    temporary.publish().unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"complete");
    std::fs::remove_file(target).unwrap();
}

#[test]
fn an_existing_different_size_destination_requires_explicit_replacement() {
    let dir = std::env::temp_dir().join(format!("aede_copy_size_mismatch_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("source.flac");
    let target = dir.join("target.flac");
    std::fs::write(&source, b"source").unwrap();
    for bytes in [b"unrelated existing output".as_slice(), b""] {
        std::fs::write(&target, bytes).unwrap();
        let result = super::super::copy_one(&source, &target, 6, false, false);
        assert!(
            result.is_err(),
            "existing output must be retained: {result:?}"
        );
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
    }
    assert_eq!(
        super::super::copy_one(&source, &target, 6, true, true).unwrap(),
        super::super::Wrote::Copied
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn publishing_a_new_output_preserves_a_target_that_appeared_during_the_copy() {
    let dir = std::env::temp_dir().join(format!("aede_copy_publish_race_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let target = dir.join("output.flac");
    let temporary = TemporaryOutput::new(&target).unwrap();
    std::fs::write(temporary.path(), b"new copy").unwrap();
    // Reproduce the old check/rename window without process scheduling.
    std::fs::write(&target, b"another writer's complete output").unwrap();
    assert!(temporary.publish_new().is_err());
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"another writer's complete output"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_linked_subfolder_cannot_redirect_an_output() {
    let dir = std::env::temp_dir().join(format!("aede_copy_parent_link_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let base = dir.join("out");
    let music = dir.join("music");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&music).unwrap();
    std::os::unix::fs::symlink(&music, base.join("Album")).unwrap();
    assert!(validate_destination(&base, Path::new("Album/01.flac")).is_err());
    assert!(validate_destination(&base, Path::new("../music/01.flac")).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn failed_or_abandoned_output_is_removed_without_touching_the_target() {
    let dir = std::env::temp_dir().join(format!("aede_copy_abandoned_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("output.mp3");
    std::fs::write(&target, b"original").unwrap();
    let first = TemporaryOutput::new(&target).unwrap();
    let second = TemporaryOutput::new(&target).unwrap();
    assert_ne!(first.path(), second.path());
    assert_eq!(first.path().extension().unwrap(), "mp3");
    std::fs::write(first.path(), b"partial").unwrap();
    let path = first.path().to_path_buf();
    drop(first);
    drop(second);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn existing_content_verification_does_not_accept_same_size_different_bytes() {
    let dir = std::env::temp_dir().join(format!("aede_copy_existing_check_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.flac");
    let target = dir.join("target.flac");
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&target, b"wrong!").unwrap();
    assert!(super::super::copy_one_checked(&source, &target, 6, false, false, true).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"wrong!");
    assert_eq!(
        super::super::copy_one_checked(&source, &target, 6, true, true, true).unwrap(),
        super::super::Wrote::Copied
    );
    assert_eq!(
        super::super::copy_one_checked(&source, &target, 6, false, false, true).unwrap(),
        super::super::Wrote::Skipped
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn streaming_comparison_checks_bytes_beyond_the_first_buffer() {
    let dir = std::env::temp_dir().join(format!("aede_copy_streaming_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let left = dir.join("left");
    let right = dir.join("right");
    let mut bytes = vec![42; (1 << 20) + 3];
    std::fs::write(&left, &bytes).unwrap();
    std::fs::write(&right, &bytes).unwrap();
    assert!(files_match(&left, &right).unwrap());
    *bytes.last_mut().unwrap() = 43;
    std::fs::write(&right, &bytes).unwrap();
    assert!(!files_match(&left, &right).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn duplicate_planned_paths_are_refused_without_touching_existing_outputs() {
    let dir = std::env::temp_dir().join(format!("aede_copy_plan_alias_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("same.flac"), b"keep me").unwrap();
    let item = super::super::Item {
        source: PathBuf::from("source"),
        relative: PathBuf::from("same.flac"),
        size: 7,
        kind: super::super::ItemKind::Audio,
        convert: None,
        contents: None,
    };
    let plan = super::super::Plan {
        items: vec![item.clone(), item],
        ..Default::default()
    };
    assert!(validate_filesystem_names(&dir, &plan).is_err());
    assert_eq!(std::fs::read(dir.join("same.flac")).unwrap(), b"keep me");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn filesystem_preflight_refuses_unicode_aliases_outside_the_portable_vocabulary() {
    let dir =
        std::env::temp_dir().join(format!("aede_copy_unknown_unicode_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let composed = "Άλμπουμ";
    let decomposed = "Α\u{301}λμπουμ";
    std::fs::create_dir(dir.join(composed)).unwrap();
    let aliases = dir.join(decomposed).exists();
    std::fs::remove_dir(dir.join(composed)).unwrap();
    let item = |folder: &str| super::super::Item {
        source: PathBuf::from("source"),
        relative: Path::new(folder).join("01.flac"),
        size: 7,
        kind: super::super::ItemKind::Audio,
        convert: None,
        contents: None,
    };
    let plan = super::super::Plan {
        items: vec![item(composed), item(decomposed)],
        ..Default::default()
    };
    let outcome = validate_filesystem_names(&dir, &plan);
    if aliases {
        assert!(
            outcome.is_err(),
            "an actual filesystem alias must be refused"
        );
    } else {
        assert!(outcome.is_ok());
    }
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(dir).unwrap();
}
