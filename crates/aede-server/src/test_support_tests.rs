use super::*;

#[test]
fn overlapping_scan_workers_remain_active_until_the_last_guard_drops() {
    let activity = crate::state::ScanActivity::default();
    assert!(!activity.is_running());
    let first = activity.start();
    let second = activity.clone().start();
    assert!(activity.is_running());
    drop(first);
    assert!(activity.is_running());
    drop(second);
    assert!(!activity.is_running());
}

#[test]
fn sample_states_isolate_optional_stores_without_creating_folders() {
    let first = sample_state();
    let second = sample_state();
    assert_ne!(first.data_dir, second.data_dir);
    for state in [first, second] {
        assert_eq!(
            state.data_dir.parent(),
            Some(std::env::temp_dir().as_path())
        );
        assert!(
            !state.data_dir.exists(),
            "read-only fixtures need no disk cleanup"
        );
        assert!(auth::load_accounts(&state).unwrap().is_none());
    }
}
