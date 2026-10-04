use std::convert::Infallible;
use std::path::PathBuf;

use super::{PcmStreamFormat, PcmTrack, StreamError};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit-stereo.flac")
}

fn source_blocks(path: &std::path::Path) -> (PcmStreamFormat, Vec<Vec<f32>>) {
    let mut track = PcmTrack::open(path).expect("source opens");
    let format = track.format();
    let mut blocks = Vec::new();
    while let Some(block) = track
        .read_block(|_| Ok::<(), Infallible>(()))
        .expect("source decodes")
    {
        blocks.push(block.samples.to_vec());
    }
    (format, blocks)
}

#[test]
fn source_observation_precedes_processing_and_does_not_repeat_at_eof() {
    let (source_format, expected) = source_blocks(&fixture());
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    let mut observed = Vec::new();
    let mut output = Vec::new();
    while let Some(block) = track
        .read_block_observed(
            |format, samples| {
                assert_eq!(format, source_format);
                assert!(!samples.is_empty());
                observed.push(samples.to_vec());
            },
            |samples| {
                for sample in samples {
                    *sample *= 0.5;
                }
                Ok::<(), Infallible>(())
            },
        )
        .expect("track decodes")
    {
        output.push(block.samples.to_vec());
    }
    assert_eq!(observed, expected);
    assert_eq!(output.len(), observed.len());
    for (source, processed) in observed.iter().flatten().zip(output.iter().flatten()) {
        assert_eq!(*processed, source * 0.5);
    }
    assert!(
        track
            .read_block_observed(
                |_, _| panic!("EOF must not be observed"),
                |_| Ok::<(), Infallible>(()),
            )
            .expect("EOF")
            .is_none()
    );
}

#[test]
fn source_observation_preserves_every_input_block_during_resampling() {
    let (source_format, expected) = source_blocks(&fixture());
    assert!(expected.len() > 1);
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track.set_output_rate(48_000).expect("output rate");
    let mut observed = Vec::new();
    let mut output_frames = 0usize;
    while let Some(block) = track
        .read_block_observed(
            |format, samples| {
                assert_eq!(format, source_format);
                assert!(!samples.is_empty());
                observed.push(samples.to_vec());
            },
            |samples| {
                samples.fill(0.0);
                Ok::<(), Infallible>(())
            },
        )
        .expect("track resamples")
    {
        output_frames += block.frames;
        assert!(block.samples.iter().all(|sample| *sample == 0.0));
    }
    assert_eq!(observed, expected);
    let source_frames: usize = expected.iter().map(|block| block.len() / 2).sum();
    assert_eq!(
        output_frames as u64,
        (source_frames as u64 * 48_000).div_ceil(44_100)
    );
}

#[test]
fn source_observation_precedes_downmix_and_excludes_the_resampler_tail() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let (source_format, expected) = source_blocks(&path);
    assert_eq!(source_format.channels(), 6);
    assert_eq!(expected.len(), 1);
    let mut track = PcmTrack::open_stereo(&path).expect("known layout");
    track.set_output_rate(48_000).expect("output rate");
    let output_format = track.format();
    let mut observed = Vec::new();
    let mut output_frames = 0usize;
    while let Some(block) = track
        .read_block_observed(
            |format, samples| {
                assert_eq!(format, source_format);
                assert!(!samples.is_empty());
                observed.push(samples.to_vec());
            },
            |samples| {
                samples.fill(0.0);
                Ok::<(), Infallible>(())
            },
        )
        .expect("track downmixes and resamples")
    {
        output_frames += block.frames;
        assert_eq!(block.samples.len(), block.frames * 2);
        assert!(block.samples.iter().all(|sample| *sample == 0.0));
    }
    assert_eq!(observed, expected);
    assert_eq!(
        output_frames as u64,
        (120 * u64::from(output_format.sample_rate()))
            .div_ceil(u64::from(source_format.sample_rate()))
    );
    assert!(
        track
            .read_block_observed(
                |_, _| panic!("resampler tail must not be observed"),
                |_| Ok::<(), Infallible>(()),
            )
            .expect("EOF")
            .is_none()
    );
}

#[test]
fn a_remote_sink_receives_complete_processed_pcm_and_format_metadata() {
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    let format = track.format();
    assert_eq!(format.sample_rate(), 44_100);
    assert_eq!(format.channels(), 2);

    let mut frames = 0;
    let mut changed = false;
    while let Some(block) = track
        .read_block(|samples| {
            for sample in samples {
                *sample *= 0.5;
            }
            Ok::<(), Infallible>(())
        })
        .expect("track decodes")
    {
        frames += block.frames;
        assert_eq!(block.samples.len(), block.frames * 2);
        assert_eq!(block.f32le.len(), block.samples.len() * 4);
        for (sample, bytes) in block.samples.iter().zip(block.f32le.as_chunks::<4>().0) {
            assert_eq!(*sample, f32::from_le_bytes(*bytes));
            changed |= *sample != 0.0;
        }
    }
    assert!(frames > 0);
    assert!(changed);
    assert!(
        track
            .read_block(|_| Ok::<(), Infallible>(()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_processor_cannot_send_non_finite_pcm() {
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    let error = track.read_block(|samples| {
        samples[0] = f32::NAN;
        Ok::<(), Infallible>(())
    });
    assert!(matches!(error, Err(StreamError::NonFiniteSample)));
}

#[test]
fn a_processor_failure_is_returned_to_the_caller() {
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    let error = track.read_block(|_| Err("DSP stage failed"));
    assert!(matches!(
        error,
        Err(StreamError::Process("DSP stage failed"))
    ));
}

#[test]
fn a_remote_or_local_sink_receives_exact_resampled_pcm() {
    let mut direct = PcmTrack::open(&fixture()).expect("fixture opens");
    let mut source_frames = 0usize;
    while let Some(block) = direct
        .read_block(|_| Ok::<(), Infallible>(()))
        .expect("source decodes")
    {
        source_frames += block.frames;
    }

    let mut converted = PcmTrack::open(&fixture()).expect("fixture opens");
    converted.set_output_rate(48_000).expect("device rate");
    assert_eq!(converted.source_format().sample_rate(), 44_100);
    assert_eq!(converted.format().sample_rate(), 48_000);
    let mut output_frames = 0usize;
    while let Some(block) = converted
        .read_block(|_| Ok::<(), Infallible>(()))
        .expect("conversion succeeds")
    {
        output_frames += block.frames;
        assert_eq!(block.samples.len(), block.frames * 2);
        assert_eq!(block.f32le.len(), block.frames * 2 * 4);
        assert!(block.samples.iter().all(|sample| sample.is_finite()));
    }
    assert_eq!(
        output_frames as u64,
        (source_frames as u64 * 48_000).div_ceil(44_100)
    );
}

#[test]
fn output_rate_cannot_change_after_decoding_begins() {
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track
        .read_block(|_| Ok::<(), Infallible>(()))
        .expect("first block");
    assert!(track.set_output_rate(48_000).is_err());
}
#[test]
fn real_five_one_wav_downmixes_to_stereo_without_lfe() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/channel_fixtures/surround-5_1.wav");
    let mut track = super::PcmTrack::open_stereo(&path).expect("known layout");
    assert_eq!(track.source_format().channels(), 6);
    assert_eq!(track.source_format().layout().mask(), Some(0x3f));
    assert_eq!(track.format().channels(), 2);
    let block = track
        .read_block(|_| Ok::<_, ()>(()))
        .expect("read")
        .expect("samples");
    assert_eq!(block.frames, 120);
    assert!(block.samples[0] > 0.2 && block.samples[1] == 0.0); // FL
    assert!(block.samples[2] == 0.0 && block.samples[3] > 0.2); // FR
    assert!((block.samples[4] - block.samples[5]).abs() < 0.000_001); // center
    assert_eq!(&block.samples[6..8], &[0.0, 0.0]); // LFE
    assert!(block.samples[8] > 0.1 && block.samples[9] == 0.0); // rear left
    assert!(block.samples[10] == 0.0 && block.samples[11] > 0.1); // rear right
    assert_eq!(block.f32le.len(), block.frames * 2 * 4);
}

#[test]
fn seeking_rounds_to_the_source_frame_and_observes_only_the_remaining_audio() {
    let (source_format, blocks) = source_blocks(&fixture());
    let reference: Vec<_> = blocks.into_iter().flatten().collect();
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    // 1 ms at 44.1 kHz lies between frames 44 and 45.
    let seek = track
        .seek_from_start(std::time::Duration::from_millis(1), || false)
        .expect("seek succeeds");
    assert_eq!(seek.frames, 44);
    assert!(!seek.reached_eof);
    let mut observed = Vec::new();
    let mut output = Vec::new();
    while let Some(block) = track
        .read_block_observed(
            |format, samples| {
                assert_eq!(format, source_format);
                observed.extend_from_slice(samples);
            },
            |_| Ok::<(), Infallible>(()),
        )
        .expect("suffix decodes")
    {
        output.extend_from_slice(block.samples);
    }
    assert_eq!(observed, reference[44 * 2..]);
    assert_eq!(output, observed);
}

#[test]
fn source_seek_happens_before_resampling_and_retains_exact_remaining_frame_count() {
    let (_, blocks) = source_blocks(&fixture());
    let source_frames = blocks.iter().map(|block| block.len() / 2).sum::<usize>();
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track.set_output_rate(48_000).expect("output rate");
    let seek = track
        .seek_from_start(std::time::Duration::from_millis(137), || false)
        .expect("seek succeeds");
    assert_eq!(seek.frames, 6_041);
    let mut observed_frames = 0;
    let mut output_frames = 0;
    while let Some(block) = track
        .read_block_observed(
            |format, samples| observed_frames += samples.len() / usize::from(format.channels()),
            |_| Ok::<(), Infallible>(()),
        )
        .expect("suffix resamples")
    {
        output_frames += block.frames;
    }
    let remaining_frames = source_frames - seek.frames as usize;
    assert_eq!(observed_frames, remaining_frames);
    assert_eq!(
        output_frames as u64,
        (remaining_frames as u64 * 48_000).div_ceil(44_100)
    );
}

#[test]
fn multichannel_seek_preserves_source_layout_and_stereo_downmix() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let (source_format, blocks) = source_blocks(&path);
    let reference: Vec<_> = blocks.into_iter().flatten().collect();
    let mut track = PcmTrack::open_stereo(&path).expect("known layout");
    let seek = track
        .seek_from_start(std::time::Duration::from_millis(1), || false)
        .expect("seek succeeds");
    let mut observed = Vec::new();
    let mut output_frames = 0;
    while let Some(block) = track
        .read_block_observed(
            |format, samples| {
                assert_eq!(format, source_format);
                observed.extend_from_slice(samples);
            },
            |_| Ok::<(), Infallible>(()),
        )
        .expect("remaining multichannel frames downmix")
    {
        output_frames += block.frames;
        assert_eq!(block.samples.len(), block.frames * 2);
    }
    assert_eq!(observed, reference[seek.frames as usize * 6..]);
    assert_eq!(output_frames as u64, 120 - seek.frames);
    assert_eq!(track.source_format().layout().mask(), Some(0x3f));
}

#[test]
fn positions_past_eof_are_clamped_without_observation_or_processing() {
    let (_, blocks) = source_blocks(&fixture());
    let source_frames = blocks.iter().map(|block| block.len() / 2).sum::<usize>();
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track.set_output_rate(48_000).expect("output rate");
    let seek = track
        .seek_from_start(std::time::Duration::from_secs(10), || false)
        .expect("EOF clamps");
    assert_eq!(seek.frames, source_frames as u64);
    assert!(seek.reached_eof);
    assert!(
        track
            .read_block_observed(
                |_, _| panic!("discarded prefix must not be observed"),
                |_| -> Result<(), Infallible> { panic!("EOF has no processed samples") },
            )
            .expect("clamped EOF")
            .is_none()
    );
}

#[test]
fn seek_rejects_unrepresentable_positions_without_advancing_the_source() {
    let (_, reference) = source_blocks(&fixture());
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    assert!(matches!(
        track.seek_from_start(std::time::Duration::from_secs(u64::MAX), || false),
        Err(super::decoder::Error::InvalidPosition)
    ));
    let block = track
        .read_block(|_| Ok::<(), Infallible>(()))
        .unwrap()
        .unwrap();
    assert_eq!(block.samples, reference[0]);
}

#[test]
fn repeated_or_started_seeks_require_reopening_and_cancellation_is_reported() {
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track
        .seek_from_start(std::time::Duration::ZERO, || false)
        .unwrap();
    assert!(matches!(
        track.seek_from_start(std::time::Duration::ZERO, || false),
        Err(super::decoder::Error::SeekAfterStart)
    ));
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    track.read_block(|_| Ok::<(), Infallible>(())).unwrap();
    assert!(matches!(
        track.seek_from_start(std::time::Duration::ZERO, || false),
        Err(super::decoder::Error::SeekAfterStart)
    ));
    let mut track = PcmTrack::open(&fixture()).expect("fixture opens");
    assert!(matches!(
        track.seek_from_start(std::time::Duration::from_secs(1), || true),
        Err(super::decoder::Error::SeekCancelled)
    ));
}
