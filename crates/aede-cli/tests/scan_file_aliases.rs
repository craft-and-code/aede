#![cfg(unix)]

mod scan_file_aliases_support;

use scan_file_aliases_support::{
    Fixture, assert_cached, assert_identity_refusal, assert_paths, count, file, success,
};

#[test]
fn ordinary_full_scan_prunes_excluded_regular_file_aliases_while_absent_and_after_remount() {
    let fixture = Fixture::new();
    let previous = fixture.seed(true);
    let excluded = fixture.target.join("skip");
    fixture.exclude(&excluded);
    fixture.detach();
    fixture.add_neighbor();
    let report = success(fixture.full_scan(true, None));
    let catalog = fixture.catalog();
    // Only the surviving file alias establishes unavailability when its target
    // disappears from the parent and was not separately declared offline.
    let mut expected = fixture.expected(true, true, false);
    expected.retain(|path| path != &fixture.kept);
    assert_paths(&catalog, &expected);
    assert_cached(&previous, &catalog, &fixture.kept_alias);
    assert_eq!(count(&report, "preserved"), 1);
    assert_eq!(count(&report, "removed"), 3);
    fixture.assert_policy(std::slice::from_ref(&excluded));

    fixture.remount();
    fixture.replace_kept();
    let returned = fixture.track("parent/z-target/returned.flac");
    fixture.track("parent/z-target/skip/still-excluded.flac");
    let report = success(fixture.full_scan(true, None));
    let catalog = fixture.catalog();
    let mut expected = fixture.expected(true, true, false);
    expected.push(returned);
    assert_paths(&catalog, &expected);
    for path in [&fixture.kept, &fixture.kept_alias] {
        assert_ne!(file(&catalog, path).tags, file(&previous, path).tags);
    }
    assert_eq!(count(&report, "preserved"), 0);
    fixture.assert_policy(&[excluded]);
}

#[test]
fn delegated_full_scan_applies_partial_and_whole_exclusions_to_regular_file_aliases() {
    for absent in [false, true] {
        for whole in [false, true] {
            let fixture = Fixture::new();
            let previous = fixture.seed(true);
            let excluded = if whole {
                fixture.target.clone()
            } else {
                fixture.target.join("skip")
            };
            fixture.exclude(&excluded);
            if absent {
                fixture.detach();
            } else {
                // Even valid replacements must not replace cached offline tags.
                fixture.replace_kept();
                fixture.track("parent/z-target/new.flac");
            }
            fixture.add_neighbor();
            let report = success(fixture.full_scan(true, Some(&fixture.target)));
            let catalog = fixture.catalog();
            assert_paths(&catalog, &fixture.expected(true, !whole, whole));
            if !whole {
                assert_cached(&previous, &catalog, &fixture.kept);
                assert_cached(&previous, &catalog, &fixture.kept_alias);
            }
            assert_eq!(count(&report, "preserved"), if whole { 0 } else { 2 });
            assert_eq!(count(&report, "removed"), if whole { 4 } else { 2 });
            assert_eq!(count(&report, "read"), 2);
            fixture.assert_policy(&[excluded]);
        }
    }
}

#[test]
fn delegated_full_scan_keeps_file_alias_cache_despite_valid_or_invalid_offline_bytes() {
    for valid in [false, true] {
        let fixture = Fixture::new();
        let previous = fixture.seed(true);
        if valid {
            fixture.replace_kept();
        } else {
            std::fs::write(&fixture.kept, b"offline invalid bytes must not be parsed").unwrap();
            std::fs::write(
                &fixture.removed,
                b"offline invalid bytes must not be parsed",
            )
            .unwrap();
        }
        fixture.track("parent/z-target/new.flac");
        fixture.add_neighbor();
        let report = success(fixture.full_scan(true, Some(&fixture.target)));
        let catalog = fixture.catalog();
        assert_paths(&catalog, &fixture.expected(true, false, false));
        for path in [
            &fixture.kept,
            &fixture.kept_alias,
            &fixture.removed,
            &fixture.removed_alias,
        ] {
            assert_cached(&previous, &catalog, path);
        }
        assert_eq!(count(&report, "preserved"), 4);
        assert_eq!(count(&report, "found"), 2);
        assert_eq!(count(&report, "read"), 2);
        assert_eq!(count(&report, "removed"), 0);
        fixture.assert_policy(&[]);
    }
}

#[test]
fn ordinary_full_scan_keeps_unexcluded_file_aliases_cached_and_refreshes_them_after_remount() {
    let fixture = Fixture::new();
    let previous = fixture.seed(true);
    fixture.detach();
    fixture.add_neighbor();
    let report = success(fixture.full_scan(true, None));
    let catalog = fixture.catalog();
    let mut expected = fixture.expected(true, false, false);
    expected.retain(|path| path != &fixture.kept && path != &fixture.removed);
    assert_paths(&catalog, &expected);
    for path in [&fixture.kept_alias, &fixture.removed_alias] {
        assert_cached(&previous, &catalog, path);
    }
    assert_eq!(count(&report, "preserved"), 2);
    assert_eq!(count(&report, "removed"), 2);

    fixture.remount();
    fixture.replace_kept();
    let report = success(fixture.full_scan(true, None));
    let catalog = fixture.catalog();
    assert_paths(&catalog, &fixture.expected(true, false, false));
    assert_ne!(
        file(&catalog, &fixture.kept_alias).tags,
        file(&previous, &fixture.kept_alias).tags
    );
    assert_eq!(
        file(&catalog, &fixture.kept_alias).tags,
        file(&catalog, &fixture.kept).tags
    );
    assert_eq!(count(&report, "preserved"), 0);
    fixture.assert_policy(&[]);
}

#[test]
fn full_scans_without_following_links_never_publish_regular_file_aliases() {
    for mode in ["mounted", "offline-accessible", "offline-absent"] {
        for partial in [false, true] {
            let fixture = Fixture::new();
            let previous = fixture.seed(false);
            let excluded = if partial {
                vec![fixture.target.join("skip")]
            } else {
                Vec::new()
            };
            if let Some(path) = excluded.first() {
                fixture.exclude(path);
            }
            if mode == "offline-absent" {
                fixture.detach();
            } else if mode == "offline-accessible" {
                std::fs::write(&fixture.kept, b"offline").unwrap();
            }
            fixture.add_neighbor();
            let unavailable = (mode != "mounted").then_some(fixture.target.as_path());
            let report = success(fixture.full_scan(false, unavailable));
            let catalog = fixture.catalog();
            assert_paths(&catalog, &fixture.expected(false, partial, false));
            if unavailable.is_some() {
                assert_cached(&previous, &catalog, &fixture.kept);
            }
            assert_eq!(
                count(&report, "preserved"),
                if unavailable.is_some() {
                    if partial { 1 } else { 2 }
                } else {
                    0
                }
            );
            assert_eq!(count(&report, "removed"), u64::from(partial));
            fixture.assert_policy(&excluded);
        }
    }
}

#[test]
fn ambiguous_selection_waives_only_proven_file_alias_exclusions_before_store_publication() {
    for remove_neighbor in [false, true] {
        let fixture = Fixture::new();
        let previous = fixture.seed(true);
        let excluded = fixture.target.join("skip");
        fixture.exclude(&excluded);
        fixture.initialize_independent_stores();
        let before = fixture.snapshot();
        let selection = fixture.dir.join("selected-volume");
        std::os::unix::fs::symlink(&fixture.target, &selection).unwrap();
        fixture.detach();
        fixture.add_neighbor();
        if remove_neighbor {
            std::fs::remove_file(fixture.root.join("z-neighbor/live.flac")).unwrap();
        }
        let output = fixture.full_scan(true, Some(&selection));
        if remove_neighbor {
            assert_identity_refusal(&output);
            assert_eq!(fixture.snapshot(), before);
        } else {
            let report = success(output);
            let catalog = fixture.catalog();
            assert_paths(&catalog, &fixture.expected(true, true, false));
            for path in [&fixture.kept, &fixture.kept_alias] {
                assert_cached(&previous, &catalog, path);
            }
            assert_eq!(count(&report, "removed"), 2);
            assert_eq!(count(&report, "preserved"), 2);
        }
        fixture.assert_policy(&[excluded]);
    }
}
