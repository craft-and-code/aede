use super::*;

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        static NEXT_SANDBOX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT_SANDBOX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aede_scan_{}_{nonce}_{serial}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn track(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac"),
            &path,
        )
        .unwrap();
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[test]
fn special_files_with_audio_extensions_are_not_opened_as_tracks() {
    let sandbox = Sandbox::new();
    std::os::unix::fs::symlink("/dev/null", sandbox.0.join("not-a-track.flac")).unwrap();
    let options = ScanOptions {
        follow_symlinks: true,
        ..Default::default()
    };
    let (catalog, report) = scan(std::slice::from_ref(&sandbox.0), None, &options, |_| {}).unwrap();
    assert_eq!(report.found, 0, "only regular files can be audio tracks");
    assert!(catalog.files.is_empty());
    assert!(report.failures.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_audio_paths_are_reported_without_lossy_catalog_identities() {
    use std::os::unix::ffi::OsStringExt;
    let sandbox = Sandbox::new();
    for byte in [0xfe, 0xff] {
        let path = sandbox.0.join(std::ffi::OsString::from_vec(
            [b"track".as_slice(), &[byte], b".flac"].concat(),
        ));
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac"),
            path,
        )
        .unwrap();
    }
    let (catalog, report) = scan(
        std::slice::from_ref(&sandbox.0),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    assert!(catalog.files.is_empty(), "paths must round-trip exactly");
    assert_eq!(report.found, 0);
    assert_eq!(report.failures.len(), 2);
    assert_ne!(report.failures[0].0, report.failures[1].0);
    assert!(
        report
            .failures
            .iter()
            .all(|(_, reason)| reason.contains("UTF-8"))
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_root_is_refused_without_persisting_a_lossy_path() {
    use std::os::unix::ffi::OsStringExt;
    let sandbox = Sandbox::new();
    let root = sandbox
        .0
        .join(std::ffi::OsString::from_vec(vec![b'a', 0xff]));
    let error = scan(&[root], None, &Default::default(), |_| {}).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("UTF-8"));
}

#[test]
fn a_file_changed_during_tag_read_is_reported_instead_of_cached() {
    let sandbox = Sandbox::new();
    let path = sandbox.track("01.flac");
    let outcome = read_fresh(&path, false, |path| {
        let tags = tags::read(path)?;
        let mut bytes = std::fs::read(path)?;
        bytes.push(0);
        std::fs::write(path, bytes)?;
        Ok(tags)
    });
    let Err(tags::TagError::Io(error)) = outcome else {
        panic!("a changing file must not acquire a reusable metadata snapshot");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
}

#[test]
fn a_concurrent_rewrite_is_not_misreported_as_permanent_audio_corruption() {
    let sandbox = Sandbox::new();
    let path = sandbox.track("01.flac");
    let outcome = read_fresh(&path, false, |path| {
        let mut bytes = std::fs::read(path)?;
        bytes.push(0);
        std::fs::write(path, bytes)?;
        Err(tags::TagError::Malformed("rewrite in progress"))
    });
    assert!(
        matches!(outcome, Err(tags::TagError::Io(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[cfg(unix)]
#[test]
fn a_file_replaced_during_tag_read_is_detected_even_with_equal_size_and_time() {
    let sandbox = Sandbox::new();
    let path = sandbox.track("01.flac");
    let replacement = sandbox.track("replacement.flac");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let outcome = read_fresh(&path, false, |path| {
        let tags = tags::read(path)?;
        std::fs::rename(&replacement, path)?;
        Ok(tags)
    });
    assert!(
        matches!(outcome, Err(tags::TagError::Io(error)) if error.kind() == std::io::ErrorKind::Interrupted)
    );
}

#[cfg(unix)]
#[test]
fn followed_directory_aliases_choose_a_stable_catalog_path() {
    let sandbox = Sandbox::new();
    let target = sandbox.track("z-album/01.flac");
    std::os::unix::fs::symlink(target.parent().unwrap(), sandbox.0.join("a-alias")).unwrap();
    let options = ScanOptions {
        follow_symlinks: true,
        ..Default::default()
    };
    let (catalog, _) = scan(std::slice::from_ref(&sandbox.0), None, &options, |_| {}).unwrap();
    assert_eq!(catalog.files.len(), 1);
    assert_eq!(
        Path::new(&catalog.files[0].path),
        sandbox.0.join("a-alias").join("01.flac")
    );
}

#[cfg(unix)]
#[test]
fn root_order_does_not_change_followed_aliases_or_the_persisted_catalog() {
    let sandbox = Sandbox::new();
    let target = sandbox
        .track("z-album/01.flac")
        .parent()
        .unwrap()
        .to_path_buf();
    let alias = sandbox.0.join("a-alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    let options = ScanOptions {
        follow_symlinks: true,
        ..Default::default()
    };
    let (first, _) = scan(&[target.clone(), alias.clone()], None, &options, |_| {}).unwrap();
    let (mut second, _) = scan(&[alias, target], None, &options, |_| {}).unwrap();
    second.scanned_at = first.scanned_at;
    assert_eq!(
        crate::store::to_json(&first),
        crate::store::to_json(&second)
    );
}

#[test]
fn inaccessible_root_keeps_cached_tracks_and_reports_the_error() {
    let sandbox = Sandbox::new();
    let track = sandbox.track("Album/01.flac");
    let root = track.parent().unwrap().to_path_buf();
    let (first, _) = scan(
        std::slice::from_ref(&root),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::write(&root, b"this is no longer a readable directory").unwrap();
    let (second, report) = scan(
        std::slice::from_ref(&root),
        Some(&first),
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(second.files.len(), 1, "inaccessible is not deleted");
    assert_eq!(second.files[0].path, first.files[0].path);
    assert_eq!(report.removed, 0);
    assert!(
        report
            .failures
            .iter()
            .any(|(path, _)| Path::new(path) == root)
    );
}

#[test]
fn same_second_same_size_changes_are_read_again() {
    let sandbox = Sandbox::new();
    let path = sandbox.track("01.flac");
    let second = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let set_time = |fraction| {
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(second + std::time::Duration::from_nanos(fraction)),
            )
            .unwrap();
    };
    set_time(100_000_000);
    let (first, _) = scan(
        std::slice::from_ref(&sandbox.0),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    set_time(800_000_000);
    let (second, report) = scan(
        std::slice::from_ref(&sandbox.0),
        Some(&first),
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        report.read, 1,
        "fractional timestamps are part of file identity"
    );
    assert_eq!(report.reused, 0);
    assert_eq!(second.files[0].mtime, first.files[0].mtime);
}

#[test]
fn cover_art_rank() {
    assert_eq!(cover_rank("cover.jpg"), Some(0));
    assert_eq!(cover_rank("folder.png"), Some(1));
    assert_eq!(cover_rank("scan-back.jpg"), Some(COVER_NAMES.len()));
    assert_eq!(cover_rank("notes.txt"), None);
    assert_eq!(cover_rank("no_extension"), None);
}

#[test]
fn scan_of_a_real_folder() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let (catalog, report) = scan(&[root], None, &ScanOptions::default(), |_| {}).expect("scan");

    assert!(report.found >= 8, "files found: {}", report.found);
    assert_eq!(report.read, report.found);
    assert_eq!(report.reused, 0);
    assert!(
        report.failures.is_empty(),
        "failures: {:?}",
        report.failures
    );
    assert_eq!(catalog.files.len(), report.found);
    assert!(catalog.find_artist("Miles Davis").is_some());
}

#[test]
fn second_scan_reuses_everything() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let (first, _) = scan(
        std::slice::from_ref(&root),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let (mut second, report) =
        scan(&[root], Some(&first), &ScanOptions::default(), |_| {}).unwrap();

    assert_eq!(report.read, 0, "no file should have been read again");
    assert_eq!(report.reused, report.found);
    // And the rebuilt catalog must be identical.
    second.scanned_at = first.scanned_at;
    assert_eq!(
        crate::store::to_json(&first),
        crate::store::to_json(&second)
    );
    assert_eq!(
        crate::conclusions::to_json(&crate::conclusions::Conclusions::from_catalog(&first)),
        crate::conclusions::to_json(&crate::conclusions::Conclusions::from_catalog(&second))
    );
}

#[test]
fn missing_files_in_accessible_folders_are_removed() {
    let sandbox = Sandbox::new();
    let track = sandbox.track("Album/01.flac");
    let (first, _) = scan(
        std::slice::from_ref(&sandbox.0),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_file(&track).unwrap();
    let (second, report) = scan(
        std::slice::from_ref(&sandbox.0),
        Some(&first),
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert!(second.files.is_empty());
    assert_eq!(report.removed, 1);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        [track.as_path()]
    );
    assert!(report.failures.is_empty());
}

#[test]
fn legacy_second_precision_is_refreshed_once_then_reused_after_reload() {
    let sandbox = Sandbox::new();
    sandbox.track("01.flac");
    let (mut legacy, _) = scan(
        std::slice::from_ref(&sandbox.0),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    legacy.file_mtime_subseconds.clear();
    let legacy = crate::store::from_json(&crate::store::to_json(&legacy)).unwrap();
    let (fresh, first_report) = scan(
        std::slice::from_ref(&sandbox.0),
        Some(&legacy),
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(first_report.read, 1);
    let stored = crate::store::from_json(&crate::store::to_json(&fresh)).unwrap();
    let (_, second_report) = scan(
        std::slice::from_ref(&sandbox.0),
        Some(&stored),
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    assert_eq!(second_report.read, 0);
    assert_eq!(second_report.reused, 1);
}

#[test]
fn full_read_still_preserves_inaccessible_paths() {
    let sandbox = Sandbox::new();
    let track = sandbox.track("Album/01.flac");
    let root = track.parent().unwrap().to_path_buf();
    let (first, _) = scan(
        std::slice::from_ref(&root),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::write(&root, b"not a directory").unwrap();
    let options = ScanOptions {
        force_read: true,
        ..Default::default()
    };
    let (second, report) = scan(&[root], Some(&first), &options, |_| {}).unwrap();
    assert_eq!(second.files.len(), 1);
    assert_eq!(report.preserved, 1);
    assert_eq!(report.removed, 0);
}

#[test]
fn explicit_exclusions_still_remove_entries_below_an_inaccessible_root() {
    let sandbox = Sandbox::new();
    let track = sandbox.track("Album/01.flac");
    let root = track.parent().unwrap().to_path_buf();
    let (first, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::write(&root, b"not a directory").unwrap();
    let options = ScanOptions {
        excluded: vec![root.clone()],
        ..Default::default()
    };
    let (second, report) = scan(&[root], Some(&first), &options, |_| {}).unwrap();
    assert!(second.files.is_empty());
    assert_eq!(report.preserved, 0);
    assert_eq!(report.removed, 1);
}

#[test]
fn cover_choice_is_deterministic_when_preference_ranks_tie() {
    let sandbox = Sandbox::new();
    sandbox.track("01.flac");
    std::fs::write(sandbox.0.join("cover-z.png"), b"later path").unwrap();
    std::fs::write(sandbox.0.join("cover-a.jpg"), b"earlier path").unwrap();
    let expected = sandbox.0.join("cover-a.jpg");
    assert_eq!(cover_in(&sandbox.0), Some(expected.clone()));
    let (catalog, _) = scan(
        std::slice::from_ref(&sandbox.0),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    let actual = catalog.releases[0].cover_path.as_deref().map(Path::new);
    assert_eq!(actual, Some(expected.as_path()));
}
