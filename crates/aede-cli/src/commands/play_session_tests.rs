use std::io;

use super::*;

struct PartialThenBroken {
    accepted_bytes: usize,
    first_write_bytes: usize,
}

impl Write for PartialThenBroken {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.accepted_bytes == 0 {
            let count = self.first_write_bytes.min(bytes.len());
            self.accepted_bytes = count;
            Ok(count)
        } else {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for PartialThenBroken {
    fn pause(&self) -> Res {
        Ok(())
    }

    fn resume(&self) -> Res {
        Ok(())
    }
}

struct InterruptedThenAccepted {
    interrupted: bool,
    accepted_bytes: Vec<u8>,
}

impl Write for InterruptedThenAccepted {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        self.accepted_bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for InterruptedThenAccepted {
    fn pause(&self) -> Res {
        Ok(())
    }

    fn resume(&self) -> Res {
        Ok(())
    }
}

#[test]
fn interrupted_partial_submission_keeps_accepted_frames_in_incomplete_history() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    let token = 7;
    session.begin_track(token, 0.0).expect("track");
    let (sender, receive) = mpsc::sync_channel(64);
    let mut records = PlaybackRecords::new(&sender);
    let mut clock = PlaybackClock::new();
    records
        .begin(token, 0, Path::new("partial.wav"), format, &clock)
        .expect("track record");
    clock.started = Instant::now() - Duration::from_secs(1);
    let mut output = PartialThenBroken {
        accepted_bytes: 0,
        first_write_bytes: 64 * 2 * 4,
    };
    let result = submit_block(
        session.push_source(&[0.25; 128 * 2]).expect("source"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    );
    assert!(result.is_err());
    assert_eq!(output.accepted_bytes, 64 * 2 * 4);
    let record = records.pending.get(&token).expect("pending listen");
    assert_eq!(record.frames, 64);
    assert_eq!(record.meter.snapshot().expect("meter").frames, 0);
    records.finish(false, &clock).expect("incomplete history");
    let PlaybackRecord::History(history) = receive.try_recv().expect("history event") else {
        panic!("expected a listening history event");
    };
    assert_eq!(history.path, Path::new("partial.wav"));
    assert_eq!(history.played_ms, 64 * 1000 / 48_000);
    assert!(!history.completed);
    assert!(receive.try_recv().is_err());
}

#[test]
fn an_interrupted_write_is_retried_before_accounting_the_submitted_span() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    session.begin_track(8, 0.0).expect("track");
    let mut output = InterruptedThenAccepted {
        interrupted: false,
        accepted_bytes: Vec::new(),
    };
    let mut accepted_frames = 0;
    let mut submitted_bytes = 0;
    let result = submit_block(
        session.push_source(&[0.25; 128 * 2]).expect("source"),
        &mut output,
        None,
        &mut PlaybackClock::new(),
        |event| {
            match event {
                Submitted::Bytes { token, count } => {
                    assert_eq!(token, 8);
                    submitted_bytes += count;
                }
                Submitted::Complete { span, samples } => {
                    assert_eq!(span.token, 8);
                    accepted_frames += samples.len() / 2;
                }
            }
            Ok(())
        },
    );
    assert_eq!(result.expect("write retries"), PlaybackEnd::Natural);
    assert!(output.interrupted);
    assert_eq!(output.accepted_bytes.len(), 128 * 2 * 4);
    assert_eq!(accepted_frames, 128);
    assert_eq!(submitted_bytes, 128 * 2 * 4);
}

#[test]
fn delayed_src_frames_keep_the_previous_tracks_listening_identity() {
    let input = PcmFormat::new(44_100, 2).expect("source format");
    let mut session = PcmSession::new(input, 48_000, ToneControls::FLAT).expect("session");
    let format = session.output_format();
    let (sender, receive) = mpsc::sync_channel(64);
    let mut records = PlaybackRecords::new(&sender);
    let mut clock = PlaybackClock::new();
    let mut output = Vec::new();
    session.begin_track(10, 0.0).expect("first track");
    records
        .begin(10, 0, Path::new("first.wav"), format, &clock)
        .expect("track record");
    submit_block(
        session.push_source(&[0.25; 120 * 2]).expect("first source"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    )
    .expect("first submission");
    records
        .source_finished(10, None)
        .expect("first source ended");
    submit_block(
        session.end_track().expect("first source boundary"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    )
    .expect("first track sealed");
    assert!(output.is_empty());
    assert!(receive.try_recv().is_err());

    session.begin_track(11, 0.0).expect("second track");
    records
        .begin(11, 1, Path::new("second.wav"), format, &clock)
        .expect("track record");
    submit_block(
        session
            .push_source(&[0.25; 5_000 * 2])
            .expect("second source"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    )
    .expect("second source submission");
    let PlaybackRecord::History(first) = receive.try_recv().expect("first history") else {
        panic!("expected first listening history");
    };
    let first_frames = (120u64 * 48_000).div_ceil(44_100);
    assert_eq!(first.path, Path::new("first.wav"));
    assert_eq!(first.played_ms, first_frames * 1000 / 48_000);
    assert!(first.completed);
    assert!(receive.try_recv().is_err());
    records
        .source_finished(11, None)
        .expect("second source ended");
    submit_block(
        session.end_track().expect("second source boundary"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    )
    .expect("second track sealed");
    submit_block(
        session.finish().expect("converter tail"),
        &mut output,
        None,
        &mut clock,
        |event| records.submitted(event),
    )
    .expect("tail submission");
    assert!(receive.try_recv().is_err());
    records
        .finish(true, &clock)
        .expect("drained output history");
    let PlaybackRecord::History(second) = receive.try_recv().expect("second history") else {
        panic!("expected second listening history");
    };
    let total_frames = (5_120u64 * 48_000).div_ceil(44_100);
    assert_eq!(output.len() as u64, total_frames * 2 * 4);
    assert_eq!(second.path, Path::new("second.wav"));
    assert_eq!(
        second.played_ms,
        (total_frames - first_frames) * 1000 / 48_000
    );
    assert!(second.completed);
    assert!(receive.try_recv().is_err());
}

struct DirectPcmOutput {
    maximum_bytes: usize,
    calls: Vec<(Vec<f32>, Vec<u8>, usize)>,
    accepted_samples: Vec<f32>,
}

impl Write for DirectPcmOutput {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        panic!("direct PCM output must receive the original samples");
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for DirectPcmOutput {
    fn write_pcm(
        &mut self,
        samples: &[f32],
        f32le: &[u8],
        byte_offset: usize,
    ) -> io::Result<usize> {
        self.calls
            .push((samples.to_vec(), f32le.to_vec(), byte_offset));
        assert!(byte_offset.is_multiple_of(8));
        let remaining = &samples[byte_offset / 4..];
        let count = remaining.len().min(self.maximum_bytes / 4);
        self.accepted_samples.extend_from_slice(&remaining[..count]);
        Ok(count * 4)
    }

    fn pause(&self) -> Res {
        Ok(())
    }

    fn resume(&self) -> Res {
        Ok(())
    }
}

#[test]
fn direct_pcm_submission_preserves_original_samples_and_each_tracks_progress() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let first = [0.25, -0.5].repeat(48);
    let second = [0.75, -1.0].repeat(96);
    let samples = [first.clone(), second.clone()].concat();
    // These bytes encode NaN rather than the original PCM. A direct output
    // must use the samples; a byte-only output would receive the sentinel.
    let sentinel = vec![0xff; samples.len() * 4];
    let spans = [
        TrackSpan {
            token: 21,
            samples: 0..first.len(),
            stats: aede_dsp::ProcessStats {
                sample_peak: 1.1,
                overfull_samples: 2,
            },
            complete: true,
        },
        TrackSpan {
            token: 22,
            samples: first.len()..samples.len(),
            stats: aede_dsp::ProcessStats {
                sample_peak: 1.4,
                overfull_samples: 4,
            },
            complete: true,
        },
    ];
    let (sender, receive) = mpsc::sync_channel(64);
    let mut records = PlaybackRecords::new(&sender);
    let mut clock = PlaybackClock::new();
    records
        .begin(21, 0, Path::new("first-direct.wav"), format, &clock)
        .expect("track record");
    records
        .begin(22, 1, Path::new("second-direct.wav"), format, &clock)
        .expect("track record");
    let mut output = DirectPcmOutput {
        maximum_bytes: 32,
        calls: Vec::new(),
        accepted_samples: Vec::new(),
    };
    let mut progress = BTreeMap::<usize, usize>::new();
    let mut complete = Vec::new();
    let result = submit_block(
        SessionBlock {
            samples: &samples,
            f32le: &sentinel,
            spans: &spans,
        },
        &mut output,
        None,
        &mut clock,
        |event| {
            match &event {
                Submitted::Bytes { token, count } => {
                    *progress.entry(*token).or_default() += count;
                }
                Submitted::Complete { span, samples } => {
                    complete.push((span.token, samples.to_vec(), span.stats));
                }
            }
            records.submitted(event)
        },
    );
    assert_eq!(result.expect("direct submission"), PlaybackEnd::Natural);
    assert_eq!(output.accepted_samples, samples);
    assert_eq!(progress.get(&21), Some(&(first.len() * 4)));
    assert_eq!(progress.get(&22), Some(&(second.len() * 4)));
    let first_calls = first.len() * 4 / output.maximum_bytes;
    for (call, (values, bytes, offset)) in output.calls.iter().enumerate() {
        let (expected, local_call) = if call < first_calls {
            (&first, call)
        } else {
            (&second, call - first_calls)
        };
        assert_eq!(values, expected);
        assert_eq!(bytes, &vec![0xff; expected.len() * 4]);
        assert_eq!(*offset, local_call * output.maximum_bytes);
    }
    assert_eq!(complete.len(), 2);
    assert_eq!(complete[0], (21, first, spans[0].stats));
    assert_eq!(complete[1], (22, second, spans[1].stats));
    let second_record = records.pending.get(&22).expect("second pending listen");
    assert_eq!(second_record.frames, 96);
    let meter = second_record.meter.sample_peak_snapshot();
    assert_eq!(meter.frames, 96);
    assert_eq!(meter.output_sample_peak, 1.0);
    assert_eq!(meter.pre_guard_sample_peak, 1.4);
    assert_eq!(meter.guarded_samples, 4);
    let PlaybackRecord::History(first_history) = receive.try_recv().expect("first history") else {
        panic!("expected first listening history");
    };
    assert_eq!(first_history.path, Path::new("first-direct.wav"));
    assert_eq!(first_history.played_ms, 1);
    assert!(first_history.completed);
    records
        .finish(true, &clock)
        .expect("complete second history");
    let PlaybackRecord::History(second_history) = receive.try_recv().expect("second history")
    else {
        panic!("expected second listening history");
    };
    assert_eq!(second_history.path, Path::new("second-direct.wav"));
    assert_eq!(second_history.played_ms, 2);
    assert!(second_history.completed);
    assert!(receive.try_recv().is_err());
}

struct BytePrefixOutput {
    prefixes: std::collections::VecDeque<usize>,
    fail_after_prefixes: bool,
    calls: Vec<Vec<u8>>,
    accepted_bytes: Vec<u8>,
}

impl Write for BytePrefixOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls.push(bytes.to_vec());
        let count = match self.prefixes.pop_front() {
            Some(prefix) => prefix.min(bytes.len()),
            None if self.fail_after_prefixes => {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            None => bytes.len(),
        };
        self.accepted_bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for BytePrefixOutput {
    fn pause(&self) -> Res {
        Ok(())
    }

    fn resume(&self) -> Res {
        Ok(())
    }
}

#[test]
fn byte_fallback_preserves_pcm_after_writes_ending_inside_samples() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    session.begin_track(30, 0.0).expect("track");
    let samples = [0.25, -0.5].repeat(128);
    let block = session.push_source(&samples).expect("source");
    let expected = block.f32le.to_vec();
    let mut output = BytePrefixOutput {
        prefixes: [1, 2, 5, 9].into(),
        fail_after_prefixes: false,
        calls: Vec::new(),
        accepted_bytes: Vec::new(),
    };
    let mut submitted_bytes = 0;
    let mut complete_count = 0;
    let result = submit_block(
        block,
        &mut output,
        None,
        &mut PlaybackClock::new(),
        |event| {
            match event {
                Submitted::Bytes { token, count } => {
                    assert_eq!(token, 30);
                    submitted_bytes += count;
                }
                Submitted::Complete {
                    span,
                    samples: accepted,
                } => {
                    assert_eq!(span.token, 30);
                    assert_eq!(accepted, samples);
                    complete_count += 1;
                }
            }
            Ok(())
        },
    );
    assert_eq!(
        result.expect("byte fallback submission"),
        PlaybackEnd::Natural
    );
    assert_eq!(output.accepted_bytes, expected);
    assert_eq!(submitted_bytes, expected.len());
    assert_eq!(complete_count, 1);
    assert_eq!(output.calls.len(), 5);
    for (call, offset) in output.calls.iter().zip([0, 1, 3, 8, 17]) {
        assert_eq!(call, &expected[offset..]);
    }
}

#[test]
fn byte_fallback_failure_keeps_only_complete_frames_of_the_exact_accepted_prefix() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    session.begin_track(31, 0.0).expect("track");
    let block = session
        .push_source(&[0.25, -0.5].repeat(128))
        .expect("source");
    let expected = block.f32le.to_vec();
    let (sender, receive) = mpsc::sync_channel(64);
    let mut records = PlaybackRecords::new(&sender);
    let mut clock = PlaybackClock::new();
    records
        .begin(31, 0, Path::new("odd-prefix.wav"), format, &clock)
        .expect("track record");
    clock.started = Instant::now() - Duration::from_secs(1);
    let mut output = BytePrefixOutput {
        prefixes: [3, 511, 5].into(),
        fail_after_prefixes: true,
        calls: Vec::new(),
        accepted_bytes: Vec::new(),
    };
    let result = submit_block(block, &mut output, None, &mut clock, |event| {
        records.submitted(event)
    });
    assert!(result.is_err());
    assert_eq!(output.accepted_bytes, expected[..519]);
    assert_eq!(output.calls.len(), 4);
    for (call, offset) in output.calls.iter().zip([0, 3, 514, 519]) {
        assert_eq!(call, &expected[offset..]);
    }
    let record = records.pending.get(&31).expect("pending listen");
    assert_eq!(record.submitted_bytes, 519);
    assert_eq!(record.frames, 64);
    assert_eq!(record.meter.sample_peak_snapshot().frames, 0);
    records.finish(false, &clock).expect("incomplete history");
    let PlaybackRecord::History(history) = receive.try_recv().expect("history") else {
        panic!("expected listening history");
    };
    assert_eq!(history.path, Path::new("odd-prefix.wav"));
    assert_eq!(history.played_ms, 1);
    assert!(!history.completed);
    assert!(receive.try_recv().is_err());
}

#[test]
fn repeated_tiny_seek_segments_are_rounded_once_when_the_listen_is_saved() {
    let (sender, receive) = mpsc::sync_channel(64);
    let mut records = PlaybackRecords::new(&sender);
    let format = PcmFormat::new(8_000, 1).unwrap();
    let mut clock = PlaybackClock::new();
    for token in 1..=8 {
        records
            .begin(token, 0, Path::new("tiny.wav"), format, &clock)
            .unwrap();
        clock.started -= Duration::from_millis(1);
        records
            .submitted(Submitted::Bytes { token, count: 4 })
            .unwrap();
        records.carry_seek(0, &clock).unwrap();
    }
    records.finish_resume().unwrap();
    let PlaybackRecord::History(listen) = receive
        .try_recv()
        .expect("eight frames are one millisecond")
    else {
        panic!("expected listening history");
    };
    assert_eq!(listen.played_ms, 1);
    assert!(!listen.completed);
    assert!(receive.try_recv().is_err());
}
