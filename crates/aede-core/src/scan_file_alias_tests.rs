#![cfg(unix)]

use super::nested_test_support::{assert_cached_file, assert_paths, canonical_sandbox};
use super::test_support::Sandbox;
use super::*;

#[path = "scan_file_alias_test_support.rs"]
mod support;
use support::{FileAliasFixture, assert_removed, file};

#[test]
fn full_scan_prunes_canonical_excluded_regular_file_aliases_while_absent_and_after_remount() {
    for declared_offline in [false, true] {
        let fixture = FileAliasFixture::new();
        let previous = fixture.seed(true);
        fixture.detach();
        fixture.add_neighbor();
        let options = fixture.options(declared_offline, vec![fixture.target.join("skip")]);
        let (offline, report) = fixture.scan(&previous, &options);
        let mut expected = fixture.expected(true, true, false);
        let mut removed = vec![fixture.removed.clone(), fixture.removed_alias.clone()];
        if declared_offline {
            fixture.assert_cached(&previous, &offline, true);
        } else {
            // The absent child was not separately watched or declared offline.
            // Only its surviving, unreadable file aliases establish retention.
            expected.retain(|path| path != &fixture.kept);
            removed.push(fixture.kept.clone());
            assert_cached_file(&previous, &offline, &fixture.kept_alias);
        }
        assert_paths(&offline, &expected);
        assert_eq!(report.preserved, if declared_offline { 2 } else { 1 });
        assert_eq!(report.read, 2);
        assert_removed(&report, &removed);

        fixture.remount();
        fixture.replace_kept();
        let returned = fixture.sandbox.track("parent/z-target/returned.flac");
        fixture.sandbox.track("parent/z-target/skip/new.flac");
        let options = ScanOptions {
            unavailable_roots: Vec::new(),
            ..options
        };
        let (mounted, report) = fixture.scan(&offline, &options);
        let mut expected = fixture.expected(true, true, false);
        expected.push(returned);
        assert_paths(&mounted, &expected);
        for path in [&fixture.kept, &fixture.kept_alias] {
            assert_ne!(file(&mounted, path).tags, file(&previous, path).tags);
        }
        assert_eq!(report.preserved, 0);
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_removed(&report, &[]);
    }
}

#[test]
fn whole_target_exclusion_prunes_regular_file_aliases_in_mounted_and_offline_scans() {
    for absent in [false, true] {
        for declared_offline in [false, true] {
            let fixture = FileAliasFixture::new();
            let previous = fixture.seed(true);
            if absent {
                fixture.detach();
            }
            fixture.add_neighbor();
            let options = fixture.options(declared_offline, vec![fixture.target.clone()]);
            let (catalog, report) = fixture.scan(&previous, &options);
            assert_paths(&catalog, &fixture.expected(true, false, true));
            assert_eq!(report.preserved, 0);
            assert_eq!(report.read, 2);
            assert_removed(
                &report,
                &[
                    fixture.kept.clone(),
                    fixture.kept_alias.clone(),
                    fixture.removed.clone(),
                    fixture.removed_alias.clone(),
                ],
            );
        }
    }
}

#[test]
fn full_scan_keeps_cached_regular_file_alias_tags_despite_valid_offline_replacements() {
    let fixture = FileAliasFixture::new();
    let previous = fixture.seed(true);
    fixture.replace_kept();
    let newcomer = fixture.sandbox.track("parent/z-target/new.flac");
    fixture.add_neighbor();
    let options = fixture.options(true, Vec::new());
    let (offline, report) = fixture.scan(&previous, &options);
    assert_paths(&offline, &fixture.expected(true, false, false));
    fixture.assert_cached(&previous, &offline, false);
    assert_eq!(report.preserved, 4);
    assert_eq!(report.found, 2);
    assert_eq!(report.read, 2);
    assert_removed(&report, &[]);

    let options = ScanOptions {
        unavailable_roots: Vec::new(),
        ..options
    };
    let (mounted, report) = fixture.scan(&offline, &options);
    let mut expected = fixture.expected(true, false, false);
    expected.push(newcomer);
    assert_paths(&mounted, &expected);
    for path in [&fixture.kept, &fixture.kept_alias] {
        assert_ne!(file(&mounted, path).tags, file(&previous, path).tags);
    }
    assert_eq!(
        file(&mounted, &fixture.kept).tags,
        file(&mounted, &fixture.kept_alias).tags
    );
    assert_eq!(report.preserved, 0);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

#[test]
fn full_scan_keeps_cached_regular_file_aliases_despite_invalid_accessible_offline_bytes() {
    let fixture = FileAliasFixture::new();
    let previous = fixture.seed(true);
    for path in [&fixture.kept, &fixture.removed] {
        std::fs::write(path, b"declared offline bytes must not be parsed").unwrap();
    }
    fixture.add_neighbor();
    let (catalog, report) = fixture.scan(&previous, &fixture.options(true, Vec::new()));
    assert_paths(&catalog, &fixture.expected(true, false, false));
    fixture.assert_cached(&previous, &catalog, false);
    assert_eq!(report.preserved, 4);
    assert_eq!(report.found, 2);
    assert_eq!(report.read, 2);
    assert_removed(&report, &[]);
}

#[test]
fn ordinary_and_declared_absent_targets_keep_nonexcluded_regular_file_aliases_cached() {
    for declared_offline in [false, true] {
        let fixture = FileAliasFixture::new();
        let previous = fixture.seed(true);
        fixture.detach();
        fixture.add_neighbor();
        let (catalog, report) =
            fixture.scan(&previous, &fixture.options(declared_offline, Vec::new()));
        let mut expected = fixture.expected(true, false, false);
        let removed = if declared_offline {
            fixture.assert_cached(&previous, &catalog, false);
            Vec::new()
        } else {
            expected.retain(|path| path != &fixture.kept && path != &fixture.removed);
            assert_cached_file(&previous, &catalog, &fixture.kept_alias);
            assert_cached_file(&previous, &catalog, &fixture.removed_alias);
            vec![fixture.kept.clone(), fixture.removed.clone()]
        };
        assert_paths(&catalog, &expected);
        assert_eq!(report.preserved, if declared_offline { 4 } else { 2 });
        assert_eq!(report.found, 2);
        assert_eq!(report.read, 2);
        assert_removed(&report, &removed);
    }
}

#[test]
fn scans_without_following_regular_file_aliases_keep_canonical_offline_exclusions() {
    for absent in [false, true] {
        for partial in [false, true] {
            let fixture = FileAliasFixture::new();
            let previous = fixture.seed(false);
            if absent {
                fixture.detach();
            } else {
                std::fs::write(&fixture.kept, b"offline").unwrap();
            }
            fixture.add_neighbor();
            let options = ScanOptions {
                follow_symlinks: false,
                ..fixture.options(
                    true,
                    if partial {
                        vec![fixture.target.join("skip")]
                    } else {
                        Vec::new()
                    },
                )
            };
            let (catalog, report) = fixture.scan(&previous, &options);
            assert_paths(&catalog, &fixture.expected(false, partial, false));
            assert_cached_file(&previous, &catalog, &fixture.kept);
            assert_eq!(report.preserved, if partial { 1 } else { 2 });
            assert_eq!(report.read, 2);
            assert_removed(
                &report,
                &if partial {
                    vec![fixture.removed.clone()]
                } else {
                    Vec::new()
                },
            );
        }
    }
}

#[test]
fn surviving_regular_file_alias_chains_resolve_canonical_exclusions_for_absent_targets() {
    for through_existing_parent in [false, true] {
        let fixture = FileAliasFixture::new();
        std::fs::remove_file(&fixture.removed_alias).unwrap();
        let hop = fixture.root.join("y-hop");
        if through_existing_parent {
            std::fs::create_dir(fixture.root.join("pivot")).unwrap();
            std::os::unix::fs::symlink("pivot", &hop).unwrap();
            std::os::unix::fs::symlink(
                "y-hop/../z-target/skip/removed.flac",
                &fixture.removed_alias,
            )
            .unwrap();
        } else {
            std::os::unix::fs::symlink("z-target/skip/removed.flac", &hop).unwrap();
            std::os::unix::fs::symlink("y-hop", &fixture.removed_alias).unwrap();
        }
        let previous = fixture.seed(true);
        fixture.detach();
        fixture.add_neighbor();
        let (catalog, report) = fixture.scan(
            &previous,
            &fixture.options(true, vec![fixture.target.join("skip")]),
        );
        assert_paths(&catalog, &fixture.expected(true, true, false));
        fixture.assert_cached(&previous, &catalog, true);
        assert_eq!(report.preserved, 2);
        assert_removed(
            &report,
            &[fixture.removed.clone(), fixture.removed_alias.clone()],
        );
    }
}

#[test]
fn broken_cyclic_and_overlong_file_alias_chains_retain_cache_without_inventing_identity() {
    for kind in ["broken", "cycle", "overlong"] {
        let fixture = FileAliasFixture::new();
        let previous = fixture.seed(true);
        std::fs::remove_file(&fixture.removed_alias).unwrap();
        std::os::unix::fs::symlink("hop-00", &fixture.removed_alias).unwrap();
        match kind {
            "broken" => {}
            "cycle" => {
                std::os::unix::fs::symlink("b-removed.flac", fixture.root.join("hop-00")).unwrap();
            }
            _ => {
                for index in 0..64 {
                    let next = if index == 63 {
                        "z-target/skip/removed.flac".into()
                    } else {
                        format!("hop-{:02}", index + 1)
                    };
                    std::os::unix::fs::symlink(next, fixture.root.join(format!("hop-{index:02}")))
                        .unwrap();
                }
            }
        }
        fixture.add_neighbor();
        let (catalog, report) = fixture.scan(&previous, &fixture.options(true, Vec::new()));
        assert_paths(&catalog, &fixture.expected(true, false, false));
        fixture.assert_cached(&previous, &catalog, false);
        assert_eq!(report.preserved, 4, "{kind}");
        assert_eq!(report.read, 2, "{kind}");
        assert_removed(&report, &[]);
        assert!(
            report
                .failures
                .iter()
                .any(|(path, _)| Path::new(path) == fixture.removed_alias)
        );
    }
}

#[test]
fn excluded_canonical_parent_still_removes_a_final_file_link_to_a_nonexcluded_target() {
    let sandbox = canonical_sandbox();
    sandbox.track("parent/z-target/keep.flac");
    sandbox.track("parent/z-target/skip/ordinary.flac");
    let neighbor = sandbox.track("parent/z-neighbor/live.flac");
    let root = sandbox.0.join("parent");
    let target = root.join("z-target");
    let alias = root.join("a-dir");
    std::os::unix::fs::symlink("z-target", &alias).unwrap();
    std::os::unix::fs::symlink(
        "../../z-neighbor/live.flac",
        target.join("skip/escape.flac"),
    )
    .unwrap();
    let options = ScanOptions {
        follow_symlinks: true,
        threads: 1,
        ..Default::default()
    };
    let (previous, _) = scan(std::slice::from_ref(&root), None, &options, |_| {}).unwrap();
    assert_paths(
        &previous,
        &[
            alias.join("keep.flac"),
            alias.join("skip/ordinary.flac"),
            alias.join("skip/escape.flac"),
            neighbor.clone(),
        ],
    );
    let options = ScanOptions {
        force_read: true,
        unavailable_roots: vec![target.clone()],
        excluded: vec![target.join("skip")],
        ..options
    };
    let (catalog, report) = scan(&[root], Some(&previous), &options, |_| {}).unwrap();
    assert_paths(&catalog, &[alias.join("keep.flac"), neighbor]);
    assert_cached_file(&previous, &catalog, &alias.join("keep.flac"));
    assert_eq!(report.preserved, 1);
    assert_removed(
        &report,
        &[
            alias.join("skip/ordinary.flac"),
            alias.join("skip/escape.flac"),
        ],
    );
}
