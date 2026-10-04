use super::*;

fn format() -> PcmFormat {
    PcmFormat::new(48_000, 2).unwrap()
}

#[test]
fn consumed_position_keeps_the_heard_occurrence_and_corrects_its_metadata_duration_at_eof() {
    let mut timeline = PlaybackTimeline::new();
    timeline
        .begin(1, format(), 10_000, Some(10_100), "first")
        .unwrap();
    timeline.submitted(1, 48_000 * 8 * 2, 500).unwrap();
    timeline.set_duration(1, 12_000).unwrap();
    timeline.completed(1).unwrap();
    timeline.begin(2, format(), 0, None, "second").unwrap();
    timeline.submitted(2, 48_000 * 8, 500).unwrap();
    timeline.completed(2).unwrap();

    let first = timeline.position(Some(48_000), 90_000).unwrap();
    assert_eq!(first.token, 1);
    assert_eq!(first.position_ms, 11_000);
    assert_eq!(first.output_frames, 48_000);
    assert_eq!(first.duration_ms, Some(12_000));
    assert_eq!(first.label, "first");
    assert!(!first.estimated);
    assert!(!first.completed);
    assert_eq!(
        timeline.position(Some(48_000), 91_000).unwrap().position_ms,
        11_000
    );

    let second = timeline.position(Some(96_000), 91_001).unwrap();
    assert_eq!(second.token, 2);
    assert_eq!(second.position_ms, 0);
    assert_eq!(second.output_frames, 96_000);
    assert_eq!(second.duration_ms, None);
    let final_position = timeline.position(Some(144_000), 91_002).unwrap();
    assert_eq!(final_position.token, 2);
    assert_eq!(final_position.position_ms, 1_000);
    assert!(final_position.completed);
    assert!(timeline.visits.is_empty());
    assert_eq!(
        timeline
            .position(Some(144_000), 99_000)
            .unwrap()
            .position_ms,
        1_000
    );
}

#[test]
fn ffplay_position_starts_at_submission_and_preserves_its_estimated_final_snapshot() {
    let mut timeline = PlaybackTimeline::new();
    timeline.begin(1, format(), 0, Some(200), "first").unwrap();
    assert!(timeline.position(None, 9_000).is_none());
    timeline.submitted(1, 48_000 * 8, 9_000).unwrap();
    assert_eq!(timeline.position(None, 9_000).unwrap().position_ms, 0);
    let heard = timeline.position(None, 9_500).unwrap();
    assert_eq!(
        heard.position_ms, 500,
        "an inaccurate total cannot hide actual progress"
    );
    assert!(heard.estimated);
    assert_eq!(heard.output_frames, 24_000);
    assert_eq!(timeline.position(None, 9_500).unwrap().position_ms, 500);
    assert_eq!(timeline.position(None, 12_000).unwrap().position_ms, 1_000);
    timeline.set_duration(1, 1_000).unwrap();
    timeline.completed(1).unwrap();
    timeline
        .begin(2, format(), 0, None, "prepared next")
        .unwrap();
    let final_position = timeline.position(None, 12_000).unwrap();
    assert_eq!(
        final_position.token, 1,
        "preparation does not change the heard occurrence"
    );
    assert_eq!(final_position.position_ms, 1_000);
    assert!(final_position.completed);
    assert!(final_position.estimated);
    timeline.submitted(2, 48_000 * 8, 12_000).unwrap();
    assert_eq!(timeline.position(None, 12_000).unwrap().token, 2);
}

#[test]
fn zero_length_occurrences_release_the_bound_and_resets_discard_the_previous_output_clock() {
    let mut timeline = PlaybackTimeline::new();
    for token in 0..MAX_PENDING {
        timeline.begin(token, format(), 0, None, "empty").unwrap();
        timeline.completed(token).unwrap();
    }
    assert!(!timeline.has_capacity());
    assert!(
        timeline
            .begin(MAX_PENDING, format(), 0, None, "overflow")
            .is_err()
    );
    let empty = timeline.position(Some(0), 0).unwrap();
    assert_eq!(empty.token, MAX_PENDING - 1);
    assert_eq!(empty.position_ms, 0);
    assert!(empty.completed);
    assert!(timeline.has_capacity());

    let changed = PcmFormat::new(44_100, 1).unwrap();
    assert!(
        timeline
            .begin(MAX_PENDING, changed, 0, None, "changed")
            .is_err()
    );
    timeline.reset_output();
    assert!(timeline.position(Some(1_000), 10_000).is_none());
    timeline
        .begin(MAX_PENDING, changed, 0, None, "changed")
        .unwrap();
    timeline.submitted(MAX_PENDING, 44_100 * 4, 10_000).unwrap();
    assert_eq!(timeline.position(Some(0), 10_000).unwrap().position_ms, 0);
}

#[test]
fn partial_frame_writes_count_only_complete_audio_and_labels_are_safe_before_rendering() {
    let mut timeline = PlaybackTimeline::new();
    timeline
        .begin(
            1,
            format(),
            0,
            None,
            "\u{1b}[2J unsafe\nlabel\t".repeat(20).as_str(),
        )
        .unwrap();
    timeline.submitted(1, 48_000 * 8 - 1, 0).unwrap();
    let position = timeline.position(Some(u64::MAX), 0).unwrap();
    assert_eq!(position.position_ms, 999);
    assert!(!position.label.contains(['\u{1b}', '\n', '\t']));
    assert!(position.label.chars().count() <= 80);
    timeline.submitted(1, 1, 0).unwrap();
    assert_eq!(
        timeline.position(Some(u64::MAX), 0).unwrap().position_ms,
        1_000
    );
    assert!(timeline.submitted(2, 1, 0).is_err());
    assert!(timeline.completed(2).is_err());
    assert!(timeline.set_duration(2, 1_000).is_err());
    timeline.submitted_bytes = u64::MAX;
    assert!(timeline.submitted(1, 1, 0).is_err());
    assert_eq!(timeline.submitted_bytes, u64::MAX);
}
