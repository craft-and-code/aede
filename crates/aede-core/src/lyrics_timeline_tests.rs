use super::super::{Line, Source, from_tag};
use super::*;

fn lyrics_with(lines: Vec<Line>) -> Lyrics {
    Lyrics {
        source: Source::Tag,
        origin: "track.flac".into(),
        lines,
    }
}

#[test]
fn plain_lyrics_do_not_acquire_an_invented_clock() {
    let lyrics = from_tag("track.flac", "first\n\nsecond").unwrap();
    let timeline = Timeline::new(&lyrics);
    assert!(timeline.is_empty());
    assert!(timeline.cues().is_empty());
    assert_eq!(timeline.active_at(0), None);
    assert_eq!(timeline.active_at(u64::MAX), None);
    assert_eq!(lyrics.text(), "first\n\nsecond");
    assert!(Timeline::new(&lyrics_with(Vec::new())).is_empty());
}

#[test]
fn cue_boundaries_are_inclusive_and_the_last_words_remain_until_track_end() {
    let lyrics = from_tag("track.flac", "[00:01]first\n[00:02]second").unwrap();
    let timeline = Timeline::new(&lyrics);
    assert_eq!(timeline.active_at(999), None);
    for position in [1_000, 1_001, 1_999] {
        assert_eq!(timeline.active_at(position).unwrap().line_indices, [0]);
    }
    for position in [2_000, 2_001, u64::MAX] {
        assert_eq!(timeline.active_at(position).unwrap().line_indices, [1]);
    }
}

#[test]
fn unsorted_equal_timestamps_share_a_group_in_the_original_line_order() {
    let lyrics = from_tag(
        "track.flac",
        "header\n[00:03]last\n[00:01]first\nuntimed\n[00:01]translation\n[00:02]middle",
    )
    .unwrap();
    let original = lyrics.clone();
    let timeline = Timeline::new(&lyrics);
    assert_eq!(
        timeline.cues(),
        [
            Cue {
                at_ms: 1_000,
                line_indices: vec![2, 4],
            },
            Cue {
                at_ms: 2_000,
                line_indices: vec![5],
            },
            Cue {
                at_ms: 3_000,
                line_indices: vec![1],
            },
        ]
    );
    assert_eq!(lyrics, original);
}

#[test]
fn timed_blank_cues_clear_words_at_verse_breaks_and_after_the_last_verse() {
    let lyrics = from_tag(
        "track.flac",
        "[00:01]first\n[00:02]\n[00:03]second\n[00:04]",
    )
    .unwrap();
    let timeline = Timeline::new(&lyrics);
    for position in [2_000, 2_999, 4_000, u64::MAX] {
        let cue = timeline.active_at(position).unwrap();
        assert!(
            cue.line_indices
                .iter()
                .all(|&index| lyrics.lines[index].text.is_empty())
        );
    }
    assert_eq!(timeline.active_at(3_000).unwrap().line_indices, [2]);
}

#[test]
fn pause_backward_seeking_and_repeat_need_only_the_current_position() {
    let lyrics = from_tag("track.flac", "[00:00]first\n[00:02]second\n[00:04]last").unwrap();
    let timeline = Timeline::new(&lyrics);
    let positions = [0, 2_200, 2_200, 4_900, 1_100, 0, 4_000];
    let expected = [0, 1, 1, 2, 0, 0, 2];
    for (position, index) in positions.into_iter().zip(expected) {
        assert_eq!(timeline.active_at(position).unwrap().line_indices, [index]);
    }
}

#[test]
fn parsed_offsets_are_not_applied_twice_and_repeated_choruses_keep_both_cues() {
    let lyrics = from_tag(
        "track.flac",
        "[offset:+500]\n[00:01][00:03]chorus\n[00:02]verse",
    )
    .unwrap();
    let timeline = Timeline::new(&lyrics);
    assert_eq!(
        timeline
            .cues()
            .iter()
            .map(|cue| cue.at_ms)
            .collect::<Vec<_>>(),
        [1_500, 2_500, 3_500]
    );
    assert_eq!(timeline.active_at(1_499), None);
    assert_eq!(timeline.active_at(1_500).unwrap().line_indices, [0]);
    assert_eq!(timeline.active_at(3_500).unwrap().line_indices, [1]);
    assert_eq!(lyrics.lines[0].text, lyrics.lines[1].text);
}

#[test]
fn zero_and_maximum_timestamps_have_no_overflow_or_special_case() {
    let lyrics = lyrics_with(vec![
        Line {
            at_ms: Some(u64::MAX),
            text: "last".into(),
        },
        Line {
            at_ms: Some(0),
            text: "first".into(),
        },
        Line {
            at_ms: None,
            text: "[9223372036854775807:00]invalid timing".into(),
        },
    ]);
    let timeline = Timeline::new(&lyrics);
    for position in [0, 1, u64::MAX - 1] {
        assert_eq!(timeline.active_at(position).unwrap().line_indices, [1]);
    }
    assert_eq!(timeline.active_at(u64::MAX).unwrap().line_indices, [0]);
}

#[test]
fn large_reverse_ordered_lyrics_support_repeated_arbitrary_position_lookups() {
    const COUNT: usize = 32_768;
    let lyrics = lyrics_with(
        (0..COUNT)
            .rev()
            .map(|index| Line {
                at_ms: Some(index as u64 * 1_000),
                text: "words".into(),
            })
            .collect(),
    );
    let timeline = Timeline::new(&lyrics);
    assert_eq!(timeline.cues().len(), COUNT);
    for request in 0..COUNT {
        let index = request * 8_191 % COUNT;
        let cue = timeline.active_at(index as u64 * 1_000 + 999).unwrap();
        assert_eq!(cue.at_ms, index as u64 * 1_000);
        assert_eq!(cue.line_indices, [COUNT - index - 1]);
    }
}
