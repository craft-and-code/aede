use super::*;
use aede_core::{
    model::EntityKind,
    user::{EntityRef, UserData},
};
#[test]
fn duration_preserves_exact_owner_matching_tracks_and_unknown_values() {
    let state = crate::test_support::sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.tracks[0].duration_ms = Some(42_000);
    let mut data = UserData::default();
    let target = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    data.entry("listener", &target, 1).loved = true;
    let context = aede_core::query::Context::new(&catalog, &data, "listener");
    assert_eq!(duration(&context, "loved"), Some(42_000));
    assert_eq!(
        duration(
            &aede_core::query::Context::new(&catalog, &data, "other"),
            "loved"
        ),
        Some(0)
    );
    catalog.tracks[0].duration_ms = None;
    assert_eq!(
        duration(
            &aede_core::query::Context::new(&catalog, &data, "listener"),
            "loved"
        ),
        None
    );
    assert_eq!(
        duration(
            &aede_core::query::Context::new(&catalog, &data, "listener"),
            "not-a-query:"
        ),
        None
    );
}
