use std::path::PathBuf;

use aede_core::playback::decoder::FileDecoder;
use aede_dsp::{Dsp, PcmFormat};

use super::{play, stream_pcm};
use crate::args::Args;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures")
        .join(name)
}

#[test]
fn pcm_stream_reaches_output_as_interleaved_little_endian_floats() {
    let path = fixture("audit-stereo.flac");
    let mut decoder = FileDecoder::open(&path).expect("fixture opens");
    let format = PcmFormat::new(decoder.sample_rate(), decoder.channels()).expect("format");
    let mut dsp = Dsp::new(format);
    let mut output = Vec::new();
    stream_pcm(&mut decoder, &mut dsp, &mut output).expect("stream succeeds");

    assert!(!output.is_empty());
    assert_eq!(output.len() % (usize::from(format.channels()) * 4), 0);
    let decoded = output
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect::<Vec<_>>();
    let mut reference = FileDecoder::open(&path).expect("fixture reopens");
    let mut block = vec![0.0; decoded.len()];
    let frames = reference.read_frames(&mut block).expect("first packet");
    assert_eq!(
        decoded[..frames * usize::from(format.channels())],
        block[..frames * usize::from(format.channels())]
    );
    assert!(decoded.iter().all(|sample| sample.is_finite()));
}

#[test]
fn play_refuses_a_catalog_location_for_a_direct_file() {
    let args = Args::parse(["play", "track.flac", "--data", "somewhere"].map(str::to_string));
    let error = play(&args).expect_err("catalog path cannot shape direct playback");
    assert!(error.to_string().contains("--data"));
}
