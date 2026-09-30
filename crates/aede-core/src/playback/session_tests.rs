use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aede_dsp::{Dsp, PcmFormat, RateConverter, ToneControls};

use super::{PcmSession, SessionBlock};
use crate::playback::stream::PcmTrack;

struct WavFixtures {
    directory: PathBuf,
}

impl WavFixtures {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let directory = std::env::temp_dir().join(format!(
                "aede_session_wavs_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => return Self { directory },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("fixture directory: {error}"),
            }
        }
    }

    fn write(&self, name: &str, rate: u32, pcm: &[i16]) -> PathBuf {
        let data_bytes = pcm.len() as u32 * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 4).to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_bytes.to_le_bytes());
        for sample in pcm {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let path = self.directory.join(name);
        std::fs::write(&path, bytes).expect("WAV fixture");
        path
    }
}

impl Drop for WavFixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[derive(Default)]
struct Received {
    samples: Vec<f32>,
    track_frames: BTreeMap<usize, usize>,
    completed: Vec<usize>,
}

impl Received {
    fn collect(&mut self, block: SessionBlock<'_>, channels: usize) {
        assert_eq!(block.f32le.len(), block.samples.len() * 4);
        for (sample, encoded) in block.samples.iter().zip(block.f32le.as_chunks::<4>().0) {
            assert_eq!(*sample, f32::from_le_bytes(*encoded));
        }
        let mut next = 0;
        for span in block.spans {
            assert_eq!(span.samples.start, next);
            assert!(span.samples.end >= next);
            assert_eq!(span.samples.start % channels, 0);
            assert_eq!(span.samples.end % channels, 0);
            *self.track_frames.entry(span.token).or_default() += span.samples.len() / channels;
            if span.complete {
                self.completed.push(span.token);
            }
            next = span.samples.end;
        }
        assert_eq!(next, block.samples.len());
        self.samples.extend_from_slice(block.samples);
    }
}

fn render(
    format: PcmFormat,
    output_rate: u32,
    tone: ToneControls,
    tracks: &[(usize, &[f32], f32)],
) -> Received {
    let mut session = PcmSession::new(format, output_rate, tone).expect("session");
    let channels = usize::from(format.channels());
    let mut received = Received::default();
    for &(token, samples, gain) in tracks {
        session.begin_track(token, gain).expect("begin track");
        for chunk in samples.chunks(197 * channels) {
            received.collect(session.push_source(chunk).expect("source"), channels);
        }
        received.collect(session.end_track().expect("track boundary"), channels);
    }
    received.collect(session.finish().expect("session tail"), channels);
    let repeated = session.finish().expect("repeated finish");
    assert!(repeated.samples.is_empty());
    assert!(repeated.f32le.is_empty());
    assert!(repeated.spans.is_empty());
    received
}

fn decode_observed_session(
    paths: &[&Path],
    format: PcmFormat,
    tone: ToneControls,
) -> (Received, Vec<Vec<f32>>) {
    let mut session = PcmSession::new(format, 48_000, tone).expect("session");
    let mut received = Received::default();
    let mut observed = vec![Vec::new(); paths.len()];
    for (token, path) in paths.iter().enumerate() {
        let mut track = PcmTrack::open(path).expect("WAV opens");
        assert_eq!(track.format(), format);
        session
            .begin_track(token, -3.0 + tone.safe_preamp_db())
            .expect("begin track");
        let mut observation_calls = 0;
        let mut decoded_blocks = 0;
        while let Some(block) = track
            .read_block_observed(
                |source_format, samples| {
                    assert_eq!(source_format, format);
                    assert!(!samples.is_empty());
                    observation_calls += 1;
                    observed[token].extend_from_slice(samples);
                },
                |_| Ok::<(), Infallible>(()),
            )
            .expect("decode source")
        {
            decoded_blocks += 1;
            received.collect(
                session.push_source(block.samples).expect("continuous DSP"),
                2,
            );
        }
        assert_eq!(observation_calls, decoded_blocks);
        received.collect(session.end_track().expect("track boundary"), 2);
    }
    received.collect(session.finish().expect("group tail"), 2);
    (received, observed)
}

#[test]
fn compatible_tracks_share_exact_continuous_src_and_equalizer_output() {
    let tone = ToneControls::new(6.0, -3.0).expect("tone");
    let gain = tone.safe_preamp_db();
    let pcm = (0..10_001)
        .flat_map(|index| {
            let sample = (index as f32 * 0.003).sin() * 0.3;
            [sample, -sample * 0.7]
        })
        .collect::<Vec<_>>();
    for (source_rate, output_rate) in [(44_100, 48_000), (44_101, 48_000), (48_000, 44_100)] {
        let format = PcmFormat::new(source_rate, 2).expect("format");
        let whole = render(format, output_rate, tone, &[(10, &pcm, gain)]);
        let split = render(
            format,
            output_rate,
            tone,
            &[
                (0, &pcm[..2_002], gain),
                (1, &pcm[2_002..6_666], gain),
                (2, &pcm[6_666..], gain),
            ],
        );
        assert_eq!(whole.samples, split.samples);
        assert_eq!(split.completed, [0, 1, 2]);
        assert_eq!(whole.completed, [10]);
        assert_eq!(
            whole.samples.len(),
            (10_001u64 * u64::from(output_rate)).div_ceil(u64::from(source_rate)) as usize * 2,
        );
    }
}

#[test]
fn pending_src_frames_use_their_own_tracks_gain_after_conversion() {
    let format = PcmFormat::new(44_100, 1).expect("format");
    let tone = ToneControls::new(3.0, -4.0).expect("tone");
    let pcm = (0..5_007)
        .map(|index| (index as f32 * 0.071).sin() * 0.2)
        .collect::<Vec<_>>();
    let first_frames = 1_001;
    let actual = render(
        format,
        48_000,
        tone,
        &[
            (7, &pcm[..first_frames], -9.0),
            (8, &pcm[first_frames..], -3.0),
        ],
    );
    let mut converter = RateConverter::new(format, 48_000).expect("converter");
    let mut expected = converter
        .push(&pcm, true)
        .expect("complete conversion")
        .to_vec();
    let first_output = (first_frames as u64 * 48_000).div_ceil(44_100) as usize;
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 1).expect("output format"));
    dsp.set_tone(tone).expect("equalizer");
    dsp.set_gain_db(-9.0, 0).expect("first gain");
    dsp.process_for_output(&mut expected[..first_output])
        .expect("first track processing");
    dsp.set_gain_db(-3.0, 0).expect("second gain");
    dsp.process_for_output(&mut expected[first_output..])
        .expect("second track processing");
    assert_eq!(actual.samples, expected);
    assert_eq!(actual.track_frames[&7], first_output);
    assert_eq!(actual.track_frames[&8], expected.len() - first_output);
    assert_eq!(actual.completed, [7, 8]);
}

#[test]
fn very_short_tracks_share_fractional_frame_counts_and_complete_once() {
    let format = PcmFormat::new(48_000, 1).expect("format");
    let lengths = [1, 0, 2, 1, 17, 3];
    let pcm = lengths
        .iter()
        .map(|&count| vec![0.2; count])
        .collect::<Vec<_>>();
    let tracks = pcm
        .iter()
        .enumerate()
        .map(|(token, pcm)| (token, pcm.as_slice(), 0.0))
        .collect::<Vec<_>>();
    let actual = render(format, 8_000, ToneControls::FLAT, &tracks);
    let mut source_frames = 0u64;
    let mut prior_boundary = 0;
    for (token, count) in lengths.into_iter().enumerate() {
        source_frames += count as u64;
        let boundary = (source_frames * 8_000).div_ceil(48_000) as usize;
        assert_eq!(actual.track_frames[&token], boundary - prior_boundary);
        prior_boundary = boundary;
    }
    assert_eq!(actual.samples.len(), 4);
    assert_eq!(actual.completed, [0, 1, 2, 3, 4, 5]);
}

#[test]
fn sealing_a_track_does_not_flush_src_or_complete_unemitted_frames() {
    let format = PcmFormat::new(44_100, 1).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    session.begin_track(0, 0.0).expect("begin");
    let first = session.push_source(&[0.2; 120]).expect("short source");
    assert!(first.samples.is_empty());
    assert!(first.spans.is_empty());
    let boundary = session.end_track().expect("boundary");
    assert!(boundary.samples.is_empty());
    assert!(boundary.spans.is_empty());
    session.begin_track(1, 0.0).expect("next");
    let mut received = Received::default();
    received.collect(session.push_source(&[0.2; 5_000]).expect("next source"), 1);
    received.collect(session.end_track().expect("next boundary"), 1);
    received.collect(session.finish().expect("group tail"), 1);
    assert_eq!(received.completed, [0, 1]);
    assert_eq!(
        received.track_frames[&0],
        (120u64 * 48_000).div_ceil(44_100) as usize
    );
    assert_eq!(
        received.samples.len(),
        (5_120u64 * 48_000).div_ceil(44_100) as usize
    );
}

#[test]
fn exact_rate_tracks_report_guard_stats_and_zero_frame_completions() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    assert_eq!(session.input_format(), format);
    assert_eq!(session.output_format(), format);
    assert!(session.compatible(format, 48_000, ToneControls::FLAT));
    assert!(!session.compatible(format, 44_100, ToneControls::FLAT));
    assert!(!session.compatible(
        PcmFormat::new(48_000, 1).expect("mono"),
        48_000,
        ToneControls::FLAT
    ));
    assert!(!session.compatible(format, 48_000, ToneControls::new(1.0, 0.0).expect("tone")));
    session.begin_track(4, 6.0).expect("first gain");
    let block = session.push_source(&[0.8, -0.8]).expect("first source");
    assert_eq!(block.samples, [1.0, -1.0]);
    assert_eq!(block.spans.len(), 1);
    assert_eq!(block.spans[0].token, 4);
    assert_eq!(block.spans[0].samples, 0..2);
    assert!(!block.spans[0].complete);
    assert_eq!(block.spans[0].stats.overfull_samples, 2);
    assert!(block.spans[0].stats.sample_peak > 1.5);
    let end = session.end_track().expect("first complete");
    assert!(end.samples.is_empty());
    assert_eq!(end.spans.len(), 1);
    assert_eq!(end.spans[0].samples, 0..0);
    assert!(end.spans[0].complete);
    session.begin_track(5, 0.0).expect("next gain");
    let block = session.push_source(&[0.2, -0.3]).expect("next source");
    assert_eq!(block.samples, [0.2, -0.3]);
    assert_eq!(block.spans[0].stats.overfull_samples, 0);
    session.end_track().expect("next complete");
    assert!(session.finish().expect("finish").samples.is_empty());
}

#[test]
fn invalid_input_and_lifecycle_calls_do_not_consume_the_session() {
    let format = PcmFormat::new(44_100, 2).expect("format");
    let mut session = PcmSession::new(format, 48_000, ToneControls::FLAT).expect("session");
    assert!(session.push_source(&[0.1, 0.2]).is_err());
    assert!(session.end_track().is_err());
    assert!(session.begin_track(0, f32::NAN).is_err());
    session.begin_track(0, 0.0).expect("begin");
    assert!(session.begin_track(1, 0.0).is_err());
    assert!(session.finish().is_err());
    assert!(session.push_source(&[0.1]).is_err());
    assert!(session.push_source(&[f32::NAN, 0.0]).is_err());
    let mut received = Received::default();
    received.collect(
        session
            .push_source(&[0.1, 0.2, 0.3, 0.4])
            .expect("valid source"),
        2,
    );
    received.collect(session.end_track().expect("end"), 2);
    received.collect(session.finish().expect("finish"), 2);
    let expected = render(
        format,
        48_000,
        ToneControls::FLAT,
        &[(0, &[0.1, 0.2, 0.3, 0.4], 0.0)],
    );
    assert_eq!(received.samples, expected.samples);
    assert_eq!(received.completed, [0]);
    assert!(session.begin_track(1, 0.0).is_err());
    assert!(session.push_source(&[0.1, 0.2]).is_err());
    assert!(session.end_track().is_err());
    let repeated = session.finish().expect("idempotent finish");
    assert!(repeated.samples.is_empty());
    assert!(repeated.spans.is_empty());
}

#[test]
fn wav_boundaries_preserve_source_observation_and_continuous_src_equalization() {
    let fixtures = WavFixtures::new();
    let first_frames = 10_001;
    let short_frames = 101;
    let pcm = (0..first_frames + short_frames)
        .flat_map(|index| {
            let left = ((index as f32 * 0.019).sin() * 10_000.0) as i16;
            let right = ((index as f32 * 0.027).cos() * 8_000.0) as i16;
            [left, right]
        })
        .collect::<Vec<_>>();
    let expected_source = pcm
        .iter()
        .map(|&sample| f32::from(sample) / 32_768.0)
        .collect::<Vec<_>>();
    let tone = ToneControls::new(6.0, -4.0).expect("tone");
    for rate in [44_100, 44_101] {
        let whole_path = fixtures.write(&format!("whole-{rate}.wav"), rate, &pcm);
        let first_path =
            fixtures.write(&format!("first-{rate}.wav"), rate, &pcm[..first_frames * 2]);
        let short_path =
            fixtures.write(&format!("short-{rate}.wav"), rate, &pcm[first_frames * 2..]);
        let format = PcmFormat::new(rate, 2).expect("source format");
        let (whole, whole_observed) = decode_observed_session(&[&whole_path], format, tone);
        let (split, split_observed) =
            decode_observed_session(&[&first_path, &short_path], format, tone);
        assert_eq!(whole_observed[0], expected_source);
        assert_eq!(split_observed[0], expected_source[..first_frames * 2]);
        assert_eq!(split_observed[1], expected_source[first_frames * 2..]);
        assert_eq!(split.samples, whole.samples);
        let first_boundary = (first_frames as u64 * 48_000).div_ceil(u64::from(rate)) as usize;
        let group_boundary =
            ((first_frames + short_frames) as u64 * 48_000).div_ceil(u64::from(rate)) as usize;
        assert_eq!(split.track_frames[&0], first_boundary);
        assert_eq!(split.track_frames[&1], group_boundary - first_boundary);
        assert_eq!(split.samples.len(), group_boundary * 2);
        assert_eq!(split.completed, [0, 1]);
    }
}
