use super::*;

fn paths() -> Vec<PathBuf> {
    ["one.wav", "two.wav", "one.wav"]
        .map(PathBuf::from)
        .to_vec()
}

#[test]
fn the_driver_retains_playlist_occurrences_and_bypasses_repeat_one_on_next() {
    let paths = paths();
    let options = PlaybackOptions {
        repeat: Repeat::One,
        ..Default::default()
    };
    let mut order = PlaybackOrder::new(&paths, None, &options).expect("order");
    assert_eq!(order.current(), Some(0));
    assert_eq!(order.advance(PlaybackEnd::Natural, 5_000).unwrap(), Some(0));
    assert_eq!(order.advance(PlaybackEnd::Next, 5_000).unwrap(), Some(1));
    assert_eq!(
        order.advance(PlaybackEnd::Previous, 3_001).unwrap(),
        Some(1)
    );
    assert_eq!(
        order.advance(PlaybackEnd::Previous, 1_000).unwrap(),
        Some(0)
    );
}

#[test]
fn runtime_shuffle_preserves_played_prefix_current_track_and_all_remaining_entries() {
    let paths = paths();
    let mut order = PlaybackOrder::new(&paths, None, &PlaybackOptions::default()).unwrap();
    order.advance(PlaybackEnd::Natural, 1_000).unwrap();
    let mut clock = PlaybackClock::new();
    clock.shuffle = PlaybackShuffle::Random;
    clock.shuffle_revision = 1;
    order.sync(&clock, None, &paths).unwrap();
    assert_eq!(order.current(), Some(1));
    assert_eq!(&order.queue.order()[..2], [0, 1]);
    let mut occurrences = order.queue.order().to_vec();
    occurrences.sort_unstable();
    assert_eq!(occurrences, [0, 1, 2]);
    clock.shuffle = PlaybackShuffle::Off;
    clock.shuffle_revision = 2;
    order.sync(&clock, None, &paths).unwrap();
    assert_eq!(order.queue.order(), [0, 1, 2]);
}

#[test]
fn repeat_all_new_seed_and_previous_restore_the_actual_cycle() {
    let paths = paths();
    let options = PlaybackOptions {
        repeat: Repeat::All,
        shuffle: PlaybackShuffle::Random,
        seed: Some(42),
        ..Default::default()
    };
    let mut order = PlaybackOrder::new(&paths, None, &options).unwrap();
    let first = order.queue.order().to_vec();
    for _ in 0..paths.len() {
        order.advance(PlaybackEnd::Natural, 1_000).unwrap();
    }
    let second_first = order.current();
    assert_ne!(order.queue.seed(), Some(42));
    assert_eq!(
        order.advance(PlaybackEnd::Previous, 1_000).unwrap(),
        first.last().copied()
    );
    assert_eq!(
        order.advance(PlaybackEnd::Next, 1_000).unwrap(),
        second_first
    );
}

#[test]
fn explicit_smart_mode_without_catalog_fails_before_any_audio_is_opened() {
    let options = PlaybackOptions {
        shuffle: PlaybackShuffle::Smart,
        ..Default::default()
    };
    assert!(PlaybackOrder::new(&paths(), None, &options).is_err());
}

#[test]
fn interrupting_preparation_of_a_new_cycle_returns_to_the_active_cycle() {
    let paths = paths();
    let options = PlaybackOptions {
        repeat: Repeat::All,
        shuffle: PlaybackShuffle::Random,
        seed: Some(42),
        ..Default::default()
    };
    let mut order = PlaybackOrder::new(&paths, None, &options).unwrap();
    let old_last = *order.queue.order().last().unwrap();
    for _ in 0..paths.len() {
        order.advance(PlaybackEnd::Natural, 1_000).unwrap();
    }
    let next_first = order.current();
    let next_seed = order.queue.seed();
    order.focus(old_last).unwrap();
    assert_eq!(order.queue.seed(), Some(42));
    assert_eq!(order.advance(PlaybackEnd::Next, 1_000).unwrap(), next_first);
    assert_eq!(order.queue.seed(), next_seed);
}
