use super::*;
fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native-extra")
        .join(name)
}
#[test]
fn native_extra_sources_decode_complete_finite_frames() {
    for (name, kind) in [
        ("track.wv", Kind::WavPack),
        ("tone-sv8.mpc", Kind::Musepack),
        ("float32.wv", Kind::WavPack),
        ("hybrid-lossy.wv", Kind::WavPack),
        ("ramp.ape", Kind::Ape),
        ("track.spx", Kind::Speex),
        ("speex-nb.spx", Kind::Speex),
        ("speex-wb.spx", Kind::Speex),
        ("speex-uwb.spx", Kind::Speex),
    ] {
        let path = fixture(name);
        let before = fs::read(&path).unwrap();
        let mut source = Decoder::open(&path)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap();
        let info = source.info();
        assert_eq!(info.kind, kind);
        let mut frames = 0u64;
        let mut peak = 0.0f32;
        while let Some(pcm) = source
            .next_chunk()
            .unwrap_or_else(|e| panic!("{name}: {e}"))
        {
            assert!(pcm.iter().all(|v| v.is_finite()));
            assert!(pcm.len().is_multiple_of(info.channels));
            frames += (pcm.len() / info.channels) as u64;
            peak = pcm.iter().fold(peak, |a, b| a.max(b.abs()));
        }
        assert!(frames > 0);
        assert!(
            peak > 0.001,
            "{name}: normalization must not attenuate real audio"
        );
        if let Some(expected) = info.frames {
            assert_eq!(frames, expected, "{name}");
        }
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
#[test]
fn native_ape_preserves_every_original_cd_sample() {
    let mut decoder = Decoder::open(&fixture("ramp.ape")).unwrap().unwrap();
    let mut at = 0usize;
    while let Some(pcm) = decoder.next_chunk().unwrap() {
        for frame in pcm.as_chunks::<2>().0 {
            let left = (at % 30001) as i32 - 15000;
            assert_eq!(*frame, [left as f32 / 32768.0, -(left as f32) / 32768.0]);
            at += 1;
        }
    }
    assert_eq!(at, 132300);
}

#[test]
fn native_extra_decoders_agree_with_independent_reference_pcm() {
    for (name, reference, tolerance) in [
        ("track.wv", "track.wv.f32", 1e-6f32),
        ("float32.wv", "float32.wv.f32", 1e-6),
        ("hybrid-lossy.wv", "hybrid-lossy.wv.f32", 1e-6),
        ("ramp.ape", "ramp.ape.f32", 1e-6),
        ("tone-sv8.mpc", "tone-sv8.f32", 2.0 / 32768.0),
        ("speex-nb.spx", "speex-nb.f32", 3.0 / 32768.0),
        ("speex-wb.spx", "speex-wb.f32", 3.0 / 32768.0),
        ("speex-uwb.spx", "speex-uwb.f32", 3.0 / 32768.0),
    ] {
        let mut source = Decoder::open(&fixture(name)).unwrap().unwrap();
        let mut pcm = Vec::new();
        while let Some(chunk) = source.next_chunk().unwrap() {
            pcm.extend(chunk);
        }
        let bytes = fs::read(fixture(reference)).unwrap();
        let expected: Vec<_> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert_eq!(pcm.len(), expected.len(), "{name} complete frames");
        let peak = pcm
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            peak <= tolerance,
            "{name}: independent PCM peak error {peak}"
        );
    }
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("aede-native-{label}-{}-{id}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn read_all(decoder: &mut Decoder) -> Result<usize> {
    let mut count = 0;
    while let Some(pcm) = decoder.next_chunk()? {
        count += pcm.len();
    }
    Ok(count)
}

#[test]
fn cancellation_before_or_after_codec_work_retires_without_publishing_pcm() {
    for name in ["track.wv", "tone-sv8.mpc", "ramp.ape", "speex-wb.spx"] {
        for stop in 1..=3 {
            let mut decoder = Decoder::open(&fixture(name)).unwrap().unwrap();
            let mut calls = 0;
            let result = decoder.next_chunk_admitted(|| {
                calls += 1;
                if calls == stop {
                    Err(Error("owner retired".into()))
                } else {
                    Ok(())
                }
            });
            assert!(result.is_err(), "{name}: cancellation check {stop}");
            assert!(
                decoder.next_chunk().is_err(),
                "{name}: retired source cannot resume"
            );
        }
    }
}

#[test]
fn incomplete_containers_and_oversized_headers_are_refused() {
    let dir = Scratch::new("invalid");
    for name in ["track.wv", "tone-sv8.mpc", "ramp.ape", "speex-wb.spx"] {
        let bytes = fs::read(fixture(name)).unwrap();
        let tail = if name == "track.wv" {
            35701
        } else {
            bytes.len() - 1
        };
        for limit in [4, 31, bytes.len() / 2, tail] {
            let path = dir.write(name, &bytes[..limit]);
            let failed = match Decoder::open(&path) {
                Err(_) => true,
                Ok(Some(mut source)) => read_all(&mut source).is_err(),
                Ok(None) => true,
            };
            assert!(
                failed,
                "{name} truncated at {limit} must not become successful EOF"
            );
        }
    }
    for (name, offset) in [("track.wv", 4), ("ramp.ape", 8)] {
        let mut bytes = fs::read(fixture(name)).unwrap();
        bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Decoder::open(&dir.write(name, &bytes)).is_err());
    }
}

#[test]
fn bad_audio_crc_retires_the_decoder_instead_of_publishing_silence() {
    let dir = Scratch::new("crc");
    for (name, offset) in [("track.wv", 100), ("ramp.ape", 150), ("speex-wb.spx", 200)] {
        let mut bytes = fs::read(fixture(name)).unwrap();
        bytes[offset] ^= 0x55;
        let path = dir.write(name, &bytes);
        match Decoder::open(&path) {
            Err(_) => {}
            Ok(Some(mut source)) => {
                assert!(
                    read_all(&mut source).is_err(),
                    "{name}: corruption must fail"
                );
                assert!(source.next_chunk().is_err());
            }
            Ok(None) => panic!("{name}: corruption must not change codec recognition"),
        }
    }
}

#[test]
fn native_decoder_keeps_the_opened_source_when_its_path_is_replaced() {
    let dir = Scratch::new("identity");
    for name in ["track.wv", "tone-sv8.mpc", "ramp.ape", "speex-wb.spx"] {
        let original = fs::read(fixture(name)).unwrap();
        let path = dir.write(name, &original);
        let mut source = Decoder::open(&path).unwrap().unwrap();
        let mut expected = Decoder::open(&fixture(name)).unwrap().unwrap();
        fs::rename(&path, dir.0.join(format!("old-{name}"))).unwrap();
        fs::write(&path, b"unrelated replacement").unwrap();
        loop {
            let a = source.next_chunk().unwrap();
            let b = expected.next_chunk().unwrap();
            assert_eq!(a, b, "{name}");
            if a.is_none() {
                break;
            }
        }
    }
}

#[test]
fn musepack_workers_can_decode_concurrently_without_shared_table_races() {
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let mut source = Decoder::open(&fixture("tone-sv8.mpc")).unwrap().unwrap();
                read_all(&mut source).unwrap()
            })
        })
        .collect();
    for worker in workers {
        assert_eq!(worker.join().unwrap(), 44113 * 2);
    }
}

fn repair_sv8_header_crc(bytes: &mut [u8]) {
    // IEEE CRC-32 over this fixture's SH payload after its four-byte checksum.
    let mut crc = !0u32;
    for &byte in &bytes[11..18] {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes[7..11].copy_from_slice(&(!crc).to_be_bytes());
}

#[test]
fn musepack_rejects_seek_exponents_that_overflow_the_reference_word_before_ffi() {
    let dir = Scratch::new("mpc-seek-power");
    let mut bytes = fs::read(fixture("tone-sv8.mpc")).unwrap();
    assert_eq!(&bytes[4..7], b"SH\x0e");
    assert_eq!(&bytes[11564..11567], b"ST\x06");
    bytes[17] = (bytes[17] & 0xf8) | 7; // 2 * 7 = 14 audio block power.
    bytes[11568] = (bytes[11568] & 15) | 0xf0; // +15 seek power => 29.
    repair_sv8_header_crc(&mut bytes);
    let path = dir.write("bad.mpc", &bytes);
    let mut source = File::open(&path).unwrap();
    assert!(
        preflight::musepack(&mut source).is_err(),
        "unsafe C seek shift was admitted"
    );
    assert!(crate::decode::PcmStreamDecoder::open(&path).is_err());
}

#[test]
fn musepack_seek_references_cannot_parse_a_header_inside_encoded_audio() {
    let dir = Scratch::new("mpc-seek-offset");
    let mut bytes = fs::read(fixture("tone-sv8.mpc")).unwrap();
    // The SO packet starts at 37. Point 32 bytes forward, inside AP data,
    // rather than to the real ST packet at 11564. Keep the padded SO size.
    bytes[40..45].copy_from_slice(&[32, 0, 0, 0, 0]);
    bytes[69..75].copy_from_slice(b"ST\x06\x01\xf2\xd0");
    let path = dir.write("hidden-table.mpc", &bytes);
    let mut source = File::open(&path).unwrap();
    assert!(
        preflight::musepack(&mut source).is_err(),
        "an embedded unvalidated table was admitted"
    );
    assert!(crate::decode::PcmStreamDecoder::open(&path).is_err());
}

#[test]
fn musepack_duplicate_headers_cannot_replace_the_preflight_channel_or_block_contract() {
    let dir = Scratch::new("mpc-header-replacement");
    let mut bytes = fs::read(fixture("tone-sv8.mpc")).unwrap();
    let mut replacement = bytes[..18].to_vec();
    replacement[17] = (replacement[17] & 0xf8) | 7;
    repair_sv8_header_crc(&mut replacement);
    bytes[40..45].copy_from_slice(&[0xda, 0x15, 0, 0, 0]);
    bytes[11568] = (bytes[11568] & 15) | 0x70;
    bytes.splice(45..45, replacement[4..18].iter().copied());
    let path = dir.write("replaced-header.mpc", &bytes);
    assert!(Decoder::open(&path).is_err());
}

#[test]
fn musepack_safe_seek_word_boundary_and_zero_seek_reference_preserve_the_pcm() {
    let dir = Scratch::new("mpc-valid-seek");
    for zero_reference in [false, true] {
        let mut bytes = fs::read(fixture("tone-sv8.mpc")).unwrap();
        // Existing block power 6 + stored seek delta 14 gives power 20,
        // the largest positive representable signed 32-bit 1152 << power.
        bytes[11568] = (bytes[11568] & 15) | 0xe0;
        if zero_reference {
            bytes[40..45].fill(0);
        }
        let path = dir.write("valid.mpc", &bytes);
        let mut source = Decoder::open(&path).unwrap().unwrap();
        let mut reference = Decoder::open(&fixture("tone-sv8.mpc")).unwrap().unwrap();
        loop {
            let actual = source.next_chunk().unwrap();
            let expected = reference.next_chunk().unwrap();
            assert_eq!(actual, expected);
            if actual.is_none() {
                break;
            }
        }
    }
}

#[test]
fn wavpack_rejects_sample_rate_changes_between_audio_blocks() {
    let dir = Scratch::new("wv-rate-change");
    let mut bytes = fs::read(fixture("track.wv")).unwrap();
    let second = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
    assert_eq!(&bytes[second..second + 4], b"wvpk");
    let flags = u32::from_le_bytes(bytes[second + 24..second + 28].try_into().unwrap());
    assert_eq!((flags >> 23) & 15, 9); // 44.1 kHz, replaced by 48 kHz.
    bytes[second + 24..second + 28]
        .copy_from_slice(&((flags & !(15 << 23)) | (10 << 23)).to_le_bytes());
    let path = dir.write("changed.wv", &bytes);
    assert!(
        Decoder::open(&path).is_err(),
        "contradictory WavPack clock was admitted"
    );
}

#[test]
fn ape_trailing_tags_do_not_become_compressed_frame_data() {
    use std::io::Write;
    let dir = Scratch::new("ape-trailing-tag");
    let mut original = fs::read(fixture("ramp.ape")).unwrap();
    original[32..36].copy_from_slice(&16u32.to_le_bytes());
    original.extend_from_slice(&[0; 16]);
    let path = dir.write("tagged.ape", &original);
    // One sparse APEv2 binary cover-art item. Its bytes are metadata, outside
    // the descriptor's compressed-audio extent and per-frame decode budget.
    let value_size = 65u32 * 1024 * 1024;
    let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(&value_size.to_le_bytes()).unwrap();
    file.write_all(&2u32.to_le_bytes()).unwrap();
    file.write_all(b"Cover Art (Front)\0").unwrap();
    file.write_all(b"cover.jpg\0").unwrap();
    let tag_size = 8 + 18 + value_size + 32;
    file.set_len(original.len() as u64 + u64::from(tag_size) - 32)
        .unwrap();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(b"APETAGEX").unwrap();
    file.write_all(&2000u32.to_le_bytes()).unwrap();
    file.write_all(&tag_size.to_le_bytes()).unwrap();
    file.write_all(&1u32.to_le_bytes()).unwrap();
    file.write_all(&[0; 12]).unwrap();
    drop(file);
    let mut source = Decoder::open(&path).unwrap().unwrap();
    let mut reference = Decoder::open(&fixture("ramp.ape")).unwrap().unwrap();
    loop {
        let actual = source.next_chunk().unwrap();
        let expected = reference.next_chunk().unwrap();
        assert_eq!(actual, expected);
        if actual.is_none() {
            break;
        }
    }
}

#[test]
fn musepack_seek_entry_count_cannot_read_beyond_its_packet_before_ffi() {
    let dir = Scratch::new("mpc-truncated-seek");
    let mut bytes = fs::read(fixture("tone-sv8.mpc")).unwrap();
    bytes[17] &= 0xf8;
    repair_sv8_header_crc(&mut bytes);
    bytes.splice(
        11564..11570,
        b"ST\x09\x28\x00\x00\x00\x00\x00".iter().copied(),
    );
    let path = dir.write("truncated-table.mpc", &bytes);
    let mut file = File::open(&path).unwrap();
    assert!(
        preflight::musepack(&mut file).is_err(),
        "truncated seek entries reached the foreign decoder"
    );
    assert!(Decoder::open(&path).is_err());
}

#[test]
fn wavpack_precision_changes_cannot_reuse_the_first_blocks_pcm_scale() {
    let dir = Scratch::new("wv-precision-change");
    let mut bytes = fs::read(fixture("track.wv")).unwrap();
    let second = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
    let flags = u32::from_le_bytes(bytes[second + 24..second + 28].try_into().unwrap());
    bytes[second + 24..second + 28].copy_from_slice(&(flags | (1 << 13)).to_le_bytes());
    let path = dir.write("changed-depth.wv", &bytes);
    assert!(
        Decoder::open(&path).is_err(),
        "a changed PCM precision reused the original scale"
    );
}

fn with_wavpack_custom_rates(rates: [Option<u32>; 2], wide: bool, large: bool) -> Vec<u8> {
    let original = fs::read(fixture("track.wv")).unwrap();
    let mut at = 0;
    let mut output = Vec::new();
    for rate in rates {
        let size = u32::from_le_bytes(original[at + 4..at + 8].try_into().unwrap()) as usize + 8;
        let mut block = original[at..at + size].to_vec();
        at += size;
        if let Some(rate) = rate {
            let flags = u32::from_le_bytes(block[24..28].try_into().unwrap());
            block[24..28].copy_from_slice(&((flags & !(15 << 23)) | (15 << 23)).to_le_bytes());
            let mut metadata = vec![
                (if wide { 0x27 } else { 0x67 }) | if large { 0x80 } else { 0 },
                2,
            ];
            if large {
                metadata.extend_from_slice(&[0, 0]);
            }
            metadata.extend_from_slice(&rate.to_le_bytes());
            if !wide {
                *metadata.last_mut().unwrap() = 0;
            }
            let added = metadata.len();
            block.splice(32..32, metadata);
            block[4..8].copy_from_slice(&((size - 8 + added) as u32).to_le_bytes());
        }
        output.extend(block);
    }
    assert!(original[at..].starts_with(b"APETAGEX"));
    output.extend_from_slice(&original[at..]);
    output
}

#[test]
fn wavpack_custom_rate_changes_are_refused_before_pcm_is_published() {
    let dir = Scratch::new("wv-custom-rate");
    for (wide, large) in [(false, false), (false, true), (true, false), (true, true)] {
        let bytes = with_wavpack_custom_rates([Some(43210), Some(47654)], wide, large);
        let path = dir.write("changed-rate.wv", &bytes);
        assert!(
            Decoder::open(&path).is_err(),
            "custom clock change was admitted"
        );
    }
}

#[test]
fn wavpack_equal_clocks_remain_valid_across_custom_and_standard_representations() {
    let dir = Scratch::new("wv-same-rate");
    for (wide, large) in [(false, false), (false, true), (true, false), (true, true)] {
        for (rates, expected) in [
            ([Some(43210), Some(43210)], 43210),
            ([None, Some(44100)], 44100),
            ([Some(44100), None], 44100),
        ] {
            let bytes = with_wavpack_custom_rates(rates, wide, large);
            let path = dir.write("same-rate.wv", &bytes);
            let mut decoder = Decoder::open(&path).unwrap().unwrap();
            assert_eq!(decoder.info().sample_rate, expected);
            assert_eq!(read_all(&mut decoder).unwrap(), 44100 * 2);
        }
    }
}

#[test]
fn wavpack_metadata_fields_cannot_cross_their_audio_block_extent() {
    let dir = Scratch::new("wv-metadata-extent");
    let header = fs::read(fixture("track.wv")).unwrap()[..32].to_vec();
    for body in [
        &[0xa7, 2][..],
        &[0x67, 0][..],
        &[0x27, 2, 1][..],
        &[0x67][..],
    ] {
        let mut bytes = header.clone();
        bytes[4..8].copy_from_slice(&((32 + body.len() - 8) as u32).to_le_bytes());
        bytes.extend_from_slice(body);
        assert!(Decoder::open(&dir.write("invalid-metadata.wv", &bytes)).is_err());
    }
}

fn wavpack_clock_metadata_block(rate: u32) -> Vec<u8> {
    let mut bytes = fs::read(fixture("track.wv")).unwrap()[..32].to_vec();
    bytes[4..8].copy_from_slice(&30u32.to_le_bytes());
    bytes[12..32].fill(0);
    bytes.extend_from_slice(&[0x67, 2]);
    bytes.extend_from_slice(&rate.to_le_bytes()[..3]);
    bytes.push(0);
    bytes
}

#[test]
fn wavpack_metadata_only_clocks_follow_the_actual_audio_blocks() {
    let dir = Scratch::new("wv-clock-context");
    for (middle, accepted) in [(47654u32, false), (43210, true)] {
        let mut bytes = with_wavpack_custom_rates([Some(43210), None], false, false);
        let second = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8;
        let flags = u32::from_le_bytes(bytes[second + 24..second + 28].try_into().unwrap());
        bytes[second + 24..second + 28]
            .copy_from_slice(&((flags & !(15 << 23)) | (15 << 23)).to_le_bytes());
        bytes.splice(second..second, wavpack_clock_metadata_block(middle));
        let path = dir.write("middle-clock.wv", &bytes);
        let result = Decoder::open(&path);
        assert_eq!(result.is_ok(), accepted);
        if accepted {
            let mut decoder = result.unwrap().unwrap();
            assert_eq!(decoder.info().sample_rate, 43210);
            assert_eq!(read_all(&mut decoder).unwrap(), 44100 * 2);
        }
    }
    let mut audio = with_wavpack_custom_rates([None, Some(43210)], false, false);
    let flags = u32::from_le_bytes(audio[24..28].try_into().unwrap());
    audio[24..28].copy_from_slice(&((flags & !(15 << 23)) | (15 << 23)).to_le_bytes());
    let mut bytes = wavpack_clock_metadata_block(43210);
    bytes.extend(audio);
    let path = dir.write("leading-clock.wv", &bytes);
    let mut decoder = Decoder::open(&path).unwrap().unwrap();
    assert_eq!(decoder.info().sample_rate, 43210);
    assert_eq!(read_all(&mut decoder).unwrap(), 44100 * 2);
}

#[test]
fn zero_frame_sv7_headers_are_refused_before_the_foreign_sample_count_underflows() {
    let dir = Scratch::new("sv7-zero-frames");
    // SV7 fixed headers are little-endian words. Keep a valid max_band=1
    // and 44.1-kHz stereo so the legacy parser reaches its sample-count logic.
    let mut bytes = vec![0; 64];
    bytes[..4].copy_from_slice(b"MP+\x07");
    bytes[8..12].copy_from_slice(&0x01000000u32.to_le_bytes());
    let path = dir.write("zero.mpc", &bytes);
    let mut file = File::open(&path).unwrap();
    assert!(
        preflight::musepack(&mut file).is_err(),
        "zero SV7 frames reached C"
    );
    assert!(Decoder::open(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    // The lower positive boundary remains admissible to preflight; this
    // synthetic header does not qualify a complete encoded SV7 recording.
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    let mut file = File::open(dir.write("one.mpc", &bytes)).unwrap();
    assert!(preflight::musepack(&mut file).is_ok());
}
