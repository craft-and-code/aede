use std::cell::Cell;
use std::io;

use aede_core::playback::decoder::IntegerFileDecoder;
use aede_core::playback::exact_output::ExactSampleRepresentation;

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "../aede-core/tests/playback_fixtures/flac"
        } else {
            "../aede-core/tests/fixtures"
        })
        .join(name)
}

fn reference(path: &Path) -> Vec<i32> {
    let mut decoder = IntegerFileDecoder::open(path).unwrap();
    let channels = usize::from(decoder.format().channels());
    let mut buffer = vec![0; 31 * channels];
    let mut samples = Vec::new();
    loop {
        let frames = decoder.read_frames(&mut buffer).unwrap();
        if frames == 0 {
            return samples;
        }
        samples.extend_from_slice(&buffer[..frames * channels]);
    }
}

#[derive(Default)]
pub(super) struct EligibleSink {
    pub(super) source: Option<IntegerPcmFormat>,
    pub(super) samples: Vec<i32>,
    stream_frames: u64,
    consumed: Cell<u64>,
    pub(super) opens: usize,
    closed: bool,
    pub(super) finished: usize,
    pub(super) aborted: bool,
    fail_drain: bool,
    consume_before_failure: u64,
    maximum_frames: usize,
    incomplete_frame: bool,
    route_checks: usize,
    fail_route_check: Option<usize>,
    refuse_bits: Option<u32>,
    pub(super) consume_writes: bool,
}

impl EligibleSink {
    pub(super) fn with_maximum_frames(maximum_frames: usize) -> Self {
        Self {
            maximum_frames,
            ..Default::default()
        }
    }
}

impl Write for EligibleSink {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        panic!("strict sink cannot receive processed bytes")
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for EligibleSink {
    fn requires_exact(&self) -> bool {
        true
    }
    fn consumed_frames(&self) -> Option<u64> {
        Some(self.consumed.get())
    }
    fn write_exact(&mut self, values: &[i32], bytes: &[u8], offset: usize) -> io::Result<usize> {
        let format = self.source.unwrap();
        let channels = usize::from(format.channels());
        assert!(offset.is_multiple_of(channels * 4));
        assert_eq!(bytes.len(), values.len() * 4);
        let remaining = &values[offset / 4..];
        let count = if self.incomplete_frame {
            1
        } else if self.maximum_frames == 0 {
            remaining.len()
        } else {
            remaining.len().min(self.maximum_frames * channels)
        };
        self.samples.extend_from_slice(&remaining[..count]);
        self.stream_frames += (count / channels) as u64;
        if self.consume_writes {
            self.consumed.set(self.stream_frames);
        }
        Ok(count * 4)
    }
    fn pause(&self) -> Res {
        Ok(())
    }
    fn resume(&self) -> Res {
        Ok(())
    }
}

impl SessionOutput for EligibleSink {
    fn needs_reopen(&mut self, _: PcmFormat) -> Result<bool, Box<dyn Error>> {
        panic!("processed preparation")
    }
    fn prepare(&mut self, _: PcmFormat) -> Result<PcmFormat, Box<dyn Error>> {
        panic!("processed preparation")
    }
    fn exact_needs_reopen(&mut self, format: IntegerPcmFormat) -> Result<bool, Box<dyn Error>> {
        self.route_checks += 1;
        if self.fail_route_check == Some(self.route_checks) {
            self.consumed
                .set(self.consume_before_failure.min(self.stream_frames));
            return Err("selected native device ID changed".into());
        }
        if self.refuse_bits == Some(format.bits_per_sample()) {
            return Err("native route does not support the next source precision".into());
        }
        Ok(self.source.is_some_and(|held| held != format))
    }
    fn prepare_exact(
        &mut self,
        format: IntegerPcmFormat,
    ) -> Result<ExactOutputFormat, Box<dyn Error>> {
        if self.source != Some(format) || self.closed {
            assert!(
                self.source.is_none() || self.closed,
                "format changes must drain"
            );
            self.source = Some(format);
            self.opens += 1;
            self.stream_frames = 0;
            self.consumed.set(0);
            self.closed = false;
        }
        Ok(ExactOutputFormat::new(
            format.sample_rate(),
            format.layout(),
            ExactSampleRepresentation::Signed32Le,
            Some(24),
        )?)
    }
    fn ensure_started(&mut self) -> Res {
        self.consumed.set(self.stream_frames);
        Ok(())
    }
    fn close_input(&mut self) {
        self.closed = true;
        self.consumed.set(if self.fail_drain {
            self.consume_before_failure.min(self.stream_frames)
        } else {
            self.stream_frames
        });
    }
    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>> {
        if self.fail_drain {
            return Err("simulated strict host failure".into());
        }
        Ok(output::DrainProgress {
            drained: self.closed,
            consumed_frames: Some(self.consumed.get()),
        })
    }
    fn host_tail(&self) -> Duration {
        Duration::ZERO
    }
    fn finish(&mut self) -> Res {
        assert_eq!(self.consumed.get(), self.stream_frames);
        self.finished += 1;
        Ok(())
    }
    fn abort(&mut self) -> Res {
        self.aborted = true;
        self.consumed.set(0);
        Ok(())
    }
}

pub(super) fn drive<O: SessionOutput>(
    paths: &[PathBuf],
    sink: &mut O,
    options: crate::args::PlaybackOptions,
) -> (Res, Vec<HistoryItem>) {
    let directory = Path::new("unused-strict-normalization-cache");
    let mut normalization =
        ReadyNormalization::new(paths, false, None, directory, NormalizationMode::Off).unwrap();
    let mut order = PlaybackOrder::new(paths, None, &options).unwrap();
    let (sender, receiver) = mpsc::sync_channel(paths.len().max(64) + 1);
    let result = play_selection_with(
        paths,
        None,
        &mut normalization,
        None,
        sink,
        &sender,
        SelectionSettings {
            normalization_mode: NormalizationMode::Off,
            tone: ToneControls::FLAT,
            order: &mut order,
            options,
        },
    );
    drop(sender);
    let records = receiver
        .into_iter()
        .map(|record| match record {
            PlaybackRecord::History(history) => history,
            PlaybackRecord::Loudness(_) => panic!("strict playback must not cache loudness"),
        })
        .collect();
    (result, records)
}

#[test]
fn the_single_driver_preserves_compatible_occurrences_and_partial_writes() {
    let path = fixture("playback-real24.flac");
    let expected = reference(&path).repeat(3);
    let mut sink = EligibleSink {
        maximum_frames: 37,
        ..Default::default()
    };
    let (result, records) = drive(
        &[path.clone(), path.clone(), path.clone()],
        &mut sink,
        Default::default(),
    );
    result.unwrap();
    assert_eq!(sink.samples, expected);
    assert_eq!(sink.opens, 1);
    assert_eq!(sink.finished, 1);
    assert_eq!(records.len(), 3);
    assert!(
        records
            .iter()
            .all(|record| record.path == path && record.completed && record.played_ms > 0)
    );
    assert!(!sink.aborted);
}

#[test]
fn the_driver_drains_previous_occurrences_before_reopening_a_different_format() {
    let paths = [fixture("playback-real24.flac"), fixture("track.wav")];
    let expected = [reference(&paths[0]), reference(&paths[1])].concat();
    let mut sink = EligibleSink::default();
    let (result, records) = drive(&paths, &mut sink, Default::default());
    result.unwrap();
    assert_eq!(sink.samples, expected);
    assert_eq!(sink.opens, 2);
    assert_eq!(sink.finished, 2);
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record.completed));
}

#[test]
fn an_unsupported_later_source_is_refused_after_valid_prior_audio_drains() {
    let paths = [fixture("track.wav"), fixture("track.mp3")];
    let expected = reference(&paths[0]);
    let mut sink = EligibleSink::default();
    let (result, records) = drive(&paths, &mut sink, Default::default());
    assert!(result.is_err());
    assert_eq!(sink.samples, expected);
    assert_eq!(sink.finished, 1);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, paths[0]);
    assert!(records[0].completed);
}

#[test]
fn strict_seek_keeps_the_exact_suffix_and_never_claims_a_complete_listen() {
    let path = fixture("track.wav");
    let mut sink = EligibleSink::default();
    let (result, records) = drive(
        std::slice::from_ref(&path),
        &mut sink,
        crate::args::PlaybackOptions {
            seek_ms: 100,
            ..Default::default()
        },
    );
    result.unwrap();
    assert_eq!(sink.samples, reference(&path)[4410..]);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].played_ms, 150);
    assert!(!records[0].completed);
}

#[test]
fn strict_output_failure_records_only_consumed_audio_before_abort_resets_the_clock() {
    let path = fixture("track.wav");
    let mut sink = EligibleSink {
        fail_drain: true,
        consume_before_failure: 4410,
        ..Default::default()
    };
    let (result, records) = drive(&[path], &mut sink, Default::default());
    assert!(result.is_err());
    assert!(sink.aborted);
    assert_eq!(sink.consumed.get(), 0);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].played_ms, 100);
    assert!(!records[0].completed);
}

#[test]
fn strict_effect_conflicts_are_refused_before_opening_a_source_or_sink() {
    for (mode, tone) in [
        (NormalizationMode::Track, ToneControls::FLAT),
        (NormalizationMode::Off, ToneControls::new(1.0, 0.0).unwrap()),
    ] {
        let paths = [PathBuf::from("missing.flac")];
        let options = crate::args::PlaybackOptions::default();
        let mut normalization = ReadyNormalization::new(
            &paths,
            false,
            None,
            Path::new("unused-strict-normalization-cache"),
            NormalizationMode::Off,
        )
        .unwrap();
        let mut order = PlaybackOrder::new(&paths, None, &options).unwrap();
        let (sender, receiver) = mpsc::sync_channel(64);
        let mut sink = EligibleSink::default();
        let result = play_selection_with(
            &paths,
            None,
            &mut normalization,
            None,
            &mut sink,
            &sender,
            SelectionSettings {
                normalization_mode: mode,
                tone,
                order: &mut order,
                options,
            },
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("normalization off and flat tone")
        );
        assert_eq!(sink.opens, 0);
        assert!(sink.samples.is_empty());
        assert!(receiver.try_recv().is_err());
    }
}

#[test]
fn strict_lookahead_starts_buffered_output_without_restarting_compatible_tracks() {
    let path = fixture("track.flac");
    let paths = vec![path.clone(); 65];
    let mut sink = EligibleSink::default();
    let (result, records) = drive(&paths, &mut sink, Default::default());
    result.unwrap();
    assert_eq!(sink.samples, reference(&path).repeat(65));
    assert_eq!(sink.opens, 1);
    assert_eq!(sink.finished, 1);
    assert_eq!(records.len(), 65);
    assert!(records.iter().all(|record| record.completed));
}

#[derive(Default)]
struct ProcessedSink {
    write_delay: Duration,
    bytes: Vec<u8>,
    format: Option<PcmFormat>,
    closed: bool,
}

impl Write for ProcessedSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        if !self.write_delay.is_zero() {
            std::thread::sleep(self.write_delay);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for ProcessedSink {
    fn pause(&self) -> Res {
        Ok(())
    }
    fn resume(&self) -> Res {
        Ok(())
    }
}

impl SessionOutput for ProcessedSink {
    fn needs_reopen(&mut self, format: PcmFormat) -> Result<bool, Box<dyn Error>> {
        Ok(self.format.is_some_and(|held| held != format))
    }
    fn prepare(&mut self, format: PcmFormat) -> Result<PcmFormat, Box<dyn Error>> {
        self.format = Some(format);
        self.closed = false;
        Ok(format)
    }
    fn close_input(&mut self) {
        self.closed = true;
    }
    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>> {
        Ok(output::DrainProgress {
            drained: self.closed,
            consumed_frames: None,
        })
    }
    fn host_tail(&self) -> Duration {
        Duration::ZERO
    }
    fn finish(&mut self) -> Res {
        Ok(())
    }
}

#[test]
fn the_same_driver_preserves_existing_processed_playback_and_history() {
    let path = fixture("playback-stereo.flac");
    let mut decoder = aede_core::playback::decoder::FileDecoder::open(&path).unwrap();
    let mut raw = vec![0.0; usize::from(decoder.channels()) * 31];
    let mut expected = Vec::new();
    loop {
        let frames = decoder.read_frames(&mut raw).unwrap();
        if frames == 0 {
            break;
        }
        for sample in &raw[..frames * usize::from(decoder.channels())] {
            expected.extend_from_slice(&sample.to_le_bytes());
        }
    }
    let paths = [path.clone(), path.clone()];
    let options = crate::args::PlaybackOptions::default();
    let mut normalization = ReadyNormalization::new(
        &paths,
        false,
        None,
        Path::new("unused-strict-normalization-cache"),
        NormalizationMode::Off,
    )
    .unwrap();
    let mut order = PlaybackOrder::new(&paths, None, &options).unwrap();
    let (sender, receiver) = mpsc::sync_channel(64);
    let mut sink = ProcessedSink::default();
    play_selection_with(
        &paths,
        None,
        &mut normalization,
        None,
        &mut sink,
        &sender,
        SelectionSettings {
            normalization_mode: NormalizationMode::Off,
            tone: ToneControls::FLAT,
            order: &mut order,
            options,
        },
    )
    .unwrap();
    assert_eq!(sink.bytes, expected.repeat(2));
    drop(sender);
    let histories = receiver.into_iter().collect::<Vec<_>>();
    assert_eq!(histories.len(), 2);
    assert!(histories.iter().all(|record| matches!(record, PlaybackRecord::History(history) if history.path == path && history.completed && history.played_ms > 0)));
}

#[test]
fn the_driver_refuses_a_strict_sink_accepting_only_one_channel_of_a_frame() {
    let path = fixture("playback-stereo.flac");
    let mut sink = EligibleSink {
        incomplete_frame: true,
        ..Default::default()
    };
    let (result, records) = drive(&[path], &mut sink, Default::default());
    assert!(result.unwrap_err().to_string().contains("incomplete frame"));
    assert!(sink.aborted);
    assert_eq!(sink.samples.len(), 1);
    assert!(records.is_empty());
}

#[test]
fn compatible_next_occurrences_revalidate_the_selected_route_before_submission() {
    let path = fixture("track.wav");
    let mut sink = EligibleSink {
        fail_route_check: Some(2),
        consume_before_failure: 4410,
        ..Default::default()
    };
    let (result, records) = drive(&[path.clone(), path.clone()], &mut sink, Default::default());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("device ID changed")
    );
    assert_eq!(sink.route_checks, 2);
    assert_eq!(sink.samples, reference(&path));
    assert!(sink.aborted);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].played_ms, 100);
    assert!(!records[0].completed);
}

#[test]
fn a_later_route_refusal_drains_valid_prior_pcm_without_skipping_or_fallback() {
    let paths = [fixture("playback-real24.flac"), fixture("track.wav")];
    let mut sink = EligibleSink {
        refuse_bits: Some(16),
        ..Default::default()
    };
    let (result, records) = drive(&paths, &mut sink, Default::default());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("next source precision")
    );
    assert_eq!(sink.samples, reference(&paths[0]));
    assert_eq!(sink.opens, 1);
    assert_eq!(sink.finished, 1);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].path, paths[0]);
    assert!(records[0].completed);
}

#[test]
fn strict_failure_retains_an_earlier_completed_visit_and_only_the_later_consumed_prefix() {
    let path = fixture("track.wav");
    let mut sink = EligibleSink {
        fail_drain: true,
        consume_before_failure: 11025 + 4410,
        ..Default::default()
    };
    let (result, records) = drive(&[path.clone(), path], &mut sink, Default::default());
    assert!(result.is_err());
    assert!(sink.aborted);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].played_ms, 250);
    assert!(records[0].completed);
    assert_eq!(records[1].played_ms, 100);
    assert!(!records[1].completed);
}

struct DigestFiles {
    root: PathBuf,
    good: PathBuf,
    bad: PathBuf,
    later: PathBuf,
    data: PathBuf,
}

impl DigestFiles {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aede_policy_digest_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let good = root.join("good.flac");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/normalization.flac"),
            &good,
        )
        .unwrap();
        let mut bytes = std::fs::read(&good).unwrap();
        assert_eq!(&bytes[..4], b"fLaC");
        assert_eq!(bytes[4] & 0x7f, 0);
        assert_eq!(&bytes[5..8], &[0, 0, 34]);
        assert!(bytes[26..42].iter().any(|&byte| byte != 0));
        // Preserve every encoded frame and its CRC; only the reference digest
        // changes. DSP and gain must never obscure this source verification.
        bytes[26] ^= 1;
        let bad = root.join("bad.flac");
        std::fs::write(&bad, bytes).unwrap();
        let later = root.join("later.flac");
        std::fs::copy(&good, &later).unwrap();
        let data = root.join("data");
        Self {
            root,
            good,
            bad,
            later,
            data,
        }
    }
}

impl Drop for DigestFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn assert_digest_failure<O: SessionOutput>(
    files: &DigestFiles,
    sink: &mut O,
    normalization_mode: NormalizationMode,
    tone: ToneControls,
) {
    let paths = [files.good.clone(), files.bad.clone(), files.later.clone()];
    let options = crate::args::PlaybackOptions::default();
    let mut normalization =
        ReadyNormalization::new(&paths, false, None, &files.data, normalization_mode).unwrap();
    let mut order = PlaybackOrder::new(&paths, None, &options).unwrap();
    let (sender, receiver) = mpsc::sync_channel(64);
    let result = play_selection_with(
        &paths,
        None,
        &mut normalization,
        None,
        sink,
        &sender,
        SelectionSettings {
            normalization_mode,
            tone,
            order: &mut order,
            options,
        },
    );
    assert!(result.unwrap_err().to_string().contains("MD5"));
    drop(sender);
    let histories = receiver
        .into_iter()
        .map(|record| match record {
            PlaybackRecord::History(history) => history,
            PlaybackRecord::Loudness(_) => panic!(
                "matching gain tags and an invalid source must not produce loudness captures"
            ),
        })
        .collect::<Vec<_>>();
    assert_eq!(histories.len(), 2);
    assert_eq!(histories[0].path, files.good);
    assert!(histories[0].completed);
    assert_eq!(histories[0].played_ms, 250);
    assert_eq!(histories[1].path, files.bad);
    assert!(!histories[1].completed);
    assert!(histories[1].played_ms > 0 && histories[1].played_ms <= 250);
    assert_eq!(
        order.current(),
        Some(1),
        "the later source was never visited"
    );
    assert!(
        aede_core::conclusions::load(&aede_core::conclusions::conclusions_path(&files.data))
            .unwrap()
            .is_none()
    );
}

#[test]
fn all_three_driver_paths_verify_original_flac_md5_before_accepting_source_completion() {
    let files = DigestFiles::new();
    let mut without_effects = ProcessedSink {
        write_delay: Duration::from_millis(1),
        ..Default::default()
    };
    assert_digest_failure(
        &files,
        &mut without_effects,
        NormalizationMode::Off,
        ToneControls::FLAT,
    );
    let mut dsp = ProcessedSink {
        write_delay: Duration::from_millis(1),
        ..Default::default()
    };
    assert_digest_failure(
        &files,
        &mut dsp,
        NormalizationMode::Track,
        ToneControls::new(3.0, -2.0).unwrap(),
    );
    assert!(!without_effects.bytes.is_empty());
    assert!(!dsp.bytes.is_empty());
    assert_ne!(
        without_effects.bytes, dsp.bytes,
        "nonzero metadata gain and tone actually ran"
    );
    let mut strict = EligibleSink {
        consume_writes: true,
        ..Default::default()
    };
    assert_digest_failure(
        &files,
        &mut strict,
        NormalizationMode::Off,
        ToneControls::FLAT,
    );
    assert!(strict.aborted);
    let expected = reference(&files.good);
    assert_eq!(&strict.samples[..expected.len()], expected);
    assert!(strict.samples.len() > expected.len());
    assert!(strict.samples.len() <= expected.len() * 2);
}
