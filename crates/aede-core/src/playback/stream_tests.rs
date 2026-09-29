use std::convert::Infallible;
use std::path::PathBuf;

use super::{PcmTrack, StreamError};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit-stereo.flac")
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
