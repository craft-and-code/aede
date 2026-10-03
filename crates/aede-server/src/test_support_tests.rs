use super::*;

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
