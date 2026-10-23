use super::nested_test_support as support;
use super::*;

use support::{assert_cached_file, assert_paths, assert_unavailable, canonical_sandbox};

#[test]
fn unavailable_absent_child_is_preserved_below_the_only_traversal_root() {
    let sandbox = canonical_sandbox();
    let retained = sandbox.track("parent/child/retained.flac");
    let root = sandbox.0.join("parent");
    let child = root.join("child");
    let (previous, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::rename(&child, sandbox.0.join("detached-child")).unwrap();
    let added = sandbox.track("parent/z-added.flac");
    let options = ScanOptions {
        unavailable_roots: vec![child.clone()],
        ..Default::default()
    };
    let (catalog, report) = scan(
        std::slice::from_ref(&root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(&catalog, &[retained.clone(), added]);
    assert_cached_file(&previous, &catalog, &retained);
    assert_eq!(report.preserved, 1);
    assert_eq!(report.removed, 0);
    assert_eq!(report.found, 1);
    assert_eq!(report.read, 1);
    assert_unavailable(&report, &child);
}

#[test]
fn accessible_child_declared_offline_is_not_read_and_its_neighbor_is_scanned() {
    let sandbox = canonical_sandbox();
    let retained = sandbox.track("parent/child/retained.flac");
    let neighbor = sandbox.track("parent/child-neighbor/live.flac");
    let root = sandbox.0.join("parent");
    let child = root.join("child");
    let (previous, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::write(&retained, b"unmounted replacement is not audio").unwrap();
    sandbox.track("parent/child/new.flac");
    let added = sandbox.track("parent/child-neighbor/new.flac");
    let options = ScanOptions {
        force_read: true,
        unavailable_roots: vec![child.clone()],
        ..Default::default()
    };
    let (catalog, report) = scan(&[root], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, &[retained.clone(), neighbor, added]);
    assert_cached_file(&previous, &catalog, &retained);
    assert_eq!(
        report.found, 2,
        "offline descendants never enter the read queue"
    );
    assert_eq!(report.read, 2);
    assert_eq!(report.preserved, 1);
    assert_eq!(report.removed, 0);
    assert_unavailable(&report, &child);
}

#[test]
fn nested_offline_retention_keeps_exclusions_and_normal_remount_semantics() {
    let sandbox = canonical_sandbox();
    let retained = sandbox.track("parent/child/retained.flac");
    let excluded_file = sandbox.track("parent/child/excluded/removed.flac");
    let root = sandbox.0.join("parent");
    let child = root.join("child");
    let excluded = child.join("excluded");
    let (previous, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_file(&retained).unwrap();
    std::fs::remove_file(&excluded_file).unwrap();
    let options = ScanOptions {
        unavailable_roots: vec![child.clone()],
        excluded: vec![excluded.clone()],
        ..Default::default()
    };
    let (offline, report) = scan(
        std::slice::from_ref(&root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(&offline, std::slice::from_ref(&retained));
    assert_eq!(report.preserved, 1);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        vec![excluded_file.as_path()]
    );
    assert_unavailable(&report, &child);

    let returned = sandbox.track("parent/child/returned.flac");
    sandbox.track("parent/child/excluded/still-excluded.flac");
    let options = ScanOptions {
        excluded: vec![excluded],
        ..Default::default()
    };
    let (mounted, report) = scan(&[root], Some(&offline), &options, |_| {}).unwrap();
    assert_paths(&mounted, std::slice::from_ref(&returned));
    assert_eq!(report.preserved, 0);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        vec![retained.as_path()]
    );
    assert!(report.failures.is_empty());
}

#[test]
fn an_excluded_offline_child_is_removed_without_an_unavailable_failure() {
    let sandbox = canonical_sandbox();
    sandbox.track("parent/child/removed.flac");
    let neighbor = sandbox.track("parent/child-neighbor/kept.flac");
    let root = sandbox.0.join("parent");
    let child = root.join("child");
    let (previous, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_dir_all(&child).unwrap();
    let options = ScanOptions {
        unavailable_roots: vec![child.clone()],
        excluded: vec![child],
        ..Default::default()
    };
    let (catalog, report) = scan(&[root], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, &[neighbor]);
    assert_eq!(report.preserved, 0);
    assert_eq!(report.removed, 1);
    assert!(report.failures.is_empty());
}

#[test]
fn offline_declarations_outside_active_roots_do_not_restore_removed_roots() {
    let sandbox = canonical_sandbox();
    let kept = sandbox.track("selected/kept.flac");
    sandbox.track("removed/old.flac");
    let selected = sandbox.0.join("selected");
    let removed = sandbox.0.join("removed");
    let (previous, _) = scan(
        &[selected.clone(), removed.clone()],
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_dir_all(&removed).unwrap();
    let options = ScanOptions {
        unavailable_roots: vec![removed],
        ..Default::default()
    };
    let (catalog, report) = scan(&[selected], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, &[kept]);
    assert_eq!(report.preserved, 0);
    assert_eq!(report.removed, 1);
    assert!(report.failures.is_empty());
}

#[test]
fn unavailable_parent_preserves_only_the_selected_descendant() {
    let sandbox = canonical_sandbox();
    let retained = sandbox.track("parent/selected/retained.flac");
    sandbox.track("parent/sibling/removed.flac");
    let parent = sandbox.0.join("parent");
    let selected = parent.join("selected");
    let (previous, _) = scan(
        std::slice::from_ref(&parent),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_file(&retained).unwrap();
    let options = ScanOptions {
        unavailable_roots: vec![parent],
        ..Default::default()
    };
    let (catalog, report) = scan(&[selected], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, std::slice::from_ref(&retained));
    assert_cached_file(&previous, &catalog, &retained);
    assert_eq!(report.preserved, 1);
    assert_eq!(
        report.removed, 1,
        "the unselected sibling is intentionally removed"
    );
    assert_eq!(report.found, 0);
    assert_eq!(report.read, 0);
    assert_eq!(report.failures.len(), 1);
}

#[test]
fn duplicate_and_overlapping_offline_roots_have_deterministic_bounded_results() {
    let sandbox = canonical_sandbox();
    let retained = sandbox.track("parent/child/retained.flac");
    let root = sandbox.0.join("parent");
    let child = root.join("child");
    let (previous, _) = scan(
        std::slice::from_ref(&root),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    std::fs::remove_file(&retained).unwrap();
    let options = ScanOptions {
        unavailable_roots: vec![child.clone(), root.clone(), child.clone()],
        ..Default::default()
    };
    let (catalog, report) = scan(&[child, root], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, std::slice::from_ref(&retained));
    assert_cached_file(&previous, &catalog, &retained);
    assert_eq!(report.preserved, 1);
    assert_eq!(report.found, 0);
    assert_eq!(report.read, 0);
    assert_eq!(report.removed, 0);
    assert!(
        report.failures.len() <= 2,
        "declarations are not repeated per file"
    );
    assert!(report.failures.windows(2).all(|paths| paths[0] < paths[1]));
}

#[cfg(unix)]
#[test]
fn followed_alias_of_an_offline_child_preserves_the_published_alias_spelling() {
    let sandbox = canonical_sandbox();
    let actual = sandbox.track("parent/z-target/retained.flac");
    let root = sandbox.0.join("parent");
    let target = root.join("z-target");
    let alias = root.join("a-alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    let options = ScanOptions {
        follow_symlinks: true,
        ..Default::default()
    };
    let (previous, _) = scan(std::slice::from_ref(&root), None, &options, |_| {}).unwrap();
    let published = alias.join("retained.flac");
    assert_paths(&previous, std::slice::from_ref(&published));
    std::fs::write(&actual, b"offline target must not be parsed").unwrap();
    sandbox.track("parent/z-target/new.flac");
    let neighbor = sandbox.track("parent/z-neighbor/added.flac");
    let options = ScanOptions {
        unavailable_roots: vec![target.clone()],
        ..options
    };
    let (catalog, report) = scan(&[root], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, &[published.clone(), neighbor]);
    assert_cached_file(&previous, &catalog, &published);
    assert_eq!(report.preserved, 1);
    assert_eq!(report.removed, 0);
    assert_eq!(report.found, 1);
    assert_eq!(report.read, 1);
    let failures: std::collections::BTreeSet<&Path> = report
        .failures
        .iter()
        .map(|(path, _)| Path::new(path))
        .collect();
    assert_eq!(failures, [alias.as_path(), target.as_path()].into());
}
