use super::*;
use crate::decode::PcmStreamDecoder;
use std::path::{Path, PathBuf};

fn fixture(channels: usize) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("opus-family1-{channels}.opus"))
}

fn read(path: &Path) -> Vec<f32> {
    let mut decoder = PcmStreamDecoder::open(path).unwrap();
    let mut samples = Vec::new();
    while let Some(chunk) = decoder.next_chunk().unwrap() {
        assert!(chunk.len() <= MAX_FRAMES * decoder.channels);
        samples.extend(chunk);
    }
    samples
}

fn frequency(channels: usize, channel: usize) -> f64 {
    if channels >= 6 && channel == 3 {
        80.0
    } else {
        700.0 + 500.0 * channel as f64
    }
}

#[test]
fn every_family_one_layout_preserves_speakers_and_non_packet_aligned_duration() {
    for channels in 1..=8 {
        let path = fixture(channels);
        let before = std::fs::read(&path).unwrap();
        let decoder = PcmStreamDecoder::open(&path).unwrap();
        let mask = [4, 3, 7, 0x33, 0x37, 0x3f, 0x70f, 0x63f][channels - 1];
        assert!(
            decoder.channel_mask == Some(mask)
                || (channels == 1 && decoder.channel_mask == Some(1))
        );
        assert_eq!((decoder.sample_rate, decoder.channels), (RATE, channels));
        let pcm = read(&path);
        assert_eq!(pcm.len(), 12037 * channels);
        for channel in 0..channels {
            let magnitudes: Vec<f64> = (0..channels)
                .map(|tone| {
                    let (mut real, mut imaginary) = (0.0, 0.0);
                    for (frame, sample) in pcm.iter().skip(channel).step_by(channels).enumerate() {
                        let phase =
                            std::f64::consts::TAU * frequency(channels, tone) * frame as f64
                                / f64::from(RATE);
                        real += f64::from(*sample) * phase.cos();
                        imaginary += f64::from(*sample) * phase.sin();
                    }
                    real.hypot(imaginary) * 2.0 / 12037.0
                })
                .collect();
            let target = magnitudes[channel];
            assert!(
                target > 0.05,
                "{channels} channels, speaker {channel}: {magnitudes:?}"
            );
            for (other, &magnitude) in magnitudes.iter().enumerate() {
                if other != channel {
                    assert!(
                        target > 5.0 * magnitude,
                        "{channels} channels, speaker {channel}: {magnitudes:?}"
                    );
                }
            }
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn duplicate_and_silent_mapping_entries_are_honored_without_reordering_speakers() {
    let original = std::fs::read(fixture(6)).unwrap();
    let baseline = read(&fixture(6));
    let changed = super::tests::head(&original, |head| {
        head[22] = head[21]; // C receives the same coded channel as FL.
        head[26] = 255; // LFE is explicitly silent.
    });
    let source = super::tests::source(&changed);
    let pcm = read(&source.0);
    for (actual, expected) in pcm
        .as_chunks::<6>()
        .0
        .iter()
        .zip(baseline.as_chunks::<6>().0)
    {
        assert_eq!(
            actual,
            &[
                expected[0],
                expected[1],
                expected[0],
                0.0,
                expected[4],
                expected[5]
            ]
        );
    }
}

#[test]
fn malformed_multistream_counts_tables_and_layouts_fail_before_native_initialization() {
    let bytes = std::fs::read(fixture(6)).unwrap();
    let (_, params) = super::tests::probed(&bytes);
    for (offset, value) in [(9, 0), (9, 9), (18, 255), (19, 0), (20, 255), (21, 254)] {
        let mut changed = params.clone();
        changed.extra_data.as_mut().unwrap()[offset] = value;
        assert!(OpusDecoder::try_new(&changed, &DecoderOptions::default()).is_err());
    }
    for length in 19..27 {
        let mut changed = params.clone();
        changed.extra_data = Some(bytes_for_header(&params)[..length].into());
        assert!(OpusDecoder::try_new(&changed, &DecoderOptions::default()).is_err());
    }
    let mut changed = params.clone();
    changed.extra_data.as_mut().unwrap()[19] = 255;
    changed.extra_data.as_mut().unwrap()[20] = 1;
    assert!(OpusDecoder::try_new(&changed, &DecoderOptions::default()).is_err());
    let mut changed = params;
    changed.channels = Some(symphonia::core::audio::Channels::FRONT_LEFT);
    assert!(OpusDecoder::try_new(&changed, &DecoderOptions::default()).is_err());
}

fn bytes_for_header(params: &CodecParameters) -> &[u8] {
    params.extra_data.as_deref().unwrap()
}

#[test]
fn multistream_pcm_writes_stay_inside_the_full_frame_buffer() {
    let bytes = std::fs::read(fixture(8)).unwrap();
    let (mut format, params) = super::tests::probed(&bytes);
    let mut native =
        native::Native::new(bytes_for_header(&params), params.channels.unwrap().bits()).unwrap();
    let packet = format.next_packet().unwrap();
    let mut small = [0.0; 8];
    assert!(native.decode(packet.data.as_ref(), &mut small).is_err());
    let mut pcm = vec![42.0; MAX_FRAMES * 8 + 2];
    assert!(
        native
            .decode(packet.data.as_ref(), &mut pcm[1..MAX_FRAMES * 8 + 1])
            .unwrap()
            > 0
    );
    assert_eq!((pcm[0], pcm[pcm.len() - 1]), (42.0, 42.0));
}

#[test]
fn multistream_preskip_gain_reset_and_packet_failures_retain_their_contracts() {
    let original = std::fs::read(fixture(8)).unwrap();
    let baseline = read(&fixture(8));
    let gain_header = super::tests::head(&original, |head| {
        head[16..18].copy_from_slice(&1536i16.to_le_bytes());
    });
    let gain_source = super::tests::source(&gain_header);
    let gain_pcm = read(&gain_source.0);
    let gain = 10f32.powf(6.0 / 20.0);
    assert_eq!(gain_pcm.len(), baseline.len());
    for (&actual, &expected) in gain_pcm.iter().zip(&baseline) {
        assert!((actual - expected * gain).abs() < 0.00001);
    }
    let cropped_header = super::tests::head(&original, |head| {
        let skip = u16::from_le_bytes([head[10], head[11]]) + 1920;
        head[10..12].copy_from_slice(&skip.to_le_bytes());
    });
    let cropped = super::tests::source(&cropped_header);
    assert_eq!(read(&cropped.0), baseline[1920 * 8..]);

    let (mut format, params) = super::tests::probed(&gain_header);
    let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
    let packet = format.next_packet().unwrap();
    let first = match decoder.decode(&packet).unwrap() {
        AudioBufferRef::F32(buffer) => (0..8).map(|c| buffer.chan(c).to_vec()).collect::<Vec<_>>(),
        _ => panic!("Opus produces floats"),
    };
    let corrupt = Packet::new_from_slice(packet.track_id(), 0, 960, &[0xff]);
    assert!(decoder.decode(&corrupt).is_err());
    assert_eq!(decoder.last_decoded().frames(), 0);
    assert!(decoder.decode(&packet).is_err());
    assert_eq!(decoder.finalize().verify_ok, Some(false));
    decoder.reset();
    match decoder.decode(&packet).unwrap() {
        AudioBufferRef::F32(buffer) => {
            for (channel, expected) in first.iter().enumerate() {
                assert_eq!(buffer.chan(channel), expected);
            }
        }
        _ => panic!("Opus produces floats"),
    }
}

#[test]
fn truncated_and_retired_multistream_files_never_publish_a_successful_short_tail() {
    let original = std::fs::read(fixture(8)).unwrap();
    let truncated = super::tests::source(&original[..original.len() - 10]);
    if let Ok(mut decoder) = PcmStreamDecoder::open(&truncated.0) {
        loop {
            match decoder.next_chunk() {
                Ok(Some(_)) => {}
                Err(_) => break,
                Ok(None) => panic!("truncated multistream completed"),
            }
        }
        assert!(decoder.next_chunk().is_err());
    }
    let mut decoder = PcmStreamDecoder::open(&fixture(8)).unwrap();
    let mut calls = 0;
    assert!(
        decoder
            .next_chunk_admitted(|| {
                calls += 1;
                if calls == 3 {
                    Err(crate::decode::DecodeError("retired".into()))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert!(decoder.next_chunk().is_err());
}
