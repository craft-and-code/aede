use super::*;

#[test]
fn structured_preview_lists_changes_and_unreadable_paths() {
    let report = ScanReport {
        found: 2,
        read: 1,
        reused: 1,
        removed: 1,
        preserved: 1,
        added_paths: vec!["/music/new.flac".into()],
        changed_paths: vec!["/music/changed.flac".into()],
        removed_paths: vec!["/music/gone.flac".into()],
        failures: vec![("/music/offline".into(), "permission denied".into())],
        ..Default::default()
    };
    let catalog = Catalog {
        roots: vec!["/music".into()],
        ..Default::default()
    };
    let result = as_json(&report, &catalog, true);
    assert_eq!(result.get("dry_run"), Some(&Json::Bool(true)));
    assert_eq!(result.get("added"), Some(&strings(&report.added_paths)));
    assert_eq!(result.get("changed"), Some(&strings(&report.changed_paths)));
    assert_eq!(result.get("removed"), Some(&strings(&report.removed_paths)));
    let failure = &result.get("unreadable").unwrap().as_arr().unwrap()[0];
    assert_eq!(failure.field_str("path").as_deref(), Some("/music/offline"));
    assert_eq!(
        failure.field_str("reason").as_deref(),
        Some("permission denied")
    );
    assert_eq!(
        result.get("counts").unwrap().field_u64("preserved"),
        Some(1)
    );
}
