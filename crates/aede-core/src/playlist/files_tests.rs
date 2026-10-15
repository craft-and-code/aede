use super::test_support::Fixture;
use super::*;

#[test]
fn discovery_includes_empty_audio_folders_and_deduplicates_nested_roots_without_content_reads() {
    let fixture = Fixture::new();
    let folder = fixture.0.join("No audio");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("b.M3U8"), [0xff, 0x00]).unwrap();
    fs::write(fixture.0.join("a.m3u"), b"01.flac\n").unwrap();
    fs::write(fixture.0.join("other.txt"), b"not a playlist").unwrap();
    let roots = vec![
        folder.to_str().unwrap().into(),
        fixture.0.to_str().unwrap().into(),
        fixture.0.to_str().unwrap().into(),
    ];
    let found = discover(&roots, &[]).unwrap();
    assert_eq!(found.files.len(), 2);
    assert!(
        found
            .files
            .windows(2)
            .all(|files| files[0].path < files[1].path)
    );
    let invalid = found
        .files
        .iter()
        .find(|file| file.filename == "b.M3U8")
        .unwrap();
    assert!(matches!(
        read(&roots, &[], &invalid.id),
        Err(Error::InvalidText)
    ));
    assert_eq!(fs::read(folder.join("b.M3U8")).unwrap(), [0xff, 0x00]);
}

#[test]
fn exclusions_and_missing_roots_are_explicit_and_cannot_be_bypassed_by_old_ids() {
    let fixture = Fixture::new();
    let folder = fixture.0.join("Excluded");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("private.m3u"), b"01.flac\n").unwrap();
    let id = discover(&fixture.roots(), &[]).unwrap().files[0].id.clone();
    let excluded = vec![folder.to_str().unwrap().into()];
    assert!(
        discover(&fixture.roots(), &excluded)
            .unwrap()
            .files
            .is_empty()
    );
    assert!(matches!(
        read(&fixture.roots(), &excluded, &id),
        Err(Error::NotFound)
    ));
    let found = discover(&[fixture.0.join("missing").to_str().unwrap().into()], &[]).unwrap();
    assert_eq!(found.unavailable_roots, 1);
    assert!(found.files.is_empty());
    assert!(matches!(
        read(&fixture.roots(), &[], "../private.m3u"),
        Err(Error::InvalidIdentity)
    ));
}

#[cfg(unix)]
#[test]
fn root_aliases_are_admitted_but_child_symlink_files_and_folders_are_never_followed() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let external = Fixture::new();
    fs::write(fixture.0.join("local.m3u"), b"01.flac\n").unwrap();
    fs::write(external.0.join("outside.m3u"), b"02.flac\n").unwrap();
    symlink(external.0.join("outside.m3u"), fixture.0.join("file.m3u")).unwrap();
    symlink(&external.0, fixture.0.join("directory")).unwrap();
    let alias = external.0.join("root-alias");
    symlink(&fixture.0, &alias).unwrap();
    let found = discover(
        &[alias.to_str().unwrap().into(), fixture.roots()[0].clone()],
        &[],
    )
    .unwrap();
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].filename, "local.m3u");
    assert_eq!(
        read(&fixture.roots(), &[], &found.files[0].id)
            .unwrap()
            .content,
        "01.flac\n"
    );
}

#[test]
fn capacity_depth_and_deadline_refuse_partial_success() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.m3u"), b"a\n").unwrap();
    fs::write(fixture.0.join("b.m3u"), b"b\n").unwrap();
    assert!(matches!(
        discover_with_limits(&fixture.roots(), &[], Limits { files: 1, ..LIMITS }),
        Err(Error::TooLarge)
    ));
    assert!(matches!(
        discover_with_limits(
            &fixture.roots(),
            &[],
            Limits {
                entries: 1,
                ..LIMITS
            }
        ),
        Err(Error::TooLarge)
    ));
    fs::create_dir(fixture.0.join("folder")).unwrap();
    assert!(matches!(
        discover_with_limits(&fixture.roots(), &[], Limits { depth: 0, ..LIMITS }),
        Err(Error::TooLarge)
    ));
    assert!(matches!(
        discover_with_limits(
            &fixture.roots(),
            &[],
            Limits {
                elapsed: Duration::ZERO,
                ..LIMITS
            }
        ),
        Err(Error::Timeout)
    ));
}

#[cfg(unix)]
#[test]
fn a_watched_readable_child_remains_discoverable_under_an_unreadable_watched_parent() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let parent = fixture.0.join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).unwrap();
    fs::write(child.join("selection.m3u"), b"01.flac\n").unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o111)).unwrap();
    // A privileged Unix runner bypasses this permission scenario.
    if fs::read_dir(&parent).is_ok() {
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let child_only = discover(&[child.to_str().unwrap().into()], &[]);
    let combined = discover(
        &[
            parent.to_str().unwrap().into(),
            child.to_str().unwrap().into(),
        ],
        &[],
    );
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(child_only.unwrap().files.len(), 1);
    let combined = combined.unwrap();
    assert_eq!(combined.files.len(), 1);
    assert_eq!(combined.unreadable_directories, 1);
}

#[cfg(unix)]
#[test]
fn revisiting_a_child_of_a_partially_readable_root_does_not_consume_another_file_slot() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let child = fixture.0.join("Readable");
    let blocked = fixture.0.join("Blocked");
    fs::create_dir(&child).unwrap();
    fs::create_dir(&blocked).unwrap();
    fs::write(child.join("selection.m3u"), b"01.flac\n").unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&blocked).is_ok() {
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let found = discover_with_limits(
        &[fixture.roots()[0].clone(), child.to_str().unwrap().into()],
        &[],
        Limits { files: 1, ..LIMITS },
    );
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();
    let found = found.unwrap();
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.unreadable_directories, 1);
}
