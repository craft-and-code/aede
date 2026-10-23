#![cfg(unix)]

use super::nested_test_support as existing_support;
use super::test_support::Sandbox;
use super::*;

use existing_support::{assert_cached_file, assert_paths, assert_unavailable, canonical_sandbox};

#[path = "scan_alias_exclusion_test_support.rs"]
mod support;
use support::AliasFixture;

#[test]
fn excluded_alias_descendants_are_removed_while_an_absent_target_is_retained_and_remounted() {
    let fixture = AliasFixture::new();
    let previous = fixture.seed(true);
    fixture.detach();
    fixture.add_neighbor();
    let options = ScanOptions {
        follow_symlinks: true,
        force_read: true,
        threads: 1,
        unavailable_roots: vec![fixture.target.clone()],
        excluded: vec![fixture.target.join("excluded")],
        ..Default::default()
    };
    let (offline, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(&offline, &fixture.expected_partial(&fixture.alias));
    for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
        assert_cached_file(&previous, &offline, &fixture.alias.join(relative));
    }
    assert_eq!(report.preserved, 2);
    assert_eq!(report.read, 2);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        vec![fixture.alias.join("excluded/removed.flac").as_path()]
    );

    fixture.remount();
    std::fs::remove_file(fixture.target.join(r"kept\native.flac")).unwrap();
    let returned = fixture.sandbox.track("parent/z-target/returned.flac");
    fixture
        .sandbox
        .track("parent/z-target/excluded/still-excluded.flac");
    let mounted_options = ScanOptions {
        unavailable_roots: Vec::new(),
        ..options
    };
    let (mounted, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&offline),
        &mounted_options,
        |_| {},
    )
    .unwrap();
    assert_paths(
        &mounted,
        &[
            fixture.alias.join(returned.file_name().unwrap()),
            fixture.alias.join("excluded-neighbor/kept.flac"),
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ],
    );
    assert_eq!(report.preserved, 0);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        vec![fixture.alias.join(r"kept\native.flac").as_path()]
    );
    assert!(report.failures.is_empty());
}

#[test]
fn canonical_exclusion_overrides_accessible_offline_alias_retention_even_with_full_reads() {
    let fixture = AliasFixture::new();
    let previous = fixture.seed(true);
    std::fs::write(
        fixture.target.join(r"kept\native.flac"),
        b"offline replacement must not be read",
    )
    .unwrap();
    fixture.sandbox.track("parent/z-target/new.flac");
    fixture.add_neighbor();
    let options = ScanOptions {
        follow_symlinks: true,
        force_read: true,
        threads: 1,
        unavailable_roots: vec![fixture.target.clone()],
        excluded: vec![fixture.target.join("excluded")],
        ..Default::default()
    };
    let (catalog, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(&catalog, &fixture.expected_partial(&fixture.alias));
    for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
        assert_cached_file(&previous, &catalog, &fixture.alias.join(relative));
    }
    assert_eq!(report.preserved, 2);
    assert_eq!(report.found, 2);
    assert_eq!(report.read, 2);
    assert_eq!(report.removed, 1);
    assert_eq!(
        report
            .removed_paths
            .iter()
            .map(Path::new)
            .collect::<Vec<_>>(),
        vec![fixture.alias.join("excluded/removed.flac").as_path()]
    );
}

#[test]
fn a_broken_followed_alias_keeps_canonical_exclusions_without_an_offline_declaration() {
    let fixture = AliasFixture::new();
    let previous = fixture.seed(true);
    fixture.detach();
    fixture.add_neighbor();
    let options = ScanOptions {
        follow_symlinks: true,
        force_read: true,
        threads: 1,
        excluded: vec![fixture.target.join("excluded")],
        ..Default::default()
    };
    let (catalog, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(&catalog, &fixture.expected_partial(&fixture.alias));
    assert_cached_file(
        &previous,
        &catalog,
        &fixture.alias.join(r"kept\native.flac"),
    );
    assert_eq!(report.preserved, 2);
    assert_eq!(report.removed, 1);
    assert_eq!(report.read, 2);
}

#[test]
fn excluding_an_entire_absent_target_also_removes_its_previously_published_alias_paths() {
    let fixture = AliasFixture::new();
    let previous = fixture.seed(true);
    fixture.detach();
    fixture.add_neighbor();
    let options = ScanOptions {
        follow_symlinks: true,
        unavailable_roots: vec![fixture.target.clone()],
        excluded: vec![fixture.target.clone()],
        threads: 1,
        ..Default::default()
    };
    let (catalog, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(
        &catalog,
        &[
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ],
    );
    assert_eq!(report.preserved, 0);
    assert_eq!(report.removed, 3);
}

#[test]
fn excluding_an_entire_accessible_offline_target_removes_all_cached_alias_paths() {
    let fixture = AliasFixture::new();
    let previous = fixture.seed(true);
    fixture.add_neighbor();
    let options = ScanOptions {
        follow_symlinks: true,
        force_read: true,
        unavailable_roots: vec![fixture.target.clone()],
        excluded: vec![fixture.target.clone()],
        threads: 1,
        ..Default::default()
    };
    let (catalog, report) = scan(
        std::slice::from_ref(&fixture.root),
        Some(&previous),
        &options,
        |_| {},
    )
    .unwrap();
    assert_paths(
        &catalog,
        &[
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ],
    );
    assert_eq!(report.preserved, 0);
    assert_eq!(report.removed, 3);
    assert!(report.failures.is_empty());
}

#[test]
fn an_unfollowed_alias_does_not_change_canonical_offline_exclusions_or_folder_boundaries() {
    for absent in [false, true] {
        let fixture = AliasFixture::new();
        let previous = fixture.seed(false);
        if absent {
            fixture.detach();
        } else {
            std::fs::write(fixture.target.join(r"kept\native.flac"), b"offline").unwrap();
        }
        fixture.add_neighbor();
        let options = ScanOptions {
            force_read: true,
            threads: 1,
            unavailable_roots: vec![fixture.target.clone()],
            excluded: vec![fixture.target.join("excluded")],
            ..Default::default()
        };
        let (catalog, report) = scan(
            std::slice::from_ref(&fixture.root),
            Some(&previous),
            &options,
            |_| {},
        )
        .unwrap();
        assert_paths(&catalog, &fixture.expected_partial(&fixture.target));
        for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
            assert_cached_file(&previous, &catalog, &fixture.target.join(relative));
        }
        assert_eq!(report.preserved, 2);
        assert_eq!(report.removed, 1);
        assert_eq!(report.read, 2);
        assert_unavailable(&report, &fixture.target);
    }
}

#[test]
fn surviving_alias_chains_and_existing_link_parent_components_keep_canonical_exclusions() {
    for through_existing_parent in [false, true] {
        let fixture = AliasFixture::new();
        std::fs::remove_file(&fixture.alias).unwrap();
        let hop = fixture.root.join("y-hop");
        if through_existing_parent {
            std::fs::create_dir(fixture.root.join("pivot")).unwrap();
            std::os::unix::fs::symlink("pivot", &hop).unwrap();
            std::os::unix::fs::symlink(
                PathBuf::from("y-hop").join("..").join("z-target"),
                &fixture.alias,
            )
            .unwrap();
        } else {
            std::os::unix::fs::symlink("z-target", &hop).unwrap();
            std::os::unix::fs::symlink("y-hop", &fixture.alias).unwrap();
        }
        let previous = fixture.seed(true);
        fixture.detach();
        fixture.add_neighbor();
        let options = ScanOptions {
            follow_symlinks: true,
            force_read: true,
            threads: 1,
            unavailable_roots: vec![fixture.target.clone()],
            excluded: vec![fixture.target.join("excluded")],
            ..Default::default()
        };
        let (catalog, report) = scan(
            std::slice::from_ref(&fixture.root),
            Some(&previous),
            &options,
            |_| {},
        )
        .unwrap();
        assert_paths(&catalog, &fixture.expected_partial(&fixture.alias));
        for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
            assert_cached_file(&previous, &catalog, &fixture.alias.join(relative));
        }
        assert_eq!(report.preserved, 2);
        assert_eq!(report.removed, 1);
        assert_eq!(report.read, 2);
        assert_eq!(
            report
                .removed_paths
                .iter()
                .map(Path::new)
                .collect::<Vec<_>>(),
            vec![fixture.alias.join("excluded/removed.flac").as_path()]
        );
    }
}
