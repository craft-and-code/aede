use super::{Queue, Repeat, Transport};

#[test]
fn selection_order_and_duplicate_entries_are_preserved() {
    let queue = Queue::new(vec![4, 2, 4], None);
    assert_eq!(queue.selection(), [4, 2, 4]);
    assert_eq!(queue.ordered_tracks().collect::<Vec<_>>(), [4, 2, 4]);
    assert_eq!(queue.current(), None);
}

#[test]
fn one_seed_produces_a_reproducible_permutation() {
    let tracks: Vec<u32> = (0..20).collect();
    let one = Queue::new(tracks.clone(), Some(42));
    let two = Queue::new(tracks.clone(), Some(42));
    let order = one.ordered_tracks().collect::<Vec<_>>();
    assert_eq!(order, two.ordered_tracks().collect::<Vec<_>>());
    assert_ne!(order, tracks);
    let mut sorted = order;
    sorted.sort_unstable();
    assert_eq!(sorted, tracks);
}

#[test]
fn play_pause_stop_and_resume_keep_a_clear_cursor() {
    let mut queue = Queue::new(vec![7, 8], None);
    assert_eq!(queue.transport(), Transport::Stopped);
    assert_eq!(queue.play(), Some(7));
    assert!(queue.set_position_ms(1_200));
    queue.pause();
    assert_eq!(queue.transport(), Transport::Paused);
    assert_eq!(queue.play(), Some(7));
    assert_eq!(queue.position_ms(), 1_200);
    queue.stop();
    assert_eq!(queue.transport(), Transport::Stopped);
    assert_eq!(queue.current(), Some(7));
    assert_eq!(queue.position_ms(), 0);
}

#[test]
fn previous_restarts_after_three_seconds_and_steps_back_before_that() {
    let mut queue = Queue::new(vec![1, 2, 3], None);
    queue.play();
    assert_eq!(queue.skip_next(), Some(2));
    queue.set_position_ms(3_001);
    assert_eq!(queue.previous(), Some(2));
    assert_eq!(queue.position_ms(), 0);
    assert_eq!(queue.previous(), Some(1));
    assert_eq!(queue.previous(), Some(1));
}

#[test]
fn manual_next_skips_repeat_one_but_natural_end_restarts() {
    let mut queue = Queue::new(vec![1, 2], None);
    queue.set_repeat(Repeat::One);
    queue.play();
    queue.set_position_ms(9_000);
    assert_eq!(queue.finished(), Some(1));
    assert_eq!(queue.position_ms(), 0);
    assert_eq!(queue.skip_next(), Some(2));
    assert_eq!(queue.transport(), Transport::Playing);
}

#[test]
fn end_of_unrepeated_queue_stops_and_can_be_played_again() {
    let mut queue = Queue::new(vec![1], None);
    queue.play();
    assert_eq!(queue.finished(), None);
    assert_eq!(queue.transport(), Transport::Stopped);
    assert_eq!(queue.current(), None);
    assert_eq!(queue.play(), Some(1));
}

#[test]
fn repeat_all_advances_seed_for_each_new_shuffled_cycle() {
    let mut queue = Queue::new(vec![1, 2, 3, 4], Some(8));
    queue.set_repeat(Repeat::All);
    queue.play();
    let first_cycle = queue.ordered_tracks().collect::<Vec<_>>();
    for _ in 1..4 {
        queue.finished();
    }
    assert_eq!(queue.cursor(), Some(3));
    assert!(queue.finished().is_some());
    assert_eq!(queue.cursor(), Some(0));
    assert_ne!(queue.seed(), Some(8));
    let second_cycle = queue.ordered_tracks().collect::<Vec<_>>();
    assert_ne!(first_cycle, second_cycle);
    assert_eq!(
        second_cycle,
        Queue::new(vec![1, 2, 3, 4], queue.seed())
            .ordered_tracks()
            .collect::<Vec<_>>()
    );
    let first_of_second = queue.current();
    assert_eq!(queue.previous(), first_cycle.last().copied());
    assert_eq!(queue.skip_next(), first_of_second);
}

#[test]
fn empty_queue_never_claims_to_play() {
    let mut queue = Queue::new(Vec::new(), Some(0));
    assert_eq!(queue.play(), None);
    assert_eq!(queue.skip_next(), None);
    assert_eq!(queue.previous(), None);
    assert!(!queue.set_position_ms(100));
    assert_eq!(queue.transport(), Transport::Stopped);
}

#[test]
fn natural_end_only_advances_an_actually_playing_track() {
    let mut queue = Queue::new(vec![1, 2], None);
    assert_eq!(queue.finished(), None);
    assert_eq!(queue.current(), None);
    queue.play();
    queue.pause();
    assert_eq!(queue.finished(), None);
    assert_eq!(queue.current(), Some(1));
}

#[test]
fn replacing_future_order_keeps_current_occurrence_and_the_played_prefix() {
    let mut queue = Queue::new(vec![7, 7, 8, 9], None);
    queue.play();
    queue.skip_next();
    queue.set_position_ms(1_500);
    queue.pause();
    queue
        .reorder(vec![3, 2, 1, 0], Some(9))
        .expect("permutation");
    assert_eq!(queue.ordered_tracks().collect::<Vec<_>>(), [7, 7, 9, 8]);
    assert_eq!(queue.current_entry(), Some(1));
    assert_eq!(queue.position_ms(), 1_500);
    assert_eq!(queue.transport(), Transport::Paused);
    assert_eq!(queue.previous(), Some(7));
    assert_eq!(queue.current_entry(), Some(0));
}

#[test]
fn invalid_reordering_cannot_change_playback_or_discard_an_occurrence() {
    let mut queue = Queue::new(vec![1, 2, 1], Some(3));
    queue.play();
    let before = queue.order().to_vec();
    for invalid in [vec![0, 1], vec![0, 1, 1], vec![0, 1, 3]] {
        assert!(queue.reorder(invalid.clone(), None).is_err());
        assert!(queue.set_next_order(invalid, None).is_err());
        assert_eq!(queue.order(), before);
        assert_eq!(queue.seed(), Some(3));
    }
    queue.set_position_ms(1_500);
    queue.pause();
    assert!(!queue.select_entry(3));
    assert_eq!(queue.position_ms(), 1_500);
    assert!(queue.select_entry(2));
    assert_eq!(queue.current_entry(), Some(2));
    assert_eq!(queue.current(), Some(1));
    assert_eq!(queue.position_ms(), 0);
    assert_eq!(queue.transport(), Transport::Paused);
}

#[test]
fn externally_planned_cycles_keep_previous_and_forward_navigation() {
    let mut queue = Queue::new(vec![10, 11, 12], None);
    queue.set_repeat(Repeat::All);
    queue.play();
    queue.finished();
    queue.finished();
    assert!(
        queue
            .set_next_order(vec![1, 0, 2], Some(77))
            .expect("cycle")
    );
    assert_eq!(queue.finished(), Some(11));
    assert_eq!(queue.previous(), Some(12));
    // A driver planning ahead must not overwrite the cycle restored by Previous.
    assert!(
        !queue
            .set_next_order(vec![2, 0, 1], Some(88))
            .expect("valid")
    );
    assert_eq!(queue.skip_next(), Some(11));
    assert_eq!(queue.seed(), Some(77));
}
