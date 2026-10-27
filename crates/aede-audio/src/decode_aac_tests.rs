use super::*;

#[test]
fn native_aac_lc_uses_the_playable_edit_window_and_codec_format() {
    for (name, rate, channels, frames, reference) in [
        ("mono441", 44100, 1usize, 10001usize, "mono441"),
        ("stereo48", 48000, 2, 48013, "stereo48"),
        ("stereo96", 96000, 2, 12037, "stereo96"),
        ("short48", 48000, 1, 121, "short48"),
        ("coarse48", 48000, 2, 48013, "stereo48"),
        ("stream48", 48000, 2, 240013, "stream48"),
    ] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-aac");
        let source = root.join(format!("{name}.m4a"));
        let before = std::fs::read(&source).unwrap();
        let mut decoder = PcmStreamDecoder::open(&source).unwrap();
        assert_eq!((decoder.sample_rate, decoder.channels), (rate, channels));
        let mut actual = Vec::new();
        while let Some(chunk) = decoder.next_chunk().unwrap() {
            actual.extend(chunk);
        }
        assert_eq!(actual.len(), frames * channels, "{name}");
        let bytes = std::fs::read(root.join(format!("{reference}.f32"))).unwrap();
        let expected: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert_eq!(expected.len(), actual.len());
        let error: f64 = actual
            .iter()
            .zip(&expected)
            .map(|(&a, &b)| f64::from(a - b).powi(2))
            .sum();
        let signal: f64 = expected.iter().map(|&v| f64::from(v).powi(2)).sum();
        assert!(actual.iter().all(|v| v.is_finite()));
        assert!(
            error < signal * 1e-8,
            "{name}: independent decoder SNR below 80 dB ({error}/{signal})"
        );
        assert_eq!(std::fs::read(source).unwrap(), before);
    }
}

#[test]
fn native_adts_keeps_coded_frames_without_inventing_gapless_metadata() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-aac/mono441.aac");
    let mut decoder = PcmStreamDecoder::open(&path).unwrap();
    assert_eq!((decoder.sample_rate, decoder.channels), (44100, 1));
    let mut frames = 0;
    while let Some(chunk) = decoder.next_chunk().unwrap() {
        frames += chunk.len();
    }
    assert_eq!(frames, 11 * 1024);
}

#[test]
fn missing_aac_tail_is_an_error_and_cancellation_retires_buffered_pcm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-aac");
    let mut bytes = std::fs::read(root.join("stereo48.m4a")).unwrap();
    // Remove the final packet while retaining the complete MP4 index.
    let table = bytes.windows(4).position(|b| b == b"stsz").unwrap() + 4;
    let count = u32::from_be_bytes(bytes[table + 8..table + 12].try_into().unwrap()) as usize;
    let last = table + 12 + (count - 1) * 4;
    let size = u32::from_be_bytes(bytes[last..last + 4].try_into().unwrap()) as usize;
    bytes.truncate(bytes.len() - size);
    let mdat = bytes.windows(4).position(|b| b == b"mdat").unwrap() - 4;
    let length = (bytes.len() - mdat) as u32;
    bytes[mdat..mdat + 4].copy_from_slice(&length.to_be_bytes());
    let temporary = crate::test_support::Wave::new(&[0.0], 1);
    let path = temporary.0.clone();
    std::fs::write(&path, &bytes).unwrap();
    let mut source = PcmStreamDecoder::open(&path).unwrap();
    loop {
        match source.next_chunk() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("a missing AAC packet must not become successful EOF"),
            Err(_) => break,
        }
    }
    assert!(source.next_chunk().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut source = PcmStreamDecoder::open(&root.join("stereo48.m4a")).unwrap();
    assert!(source.next_chunk().unwrap().is_some());
    assert!(
        source
            .next_chunk_admitted(|| Err(DecodeError("cancelled".into())))
            .is_err()
    );
    assert!(source.next_chunk().is_err());
}

#[test]
fn unsupported_aac_edits_are_refused_before_native_pcm_admission() {
    let original = include_bytes!("../tests/fixtures/native-aac/stereo48.m4a");
    let edit = original.windows(4).position(|b| b == b"elst").unwrap() + 4;
    for (offset, value) in [
        (edit + 4, 2u32),
        (edit + 12, u32::MAX),
        (edit + 16, 0),
        (edit, 1),
    ] {
        let temporary = crate::test_support::Wave::new(&[0.0], 1);
        let path = temporary.0.clone();
        let mut bytes = original.to_vec();
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            PcmStreamDecoder::open(&path).is_err(),
            "edit at {offset}: {value}"
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn sample_accurate_aac_edits_are_not_rounded_into_another_duration() {
    let original = include_bytes!("../tests/fixtures/native-aac/stereo48.m4a");
    let edit = original.windows(4).position(|b| b == b"elst").unwrap() + 4;
    for duration in [48012u32, 48014] {
        let temporary = crate::test_support::Wave::new(&[0.0], 1);
        let mut bytes = original.to_vec();
        bytes[edit + 8..edit + 12].copy_from_slice(&duration.to_be_bytes());
        std::fs::write(&temporary.0, &bytes).unwrap();
        assert!(
            PcmStreamDecoder::open(&temporary.0).is_err(),
            "an explicit {duration}-frame edit must not be admitted as 48013 frames"
        );
        assert_eq!(std::fs::read(&temporary.0).unwrap(), bytes);
    }
}

#[test]
fn aac_encoder_preroll_can_span_several_packets_without_changing_channels() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-aac");
    let original = std::fs::read(root.join("stereo48.m4a")).unwrap();
    let edit = original.windows(4).position(|b| b == b"elst").unwrap() + 4;
    let movie = original.windows(4).position(|b| b == b"mvhd").unwrap() + 4;
    let track = original.windows(4).position(|b| b == b"tkhd").unwrap() + 4;
    let mut full = PcmStreamDecoder::open(&root.join("stereo48.m4a")).unwrap();
    let mut expected = Vec::new();
    while let Some(pcm) = full.next_chunk().unwrap() {
        expected.extend(pcm);
    }
    for delay in [2112u32, 3072] {
        let mut bytes = original.to_vec();
        let skipped = delay - 1024;
        let duration = 48013 - skipped;
        bytes[edit + 8..edit + 12].copy_from_slice(&duration.to_be_bytes());
        bytes[edit + 12..edit + 16].copy_from_slice(&delay.to_be_bytes());
        bytes[movie + 16..movie + 20].copy_from_slice(&duration.to_be_bytes());
        bytes[track + 20..track + 24].copy_from_slice(&duration.to_be_bytes());
        let temporary = crate::test_support::Wave::new(&[0.0], 1);
        std::fs::write(&temporary.0, &bytes).unwrap();
        let mut source = PcmStreamDecoder::open(&temporary.0).unwrap();
        assert_eq!((source.sample_rate, source.channels), (48000, 2));
        let mut pcm = Vec::new();
        while let Some(chunk) = source.next_chunk().unwrap() {
            pcm.extend(chunk);
        }
        assert_eq!(pcm, &expected[skipped as usize * 2..]);
        assert_eq!(pcm.len(), duration as usize * 2);
        let mut retired = PcmStreamDecoder::open(&temporary.0).unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(
            retired
                .next_chunk_admitted(|| {
                    calls.set(calls.get() + 1);
                    if calls.get() > 3 {
                        Err(DecodeError("cancelled during encoder pre-roll".into()))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
        );
        assert!(retired.next_chunk().is_err());
        assert_eq!(std::fs::read(&temporary.0).unwrap(), bytes);
    }
}
