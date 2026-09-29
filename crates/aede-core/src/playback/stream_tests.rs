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
