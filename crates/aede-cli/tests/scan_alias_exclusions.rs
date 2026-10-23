#![cfg(unix)]

mod scan_alias_exclusions_support;

use scan_alias_exclusions_support::{
    Fixture, assert_cached, assert_identity_refusal, assert_paths, success,
};

#[test]
fn ordinary_full_scan_removes_excluded_cached_alias_tracks_while_the_target_is_absent() {
    let fixture = Fixture::new();
    let previous = fixture.seed(true, None);
    let excluded = fixture.target.join("excluded");
    fixture.exclude(&excluded);
    fixture.detach();
    fixture.track("parent/z-neighbor/added.flac");
    let report = success(fixture.full_scan(None));
    let catalog = fixture.catalog();
    assert_paths(&catalog, &fixture.expected_partial());
    for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
        assert_cached(&previous, &catalog, &fixture.alias.join(relative));
    }
    assert_eq!(
        report.get("counts").unwrap().field_u64("preserved"),
        Some(2)
    );
    assert_eq!(report.get("counts").unwrap().field_u64("removed"), Some(1));
    fixture.assert_exclusion(&excluded);

    fixture.remount();
    std::fs::remove_file(fixture.target.join(r"kept\native.flac")).unwrap();
    fixture.track("parent/z-target/returned.flac");
    fixture.track("parent/z-target/excluded/still-excluded.flac");
    let report = success(fixture.full_scan(None));
    assert_paths(
        &fixture.catalog(),
        &[
            fixture.alias.join("returned.flac"),
            fixture.alias.join("excluded-neighbor/kept.flac"),
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ],
    );
    assert_eq!(
        report.get("counts").unwrap().field_u64("preserved"),
        Some(0)
    );
    assert_eq!(report.get("counts").unwrap().field_u64("removed"), Some(1));
    fixture.assert_exclusion(&excluded);
}

#[test]
fn owned_full_scan_keeps_partial_exclusions_when_the_followed_target_is_absent() {
    let fixture = Fixture::new();
    let previous = fixture.seed(true, None);
    let excluded = fixture.target.join("excluded");
    fixture.exclude(&excluded);
    fixture.detach();
    fixture.track("parent/z-neighbor/added.flac");
    let report = success(fixture.full_scan(Some(&fixture.target)));
    let catalog = fixture.catalog();
    assert_paths(&catalog, &fixture.expected_partial());
    for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
        assert_cached(&previous, &catalog, &fixture.alias.join(relative));
    }
    assert_eq!(
        report.get("counts").unwrap().field_u64("preserved"),
        Some(2)
    );
    assert_eq!(report.get("counts").unwrap().field_u64("removed"), Some(1));
    fixture.assert_exclusion(&excluded);
}

#[test]
fn owned_full_scan_preserves_nonexcluded_cached_metadata_without_reading_the_offline_target() {
    let fixture = Fixture::new();
    let previous = fixture.seed(true, None);
    let excluded = fixture.target.join("excluded");
    fixture.exclude(&excluded);
    std::fs::write(
        fixture.target.join(r"kept\native.flac"),
        b"accessible offline bytes must not be parsed",
    )
    .unwrap();
    fixture.track("parent/z-target/new.flac");
    fixture.track("parent/z-neighbor/added.flac");
    let report = success(fixture.full_scan(Some(&fixture.target)));
    let catalog = fixture.catalog();
    assert_paths(&catalog, &fixture.expected_partial());
    for relative in [r"kept\native.flac", "excluded-neighbor/kept.flac"] {
        assert_cached(&previous, &catalog, &fixture.alias.join(relative));
    }
    assert_eq!(
        report.get("counts").unwrap().field_u64("preserved"),
        Some(2)
    );
    assert_eq!(report.get("counts").unwrap().field_u64("read"), Some(2));
    fixture.assert_exclusion(&excluded);
}

#[test]
fn whole_target_exclusion_never_successfully_publishes_retained_alias_tracks() {
    for absent in [false, true] {
        let fixture = Fixture::new();
        fixture.seed(true, None);
        fixture.exclude(&fixture.target);
        fixture.initialize_independent_stores();
        let before = fixture.snapshot();
        if absent {
            fixture.detach();
        }
        fixture.track("parent/z-neighbor/added.flac");
        let output = fixture.full_scan(Some(&fixture.target));
        let expected = [
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ];
        if output.status.success() {
            let report = success(output);
            assert_paths(&fixture.catalog(), &expected);
            assert_eq!(report.get("counts").unwrap().field_u64("removed"), Some(3));
        } else {
            // AE-035 may conservatively refuse an unknown historical alias
            // association. Such a refusal must precede every store write.
            assert_identity_refusal(&output);
            assert_eq!(fixture.snapshot(), before);
        }
        fixture.assert_exclusion(&fixture.target);

        if absent {
            fixture.remount();
        }
        success(fixture.full_scan(None));
        assert_paths(&fixture.catalog(), &expected);
        fixture.assert_exclusion(&fixture.target);
    }
}

#[test]
fn an_unresolvable_selected_alias_with_partial_exclusions_is_refused_before_store_publication() {
    let fixture = Fixture::new();
    std::fs::remove_file(&fixture.alias).unwrap();
    let selected_alias = fixture.dir.join("selected-volume");
    std::os::unix::fs::symlink(&fixture.target, &selected_alias).unwrap();
    let previous = fixture.seed(false, Some(&selected_alias));
    fixture.exclude(&fixture.target.join("excluded"));
    fixture.initialize_independent_stores();
    let before = fixture.snapshot();
    std::fs::remove_file(&selected_alias).unwrap();
    fixture.detach();
    fixture.track("parent/z-neighbor/added.flac");

    let output = fixture.full_scan(Some(&selected_alias));
    assert_identity_refusal(&output);
    assert_eq!(fixture.snapshot(), before);

    fixture.remount();
    std::os::unix::fs::symlink(&fixture.target, &selected_alias).unwrap();
    success(fixture.full_scan(Some(&selected_alias)));
    let catalog = fixture.catalog();
    assert_paths(
        &catalog,
        &[
            fixture.target.join(r"kept\native.flac"),
            fixture.target.join("excluded-neighbor/kept.flac"),
            fixture.root.join("z-neighbor/live.flac"),
            fixture.root.join("z-neighbor/added.flac"),
        ],
    );
    assert_cached(
        &previous,
        &catalog,
        &fixture.target.join(r"kept\native.flac"),
    );
}
