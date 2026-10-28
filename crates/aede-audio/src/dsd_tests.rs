use super::*;
use std::f64::consts::PI;

use super::test_fixtures::{chunk, dff, dsf, signal, source};

fn decode(bytes: &[u8], profile: Profile) -> Vec<f32> {
    let file = source(bytes);
    let mut decoder = Decoder::open(&file.0, profile).unwrap();
    let mut samples = Vec::new();
    while let Some(chunk) = decoder.next_chunk(|| Ok(())).unwrap() {
        assert!(chunk.len() <= 4096);
        samples.extend(chunk);
    }
    samples
}
#[test]
fn dsf_bit_orders_and_dff_interleaving_agree_across_channel_blocks() {
    let n = 65536;
    for rate in [2_822_400, 5_644_800] {
        for channels in [1, 2] {
            let data: Vec<_> = (0..channels)
                .map(|c| signal(rate, 1000.0 + 1700.0 * c as f64, n))
                .collect();
            let a = decode(&dsf(&data, rate, n as u64, true), Profile::Measurement);
            let b = decode(&dsf(&data, rate, n as u64, false), Profile::Measurement);
            let c = decode(&dff(&data, rate), Profile::Measurement);
            assert_eq!(a.len(), n / 8 * channels);
            assert_eq!(a, b);
            assert_eq!(a, c);
            assert_eq!(
                decode(&dsf(&data, rate, n as u64, true), Profile::Listening),
                decode(&dff(&data, rate), Profile::Listening)
            );
        }
    }
}
fn amplitude(pcm: &[f32], rate: f64, frequency: f64) -> f64 {
    let samples = &pcm[512..pcm.len() - 512];
    let (mut re, mut im) = (0.0, 0.0);
    for (i, x) in samples.iter().enumerate() {
        let phase = 2.0 * PI * frequency * i as f64 / rate;
        re += f64::from(*x) * phase.cos();
        im += f64::from(*x) * phase.sin();
    }
    2.0 * re.hypot(im) / samples.len() as f64
}
#[test]
fn measurement_preserves_ultrasound_and_listening_rejects_it() {
    for rate in [2_822_400, 5_644_800] {
        for frequency in [1000.0, 20_000.0, 60_000.0] {
            let n = rate as usize / 5;
            let data = vec![signal(rate, frequency, n)];
            let bytes = dsf(&data, rate, n as u64, true);
            let measured = decode(&bytes, Profile::Measurement);
            let audible = decode(&bytes, Profile::Listening);
            let wide = amplitude(&measured, f64::from(rate / 8), frequency);
            assert!(
                (wide - 0.35).abs() < 0.006,
                "{rate} {frequency}: wide {wide}"
            );
            assert_eq!(
                audible.len(),
                (n as u64).div_ceil(u64::from(rate / 88_200)) as usize
            );
            if frequency <= 20_000.0 {
                let level = amplitude(&audible, 88_200.0, frequency);
                assert!(
                    (level - 0.35).abs() < 0.006,
                    "{rate} {frequency}: audible {level}"
                );
            } else {
                let rms = (audible[512..audible.len() - 512]
                    .iter()
                    .map(|x| f64::from(*x).powi(2))
                    .sum::<f64>()
                    / (audible.len() - 1024) as f64)
                    .sqrt();
                assert!(rms < 0.002, "ultrasonic alias {rms}");
            }
        }
    }
}
#[test]
fn partial_byte_and_dsf_padding_do_not_extend_duration() {
    for rate in [2_822_400, 5_644_800] {
        for channels in [1, 2] {
            for n in [1, 7, 8, 9, 32767, 32768, 32769] {
                let data = vec![signal(rate, 1000.0, n); channels];
                let bytes = dsf(&data, rate, n as u64, true);
                assert_eq!(
                    decode(&bytes, Profile::Measurement).len(),
                    n.div_ceil(8) * channels
                );
                assert_eq!(
                    decode(&bytes, Profile::Listening).len(),
                    n.div_ceil((rate / 88_200) as usize) * channels
                );
            }
        }
    }
}

#[test]
fn listening_keeps_channels_separate_across_wraps_and_the_final_drain() {
    for rate in [2_822_400, 5_644_800] {
        let n = 262_151;
        let left = signal(rate, 1000.0, n);
        let right = signal(rate, 3700.0, n);
        let stereo = decode(
            &dsf(&[left.clone(), right.clone()], rate, n as u64, true),
            Profile::Listening,
        );
        let left = decode(&dsf(&[left], rate, n as u64, true), Profile::Listening);
        let right = decode(&dsf(&[right], rate, n as u64, true), Profile::Listening);
        assert_eq!(stereo.len(), left.len() * 2);
        assert_eq!(left.len(), right.len());
        for (i, frame) in stereo.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(frame, &[left[i], right[i]]);
        }
    }
}

#[test]
fn cancellation_during_listening_drain_cannot_publish_or_resume() {
    // One valid source bit still needs lookahead and a listening-filter drain.
    let file = source(&dsf(&[vec![1]], 5_644_800, 1, true));
    let mut decoder = Decoder::open(&file.0, Profile::Listening).unwrap();
    let mut checks = 0;
    let result = decoder.next_chunk(|| {
        checks += 1;
        if checks == 50 {
            Err(DsdError("cancelled".into()))
        } else {
            Ok(())
        }
    });
    assert!(result.is_err());
    assert_eq!(checks, 50);
    assert!(decoder.next_chunk(|| Ok(())).is_err());
}

#[test]
fn late_read_failure_retires_both_profiles_at_both_native_rates() {
    for rate in [2_822_400, 5_644_800] {
        for profile in [Profile::Measurement, Profile::Listening] {
            let file = source(&dsf(&[vec![0x69; 32768]], rate, 262144, true));
            let mut decoder = Decoder::open(&file.0, profile).unwrap();
            assert!(decoder.next_chunk(|| Ok(())).unwrap().is_some());
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file.0)
                .unwrap()
                .set_len(92 + 4096)
                .unwrap();
            assert!(decoder.next_chunk(|| Ok(())).is_err());
            assert!(decoder.next_chunk(|| Ok(())).is_err());
        }
    }
}
#[test]
fn truncated_and_hostile_extents_are_refused() {
    let good = dsf(&[vec![0x69; 4096]], 2_822_400, 32768, true);
    for n in [0, 4, 27, 79, 91, good.len() - 1] {
        let file = source(&good[..n]);
        assert!(Decoder::open(&file.0, Profile::Measurement).is_err());
    }
    for (at, value) in [(4, u64::MAX), (12, 92), (64, u64::MAX), (84, u64::MAX)] {
        let mut bad = good.clone();
        bad[at..at + 8].copy_from_slice(&value.to_le_bytes());
        assert!(Decoder::open(&source(&bad).0, Profile::Measurement).is_err());
    }
    let good = dff(&[vec![0x69; 1024], vec![0x69; 1024]], 2_822_400);
    for n in 0..good.len() {
        let file = source(&good[..n]);
        assert!(Decoder::open(&file.0, Profile::Measurement).is_err());
    }
    let mut dst = good.clone();
    let at = dst.windows(4).position(|x| x == b"CMPR").unwrap() + 12;
    dst[at..at + 4].copy_from_slice(b"DST ");
    assert!(
        Decoder::open(&source(&dst).0, Profile::Measurement)
            .err()
            .expect("unsupported input")
            .to_string()
            .contains("DST")
    );
}
#[test]
fn cancellation_during_measurement_retires_the_source() {
    let file = source(&dsf(&[vec![0x69; 8192]], 2_822_400, 65536, true));
    let mut decoder = Decoder::open(&file.0, Profile::Measurement).unwrap();
    let mut checks = 0;
    assert!(
        decoder
            .next_chunk(|| {
                checks += 1;
                if checks == 20 {
                    Err(DsdError("cancelled".into()))
                } else {
                    Ok(())
                }
            })
            .is_err()
    );
    assert!(decoder.next_chunk(|| Ok(())).is_err());
}
#[test]
fn shared_reader_selects_by_signature_and_playback_profile_is_explicit() {
    let file = source(&dsf(&[vec![0x69; 4096]], 2_822_400, 32768, true));
    let mut wide = crate::decode::PcmStreamDecoder::open(&file.0).unwrap();
    let mut listen = crate::decode::PcmStreamDecoder::open_for_playback(&file.0).unwrap();
    assert_eq!(wide.sample_rate, 352_800);
    assert_eq!(listen.sample_rate, 88_200);
    assert!(wide.next_chunk().unwrap().is_some());
    assert!(listen.next_chunk().unwrap().is_some());
}

#[test]
fn encoded_reference_files_agree_and_are_never_modified() {
    for (dsf, dff, rate, channels) in [
        (
            include_bytes!("../tests/fixtures/tone.dsf").as_slice(),
            include_bytes!("../tests/fixtures/tone.dff").as_slice(),
            352_800,
            1,
        ),
        (
            include_bytes!("../tests/fixtures/stereo128.dsf").as_slice(),
            include_bytes!("../tests/fixtures/stereo128.dff").as_slice(),
            705_600,
            2,
        ),
    ] {
        let a = decode(dsf, Profile::Measurement);
        assert_eq!(a, decode(dff, Profile::Measurement));
        assert_eq!(a.len(), 8192 * channels);
        for channel in 0..channels {
            let samples: Vec<_> = a.chunks_exact(channels).map(|f| f[channel]).collect();
            let frequency = if channel == 0 { 1000.0 } else { 2700.0 };
            assert!((amplitude(&samples, f64::from(rate), frequency) - 0.35).abs() < 0.006);
        }
        for bytes in [dsf, dff] {
            let file = source(bytes);
            let mut decoder = Decoder::open(&file.0, Profile::Listening).unwrap();
            let mut frames = 0;
            while let Some(block) = decoder.next_chunk(|| Ok(())).unwrap() {
                frames += block.len() / channels;
            }
            assert_eq!(frames, if channels == 1 { 2048 } else { 1024 });
            for _ in 0..3 {
                assert!(decoder.next_chunk(|| Ok(())).unwrap().is_none());
            }
            assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
        }
    }
}
#[test]
fn dsd_polarity_and_dc_gain_are_not_inverted_or_normalized() {
    for (byte, expected) in [(0u8, -1.0), (255, 1.0)] {
        let bytes = dsf(&[vec![byte; 8192]], 2_822_400, 65536, true);
        for profile in [Profile::Measurement, Profile::Listening] {
            let pcm = decode(&bytes, profile);
            assert!(
                pcm[512..pcm.len() - 512]
                    .iter()
                    .all(|x| (*x - expected).abs() < 1e-5)
            );
        }
    }
}
#[test]
fn unsupported_rates_channels_and_invalid_layouts_are_explicit_errors() {
    let good = dsf(&[vec![0x69; 4096]], 2_822_400, 32768, true);
    for (at, n) in [
        (40, 2u32),
        (44, 1),
        (48, 5),
        (52, 8),
        (56, 11_289_600),
        (60, 2),
        (72, 1),
    ] {
        let mut bytes = good.clone();
        bytes[at..at + 4].copy_from_slice(&n.to_le_bytes());
        assert!(Decoder::open(&source(&bytes).0, Profile::Measurement).is_err());
    }
    let mut bytes = dff(&[vec![0x69; 1024], vec![0x69; 1024]], 2_822_400);
    let at = bytes.windows(4).position(|x| x == b"SLFT").unwrap();
    bytes[at..at + 4].copy_from_slice(b"SRGT");
    assert!(Decoder::open(&source(&bytes).0, Profile::Measurement).is_err());
}

#[test]
fn structural_dsf_inspection_does_not_expand_native_pcm_admission() {
    for (rate, channel_type, channels, native) in [
        (2_822_400, 1, 1, true),
        (2_822_400, 2, 2, true),
        (3_072_000, 1, 1, false),
        (11_289_600, 1, 1, false),
        (11_289_600, 2, 2, false),
        (11_289_600, 3, 3, false),
        (11_289_600, 4, 4, false),
        (11_289_600, 5, 4, false),
        (11_289_600, 6, 5, false),
        (11_289_600, 7, 6, false),
    ] {
        let mut bytes = dsf(&vec![vec![0x69; 4096]; channels], rate, 32768, true);
        bytes[48..52].copy_from_slice(&u32::to_le_bytes(channel_type));
        let file = source(&bytes);
        let info = inspect_dsf(&mut File::open(&file.0).unwrap()).unwrap();
        assert_eq!(info.sample_rate, rate);
        assert_eq!(info.channels, channels);
        assert_eq!(info.samples, 32768);
        assert_eq!(info.metadata_offset, None);
        assert_eq!(Decoder::open(&file.0, Profile::Measurement).is_ok(), native);
        assert_eq!(std::fs::read(&file.0).unwrap(), bytes);
    }
}

#[test]
fn duplicate_and_overflowing_dff_chunks_and_inner_dsf_metadata_are_refused() {
    let good = dff(&[vec![0x69; 1024]], 2_822_400);
    let mut duplicate = good.clone();
    duplicate.extend(chunk(b"FVER", &[1, 5, 0, 0]));
    let size = (duplicate.len() - 12) as u64;
    duplicate[4..12].copy_from_slice(&size.to_be_bytes());
    assert!(Decoder::open(&source(&duplicate).0, Profile::Measurement).is_err());
    let mut overflow = good;
    let pos = overflow.windows(4).position(|x| x == b"PROP").unwrap() + 4;
    overflow[pos..pos + 8].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(Decoder::open(&source(&overflow).0, Profile::Measurement).is_err());
    let mut inner = dsf(&[vec![0x69; 4096]], 2_822_400, 32768, true);
    inner[20..28].copy_from_slice(&100u64.to_le_bytes());
    assert!(Decoder::open(&source(&inner).0, Profile::Measurement).is_err());
}

#[test]
fn dff_required_chunks_follow_the_supported_grammar() {
    let good = dff(&[vec![0x69; 1024]], 2_822_400);
    for minor in [0, 5, 255] {
        let mut compatible = good.clone();
        compatible[29] = minor;
        assert!(Decoder::open(&source(&compatible).0, Profile::Measurement).is_ok());
    }
    let mut reserved = good.clone();
    reserved[30] = 1;
    let mut late = good.clone();
    let version: Vec<_> = late.drain(16..32).collect();
    late.extend(version);
    let mut late_properties = good.clone();
    let size = u64::from_be_bytes(good[36..44].try_into().unwrap()) as usize;
    let properties: Vec<_> = late_properties.drain(32..44 + size + (size & 1)).collect();
    late_properties.extend(properties);
    let mut accepted = Vec::new();
    for (name, invalid) in [
        ("reserved version bytes", reserved),
        ("late version", late),
        ("late sound properties", late_properties),
    ] {
        if Decoder::open(&source(&invalid).0, Profile::Measurement).is_ok() {
            accepted.push(name);
        }
    }
    assert!(
        accepted.is_empty(),
        "invalid DFF declarations admitted: {accepted:?}"
    );
}
