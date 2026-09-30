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
    let (sender, receive) = mpsc::channel();
    let mut records = PlaybackRecords::new(&sender, 1);
    let mut clock = PlaybackClock::new();
    records.begin(token, 0, Path::new("partial.wav"), format, &clock);
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
    let (sender, receive) = mpsc::channel();
    let mut records = PlaybackRecords::new(&sender, 2);
    let mut clock = PlaybackClock::new();
    let mut output = Vec::new();
    session.begin_track(10, 0.0).expect("first track");
    records.begin(10, 0, Path::new("first.wav"), format, &clock);
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
    records.begin(11, 1, Path::new("second.wav"), format, &clock);
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
