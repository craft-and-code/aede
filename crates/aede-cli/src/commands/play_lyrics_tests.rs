use super::*;

fn display() -> PlaybackLyrics {
    PlaybackLyrics::new()
}

fn format() -> PcmFormat {
    PcmFormat::new(48_000, 2).unwrap()
}

fn words(text: &str) -> Option<Lyrics> {
    lyrics::from_tag("track.flac", text)
}

fn contains(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

#[test]
fn queued_next_track_waits_for_consumed_frames_and_preserves_seek_offset() {
    let mut display = display();
    display
        .enqueue(1, words("[00:00]first\n[00:01]later"), "first", format(), 0)
        .unwrap();
    display.submitted(1, 48_000 * 8 * 2, 0).unwrap();
    display.completed(1).unwrap();
    display
        .enqueue(
            2,
            words("[00:00]intro\n[00:10]seek verse"),
            "second",
            format(),
            10_000,
        )
        .unwrap();
    display.submitted(2, 48_000 * 8, 0).unwrap();
    display.completed(2).unwrap();
    let rows = display.updates(Some(0), 20_000, 80);
    assert!(contains(&rows, "first"));
    assert!(!contains(&rows, "second"));
    assert!(
        display.updates(Some(0), 20_100, 80).is_empty(),
        "pause does not advance a consumed-frame clock"
    );
    assert!(contains(
        &display.updates(Some(48_000), 20_200, 80),
        "later"
    ));
    let rows = display.updates(Some(96_000), 20_300, 80);
    assert!(contains(&rows, "second"));
    assert!(contains(&rows, "seek verse"));
    assert!(!contains(&rows, "intro"));
    assert!(display.updates(Some(144_000), 20_400, 80).is_empty());
    assert!(display.visits.is_empty());
}

#[test]
fn repeated_file_has_a_new_lyric_occurrence_on_the_continuous_output_clock() {
    let mut display = display();
    for token in [1, 2] {
        display
            .enqueue(token, words("[00:00]opening"), "same file", format(), 0)
            .unwrap();
        display.submitted(token, 48_000 * 8, 0).unwrap();
        display.completed(token).unwrap();
    }
    assert!(contains(&display.updates(Some(0), 0, 80), "opening"));
    assert!(display.updates(Some(24_000), 500, 80).is_empty());
    assert!(contains(
        &display.updates(Some(48_000), 1_000, 80),
        "opening"
    ));
}

#[test]
fn manual_transport_discards_lookahead_and_restarts_the_timeline() {
    let mut display = display();
    display
        .enqueue(
            1,
            words("[00:00]old\n[00:10]old later"),
            "old file",
            format(),
            0,
        )
        .unwrap();
    display.submitted(1, 48_000 * 8 * 20, 0).unwrap();
    assert!(contains(
        &display.updates(Some(48_000 * 12), 12_000, 80),
        "old later"
    ));
    display.reset_output();
    display
        .enqueue(2, words("[00:00]new opening"), "new file", format(), 0)
        .unwrap();
    display.submitted(2, 48_000 * 8, 0).unwrap();
    let rows = display.updates(Some(0), 0, 80);
    assert!(contains(&rows, "new opening"));
    assert!(!contains(&rows, "old"));
    assert_eq!(display.visits.len(), 1);
}

#[test]
fn ffplay_estimate_starts_at_first_submission_and_cannot_outrun_submitted_audio() {
    let mut display = display();
    display
        .enqueue(
            1,
            words("[00:00]start\n[00:01]buffered\n[00:03]not yet"),
            "file",
            format(),
            0,
        )
        .unwrap();
    assert!(display.updates(None, 5_000, 80).is_empty());
    display.submitted(1, 48_000 * 8 * 2, 5_000).unwrap();
    let rows = display.updates(None, 5_000, 100);
    assert!(contains(&rows, "Timing estimated"));
    assert!(contains(&rows, "start"));
    assert!(display.updates(None, 5_000, 100).is_empty());
    let rows = display.updates(None, 10_000, 100);
    assert!(contains(&rows, "buffered"));
    assert!(!contains(&rows, "not yet"));
}

#[test]
fn blank_cues_clear_the_current_words_and_plain_text_is_a_bounded_preview() {
    let mut display = display();
    display
        .enqueue(1, words("[00:00]verse\n[00:01]"), "file", format(), 0)
        .unwrap();
    display.submitted(1, 48_000 * 8 * 2, 0).unwrap();
    assert!(contains(&display.updates(Some(0), 0, 80), "verse"));
    assert!(contains(
        &display.updates(Some(48_000), 1_000, 80),
        "no active words"
    ));
    display.reset_output();
    display
        .enqueue(
            2,
            words("one\ntwo\nthree\nfour\nfive"),
            "plain",
            format(),
            0,
        )
        .unwrap();
    display.submitted(2, 48_000 * 8, 0).unwrap();
    let rows = display.updates(Some(0), 0, 80);
    assert!(contains(&rows, "Untimed lyrics"));
    assert!(contains(&rows, "four"));
    assert!(!contains(&rows, "five"));
    assert!(contains(&rows, "…"));
    assert!(display.updates(Some(1), 0, 80).is_empty());
}

#[test]
fn lyric_and_label_controls_cannot_inject_terminal_commands_or_overflow_the_width() {
    let mut display = display();
    display.enqueue(1, words("[00:00]\u{1b}[2J malicious\n[00:00]translated\n[00:00]three\n[00:00]four\n[00:00]five"), "\u{1b}[31m bad\nlabel", format(), 0).unwrap();
    display.submitted(1, 48_000 * 8, 0).unwrap();
    let rows = display.updates(Some(0), 0, 20);
    assert!(rows.iter().all(|row| !row.contains(['\u{1b}', '\n', '\r'])));
    assert!(rows.iter().all(|row| row.chars().count() <= 20));
    assert!(contains(&rows, "translated"));
    assert!(!contains(&rows, "five"));
    assert!(contains(&rows, "…"));
    assert_eq!(cue_text("wide", 1).chars().last(), Some('…'));
}

#[test]
fn bounded_lookahead_keeps_empty_occurrences_from_starving_playback() {
    let mut display = display();
    for token in 0..MAX_PENDING {
        display.enqueue(token, None, "empty", format(), 0).unwrap();
        display.completed(token).unwrap();
    }
    assert!(!display.has_capacity());
    assert!(
        display
            .enqueue(MAX_PENDING, None, "overflow", format(), 0)
            .is_err()
    );
    assert!(display.updates(Some(0), 0, 80).is_empty());
    assert!(display.has_capacity());
    display
        .enqueue(MAX_PENDING + 1, None, "no words", format(), 0)
        .unwrap();
    display.submitted(MAX_PENDING + 1, 8, 0).unwrap();
    assert!(contains(
        &display.updates(Some(0), 0, 80),
        "No local lyrics"
    ));
}

struct ImmediatelyConsumedOutput {
    bytes: Vec<u8>,
}

impl Write for ImmediatelyConsumedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl super::super::PlaybackOutput for ImmediatelyConsumedOutput {
    fn consumed_frames(&self) -> Option<u64> {
        Some(self.bytes.len() as u64 / 8)
    }

    fn pause(&self) -> super::super::Res {
        Ok(())
    }

    fn resume(&self) -> super::super::Res {
        Ok(())
    }
}

#[test]
fn flushing_delayed_src_releases_full_lyric_lookahead_without_losing_occurrences() {
    use aede_core::playback::session::{PcmSession, SessionBlock};
    use aede_dsp::ToneControls;

    use super::super::session::{Submitted, submit_block};
    use super::super::{PlaybackClock, PlaybackEnd, PlaybackOutput};

    fn submit(
        block: SessionBlock<'_>,
        output: &mut ImmediatelyConsumedOutput,
        clock: &mut PlaybackClock,
        completions: &mut Vec<usize>,
    ) {
        let result = submit_block(block, output, None, clock, |event| {
            if let Submitted::Complete { span, .. } = event
                && span.complete
            {
                completions.push(span.token);
            }
            Ok(())
        });
        assert_eq!(result.unwrap(), PlaybackEnd::Natural);
    }

    let input = PcmFormat::new(44_100, 2).unwrap();
    let mut session = PcmSession::new(input, 48_000, ToneControls::FLAT).unwrap();
    let output_format = session.output_format();
    let mut clock = PlaybackClock::new();
    clock.lyrics = Some(display());
    let mut output = ImmediatelyConsumedOutput { bytes: Vec::new() };
    let mut completions = Vec::new();
    for token in 1..=MAX_PENDING {
        clock
            .lyrics
            .as_mut()
            .unwrap()
            .enqueue(token, None, "short", output_format, 0)
            .unwrap();
        session.begin_track(token, 0.0).unwrap();
        submit(
            session.push_source(&[0.125, -0.125]).unwrap(),
            &mut output,
            &mut clock,
            &mut completions,
        );
        submit(
            session.end_track().unwrap(),
            &mut output,
            &mut clock,
            &mut completions,
        );
    }
    assert!(
        output.bytes.is_empty(),
        "conversion still needs more source"
    );
    assert!(completions.is_empty(), "no source boundary has emerged yet");
    assert!(!clock.lyrics.as_ref().unwrap().has_capacity());

    submit(
        session.finish().unwrap(),
        &mut output,
        &mut clock,
        &mut completions,
    );
    let consumed = output.consumed_frames().unwrap();
    let expected_first_group = (MAX_PENDING as u64 * 48_000).div_ceil(44_100);
    assert_eq!(consumed, expected_first_group);
    clock
        .lyrics
        .as_mut()
        .unwrap()
        .updates(Some(consumed), 0, 80);
    assert!(clock.lyrics.as_ref().unwrap().has_capacity());
    assert!(clock.lyrics.as_ref().unwrap().visits.is_empty());

    let token = MAX_PENDING + 1;
    clock
        .lyrics
        .as_mut()
        .unwrap()
        .enqueue(token, None, "after flush", output_format, 0)
        .unwrap();
    let mut session = PcmSession::new(input, 48_000, ToneControls::FLAT).unwrap();
    session.begin_track(token, 0.0).unwrap();
    submit(
        session.push_source(&[0.125, -0.125]).unwrap(),
        &mut output,
        &mut clock,
        &mut completions,
    );
    submit(
        session.end_track().unwrap(),
        &mut output,
        &mut clock,
        &mut completions,
    );
    submit(
        session.finish().unwrap(),
        &mut output,
        &mut clock,
        &mut completions,
    );
    let consumed = output.consumed_frames().unwrap();
    assert_eq!(consumed, expected_first_group + 48_000u64.div_ceil(44_100));
    assert_eq!(completions, (1..=token).collect::<Vec<_>>());
    clock
        .lyrics
        .as_mut()
        .unwrap()
        .updates(Some(consumed), 0, 80);
    assert!(clock.lyrics.as_ref().unwrap().visits.is_empty());
    assert!(
        output
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|bytes| f32::from_le_bytes(*bytes).is_finite())
    );
}
