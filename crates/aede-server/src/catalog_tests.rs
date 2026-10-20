use super::*;
use crate::test_support::sample_state;

#[test]
fn catalogue_revision_detects_reload_even_with_unchanged_scan_date_and_counts() {
    crate::test_support::test_runtime().block_on(async {
        let mut state = sample_state();
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        state.data_dir =
            std::env::temp_dir().join(format!("aede_revision_{}_{nonce}", std::process::id()));
        std::fs::create_dir(&state.data_dir).unwrap();
        let path = store::catalog_path(&state.data_dir);
        let before = serde_json::to_value(library(State(state.clone())).await.unwrap().0).unwrap();
        assert!(before["catalog_revision"].as_str().is_some());
        let mut changed = state.catalog.read().await.as_ref().unwrap().clone();
        changed.roots = vec!["/music/replacement".into()];
        store::save_catalog_only(&changed, &path).unwrap();
        let mut known = None;
        let mut failure = None;
        refresh_catalog(&path, &state, &mut known, &mut failure).await;
        let after = serde_json::to_value(library(State(state.clone())).await.unwrap().0).unwrap();
        assert_eq!(after["scanned_at"], before["scanned_at"]);
        assert_eq!(after["tracks"], before["tracks"]);
        assert_ne!(after["catalog_revision"], before["catalog_revision"]);
        refresh_catalog(&path, &state, &mut known, &mut failure).await;
        let unchanged =
            serde_json::to_value(library(State(state.clone())).await.unwrap().0).unwrap();
        assert_eq!(unchanged["catalog_revision"], after["catalog_revision"]);
        std::fs::write(&path, "invalid catalogue").unwrap();
        refresh_catalog(&path, &state, &mut known, &mut failure).await;
        let rejected =
            serde_json::to_value(library(State(state.clone())).await.unwrap().0).unwrap();
        assert_eq!(rejected["catalog_revision"], after["catalog_revision"]);
        std::fs::remove_dir_all(&state.data_dir).unwrap();
    });
}
