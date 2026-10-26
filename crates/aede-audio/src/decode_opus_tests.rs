use super::*;
use crate::{
    decode::{DecodeError, PcmStreamDecoder},
    test_support::Wave,
};
use std::{io::Cursor, path::Path};
use symphonia::core::{
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

const MONO: &[u8] = include_bytes!("../tests/fixtures/opus-mono.opus");
const STEREO: &[u8] = include_bytes!("../tests/fixtures/opus-stereo.opus");
const SHORT: &[u8] = include_bytes!("../tests/fixtures/opus-short.opus");

fn source(bytes: &[u8]) -> Wave {
    let source = Wave::new(&[], 1);
    std::fs::write(&source.0, bytes).unwrap();
    source
}
fn samples(path: &Path) -> (usize, Vec<f32>) {
    let mut decoder = PcmStreamDecoder::open(path).unwrap();
    assert_eq!(decoder.sample_rate, RATE);
    let mut samples = Vec::new();
    while let Some(chunk) = decoder.next_chunk().unwrap() {
        assert!(!chunk.is_empty());
        assert!(chunk.len() <= MAX_FRAMES * decoder.channels);
        samples.extend(chunk);
    }
    assert!(decoder.next_chunk().unwrap().is_none());
    (decoder.channels, samples)
}
fn head(bytes: &[u8], change: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    let segments = usize::from(bytes[26]);
    let start = 27 + segments;
    let end = start
        + bytes[27..start]
            .iter()
            .map(|n| usize::from(*n))
            .sum::<usize>();
    change(&mut bytes[start..end]);
    bytes[22..26].fill(0);
    let mut crc = 0u32;
    for byte in &bytes[..end] {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x80000000 != 0 {
                (crc << 1) ^ 0x04c11db7
            } else {
                crc << 1
            };
        }
    }
    bytes[22..26].copy_from_slice(&crc.to_le_bytes());
    bytes
}
fn probed(
    bytes: &[u8],
) -> (
    Box<dyn symphonia::core::formats::FormatReader>,
    CodecParameters,
) {
    let mut hint = Hint::new();
    hint.with_extension("opus");
    let format = symphonia::default::get_probe()
        .format(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default()),
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .unwrap()
        .format;
    let params = format.default_track().unwrap().codec_params.clone();
    (format, params)
}

#[test]
fn real_mono_stereo_and_single_page_endings_have_only_the_playable_frames() {
    for (bytes, channels, frames) in [(MONO, 1, 12037), (STEREO, 2, 48013), (SHORT, 1, 121)] {
        let original = bytes.to_vec();
        let source = source(bytes);
        let (actual, pcm) = samples(&source.0);
        assert_eq!(actual, channels);
        assert_eq!(pcm.len(), frames * channels);
        assert!(pcm.iter().all(|s| s.is_finite()));
        assert_eq!(std::fs::read(&source.0).unwrap(), original);
        if channels == 2 {
            let separation: f32 = pcm
                .as_chunks::<2>()
                .0
                .iter()
                .map(|f| (f[0] - f[1]).abs())
                .sum();
            assert!(separation / frames as f32 > 0.05);
        }
    }
}

#[test]
fn preskip_can_span_packets_and_input_rate_does_not_change_the_output_clock() {
    let baseline = source(MONO);
    let (_, expected) = samples(&baseline.0);
    let bytes = head(MONO, |h| {
        let skip = u16::from_le_bytes([h[10], h[11]]) + 1920;
        h[10..12].copy_from_slice(&skip.to_le_bytes());
        h[12..16].copy_from_slice(&44100u32.to_le_bytes());
    });
    let cropped = source(&bytes);
    let (_, actual) = samples(&cropped.0);
    assert_eq!(actual, expected[1920..]);
}

#[test]
fn declared_gain_is_applied_once_and_channels_are_not_swapped() {
    let baseline = source(STEREO);
    let (_, expected) = samples(&baseline.0);
    let changed = source(&head(STEREO, |h| {
        h[16..18].copy_from_slice(&1536i16.to_le_bytes())
    }));
    let (_, actual) = samples(&changed.0);
    let gain = 10f32.powf(6.0 / 20.0);
    for (a, e) in actual.iter().zip(&expected) {
        assert!((a - e * gain).abs() < 0.00001);
    }
}

#[test]
fn reset_keeps_header_gain_and_the_same_initial_playable_samples() {
    let (mut format, params) = probed(&head(MONO, |h| {
        h[16..18].copy_from_slice(&1536i16.to_le_bytes());
    }));
    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let packet = format.next_packet().unwrap();
    let first = match decoder.decode(&packet).unwrap() {
        AudioBufferRef::F32(buffer) => buffer.chan(0).to_vec(),
        _ => panic!("Opus produces float PCM"),
    };
    decoder.reset();
    let again = match decoder.decode(&packet).unwrap() {
        AudioBufferRef::F32(buffer) => buffer.chan(0).to_vec(),
        _ => panic!("Opus produces float PCM"),
    };
    assert_eq!(first, again);
}

#[test]
fn corrupt_packet_clears_the_previous_output_and_retires_the_codec() {
    let (mut format, params) = probed(MONO);
    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let packet = format.next_packet().unwrap();
    assert!(decoder.decode(&packet).unwrap().frames() > 0);
    let bad = Packet::new_from_slice(packet.track_id(), 0, 960, &[]);
    assert!(decoder.decode(&bad).is_err());
    assert_eq!(decoder.last_decoded().frames(), 0);
    assert!(decoder.decode(&packet).is_err());
    assert_eq!(decoder.finalize().verify_ok, Some(false));
    decoder.reset();
    let reset = decoder.decode(&packet).unwrap();
    assert_eq!(reset.frames(), 648);
}

#[test]
fn partial_stream_and_unsupported_mapping_cannot_claim_completed_native_audio() {
    let (mut format, params) = probed(MONO);
    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    decoder.decode(&format.next_packet().unwrap()).unwrap();
    assert_eq!(decoder.finalize().verify_ok, Some(false));
    let mut unsupported = params.clone();
    unsupported.extra_data.as_mut().unwrap()[18] = 1;
    assert!(OpusDecoder::try_new(&unsupported, &DecoderOptions::default()).is_err());
    let mut wrong = params.clone();
    wrong.sample_rate = Some(44100);
    assert!(OpusDecoder::try_new(&wrong, &DecoderOptions::default()).is_err());
    wrong = params.clone();
    wrong.channels = Some(symphonia::core::audio::Channels::FRONT_RIGHT);
    assert!(OpusDecoder::try_new(&wrong, &DecoderOptions::default()).is_err());
    let mut short = params;
    short.extra_data = Some(vec![0; 18].into_boxed_slice());
    assert!(OpusDecoder::try_new(&short, &DecoderOptions::default()).is_err());
}

#[test]
fn cancellation_after_decoding_never_publishes_or_resumes_the_old_packet() {
    let source = source(MONO);
    let mut decoder = PcmStreamDecoder::open(&source.0).unwrap();
    let mut calls = 0;
    let result = decoder.next_chunk_admitted(|| {
        calls += 1;
        if calls == 3 {
            Err(DecodeError("retired owner".into()))
        } else {
            Ok(())
        }
    });
    assert!(result.is_err());
    assert!(decoder.next_chunk().is_err());
    let (_, fresh) = samples(&source.0);
    assert_eq!(fresh.len(), 12037);
}

#[test]
fn reset_requires_preroll_and_truncated_files_cannot_end_successfully() {
    let (mut format, params) = probed(STEREO);
    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    decoder.decode(&format.next_packet().unwrap()).unwrap();
    let second = format.next_packet().unwrap();
    decoder.reset();
    assert!(decoder.decode(&second).is_err());
    assert_eq!(decoder.last_decoded().frames(), 0);
    for bytes in [&MONO[..MONO.len() - 10], &STEREO[..STEREO.len() - 10]] {
        let source = source(bytes);
        if let Ok(mut stream) = PcmStreamDecoder::open(&source.0) {
            loop {
                match stream.next_chunk() {
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("truncated Opus cannot complete"),
                    Err(_) => break,
                }
            }
            assert!(stream.next_chunk().is_err());
        }
    }
}

#[test]
fn chained_streams_are_not_silently_reported_as_one_completed_track() {
    let bytes = [MONO, STEREO].concat();
    let source = source(&bytes);
    if let Ok(mut stream) = PcmStreamDecoder::open(&source.0) {
        loop {
            match stream.next_chunk() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("chained Opus requires explicit handling"),
                Err(_) => break,
            }
        }
        assert!(stream.next_chunk().is_err());
    }
}

#[test]
fn opus_cut_at_a_complete_page_cannot_claim_a_complete_track() {
    let mut at = 0;
    let mut final_page = 0;
    while at < STEREO.len() {
        final_page = at;
        let segments = usize::from(STEREO[at + 26]);
        at += 27
            + segments
            + STEREO[at + 27..at + 27 + segments]
                .iter()
                .map(|n| usize::from(*n))
                .sum::<usize>();
    }
    assert_eq!(STEREO[final_page + 5] & 4, 4);
    let source = source(&STEREO[..final_page]);
    if let Ok(mut stream) = PcmStreamDecoder::open(&source.0) {
        let mut frames = 0;
        loop {
            match stream.next_chunk() {
                Ok(Some(chunk)) => frames += chunk.len() / stream.channels,
                Ok(None) => panic!("missing final EOS page accepted as {frames} complete frames"),
                Err(_) => break,
            }
        }
    }
}
