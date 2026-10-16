use super::*;

fn box_with(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    out
}

fn data_box(data_type: u32, payload: &[u8]) -> Vec<u8> {
    let mut inner = data_type.to_be_bytes().to_vec();
    inner.extend_from_slice(&0u32.to_be_bytes()); // locale
    inner.extend_from_slice(payload);
    box_with(b"data", &inner)
}

#[test]
fn text_atoms() {
    let mut ilst = box_with(b"\xa9nam", &data_box(1, b"So What"));
    ilst.extend(box_with(b"\xa9ART", &data_box(1, "Björk".as_bytes())));
    ilst.extend(box_with(b"\xa9alb", &data_box(1, b"Kind of Blue")));
    let mut tags = RawTags::default();
    read_ilst(&ilst, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
    assert_eq!(tags.first("artist"), Some("Björk"));
    assert_eq!(tags.first("album"), Some("Kind of Blue"));
}

#[test]
fn track_number_and_total() {
    let payload = [0u8, 0, 0, 3, 0, 9, 0, 0];
    let ilst = box_with(b"trkn", &data_box(0, &payload));
    let mut tags = RawTags::default();
    read_ilst(&ilst, &mut tags);
    assert_eq!(tags.first("tracknumber"), Some("3"));
    assert_eq!(tags.first("tracktotal"), Some("9"));
}

#[test]
fn cover_art_detected() {
    let ilst = box_with(b"covr", &data_box(13, &[0xFF, 0xD8]));
    let mut tags = RawTags::default();
    read_ilst(&ilst, &mut tags);
    assert!(tags.has_embedded_art);
}

#[test]
fn freeform_atom_musicbrainz() {
    let mut payload = box_with(b"mean", b"\0\0\0\0com.apple.iTunes");
    payload.extend(box_with(b"name", b"\0\0\0\0MusicBrainz Album Id"));
    payload.extend(data_box(1, b"abc-123"));
    let ilst = box_with(b"----", &payload);
    let mut tags = RawTags::default();
    read_ilst(&ilst, &mut tags);
    assert_eq!(tags.first("musicbrainz_albumid"), Some("abc-123"));
}

#[test]
fn freeform_release_track_identity_does_not_replace_recording_identity() {
    const RECORDING: &str = "11111111-1111-4111-8111-111111111111";
    const PLACEMENT: &str = "22222222-2222-4222-8222-222222222222";
    for description in ["MUSICBRAINZ_RELEASETRACKID", "MusicBrainz Release Track Id"] {
        for recording_first in [false, true] {
            let mut entries = [
                (description, PLACEMENT),
                ("MusicBrainz Track Id", RECORDING),
            ];
            if recording_first {
                entries.reverse();
            }
            let ilst: Vec<u8> = entries
                .into_iter()
                .flat_map(|(name, value)| {
                    let mut payload = box_with(b"mean", b"\0\0\0\0com.apple.iTunes");
                    payload.extend(box_with(
                        b"name",
                        &[&b"\0\0\0\0"[..], name.as_bytes()].concat(),
                    ));
                    payload.extend(data_box(1, value.as_bytes()));
                    box_with(b"----", &payload)
                })
                .collect();
            let mut tags = RawTags::default();
            read_ilst(&ilst, &mut tags);
            assert_eq!(tags.all("musicbrainz_recordingid"), [RECORDING]);
            assert_eq!(tags.all("musicbrainz_trackid"), [PLACEMENT]);
        }
    }
}

#[test]
fn mvhd_version_0() {
    let mut payload = vec![0u8, 0, 0, 0]; // version 0 + flags
    payload.extend_from_slice(&0u32.to_be_bytes()); // creation
    payload.extend_from_slice(&0u32.to_be_bytes()); // modification
    payload.extend_from_slice(&1000u32.to_be_bytes()); // timescale
    payload.extend_from_slice(&2500u32.to_be_bytes()); // duration
    let mut tags = RawTags::default();
    read_mvhd(&payload, &mut tags);
    assert_eq!(tags.properties.duration_ms, Some(2500));
}

#[test]
fn long_mp4_durations_do_not_overflow_during_unit_conversion() {
    for (timescale, duration, expected) in [
        (1000u32, u64::MAX / 2, Some(u64::MAX / 2)),
        (1, u64::MAX / 2, None),
    ] {
        let mut payload = vec![1, 0, 0, 0];
        payload.extend_from_slice(&[0; 16]);
        payload.extend_from_slice(&timescale.to_be_bytes());
        payload.extend_from_slice(&duration.to_be_bytes());
        let mut tags = RawTags::default();
        read_mvhd(&payload, &mut tags);
        assert_eq!(tags.properties.duration_ms, expected);
    }
}

#[test]
fn invalid_extended_mp4_box_sizes_are_refused_without_a_panic() {
    for declared in [8u64, 12, u64::MAX] {
        let mut bytes = box_with(b"ftyp", b"M4A \0\0\0\0");
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(b"mvhd");
        bytes.extend_from_slice(&declared.to_be_bytes());
        let fixture = super::super::test_support::BinaryFile::new("m4a", &bytes);
        assert!(matches!(fixture.read(), Err(TagError::Malformed(_))));
    }
}

fn track_with_handler(handler: &[u8; 4], format: &[u8; 4], sample_entry: &[u8]) -> Vec<u8> {
    let mut description = vec![0, 0, 0, 0, 0, 0, 0, 1];
    description.extend(box_with(format, sample_entry));
    let table = box_with(b"stbl", &box_with(b"stsd", &description));
    let mut media = vec![0; 8];
    media.extend(handler);
    media.extend([0; 12]);
    media.push(0); // empty handler name
    let mut body = box_with(b"hdlr", &media);
    body.extend(box_with(b"minf", &table));
    box_with(b"trak", &box_with(b"mdia", &body))
}

#[test]
fn video_tracks_do_not_replace_the_audio_stream_properties() {
    let mut audio = vec![0; 28];
    audio[6..8].copy_from_slice(&1u16.to_be_bytes()); // data reference index
    audio[16..18].copy_from_slice(&2u16.to_be_bytes());
    audio[18..20].copy_from_slice(&24u16.to_be_bytes());
    audio[24..28].copy_from_slice(&(44_100u32 << 16).to_be_bytes());
    let audio = track_with_handler(b"soun", b"alac", &audio);
    let mut video = vec![0; 78];
    video[6..8].copy_from_slice(&1u16.to_be_bytes());
    video[24..26].copy_from_slice(&16u16.to_be_bytes()); // width
    video[26..28].copy_from_slice(&16u16.to_be_bytes()); // height
    let video = track_with_handler(b"vide", b"avc1", &video);
    for tracks in [[&audio, &video], [&video, &audio]] {
        let mut movie = Vec::new();
        for track in tracks {
            movie.extend(track);
        }
        let mut bytes = box_with(b"ftyp", b"M4A \0\0\0\0");
        bytes.extend(box_with(b"moov", &movie));
        let fixture = super::super::test_support::BinaryFile::new("mp4", &bytes);
        let tags = fixture.read().unwrap();
        assert_eq!(tags.properties.codec, "alac");
        assert_eq!(tags.properties.sample_rate, Some(44_100));
        assert_eq!(tags.properties.bit_depth, Some(24));
        assert_eq!(tags.properties.channels, Some(2));
        assert!(tags.properties.lossless);
    }
}

#[test]
fn aac_rates_beyond_the_sample_entry_width_follow_the_decoder_config() {
    let bytes = include_bytes!("../../tests/fixtures/aac96.m4a.bin");
    let fixture = super::super::test_support::BinaryFile::new("m4a", bytes);
    for tags in [fixture.read().unwrap(), fixture.read_from_file().unwrap()] {
        assert_eq!(tags.properties.codec, "aac");
        assert_eq!(tags.properties.sample_rate, Some(96_000));
        assert_eq!(tags.properties.channels, Some(1));
        assert!(!tags.properties.lossless);
        assert_eq!(tags.first("title"), Some("Blue in Green"));
    }
    assert_eq!(std::fs::read(fixture.path()).unwrap(), bytes);
}

fn aac_esds(config: &[u8], flags: u8, padded_size: bool, object: u8) -> Vec<u8> {
    fn descriptor(kind: u8, body: &[u8], padded: bool) -> Vec<u8> {
        assert!(body.len() < 128);
        let mut out = vec![kind];
        if padded {
            out.extend([0x80, 0x80, 0x80]);
        }
        out.push(body.len() as u8);
        out.extend(body);
        out
    }
    let mut decoder = vec![object, 0x15];
    decoder.extend([0; 11]);
    decoder.extend(descriptor(5, config, padded_size));
    let mut elementary = vec![0, 1, flags];
    if flags & 0x80 != 0 {
        elementary.extend([0, 2]);
    }
    if flags & 0x40 != 0 {
        elementary.extend([3, b'a', b'b', b'c']);
    }
    if flags & 0x20 != 0 {
        elementary.extend([0, 3]);
    }
    elementary.extend(descriptor(4, &decoder, padded_size));
    let mut esds = vec![0; 4];
    esds.extend(descriptor(3, &elementary, padded_size));
    esds
}

fn aac_description(esds: &[u8]) -> Vec<u8> {
    let mut entry = vec![0; 28];
    entry[6..8].copy_from_slice(&1u16.to_be_bytes());
    entry[16..18].copy_from_slice(&1u16.to_be_bytes());
    entry[18..20].copy_from_slice(&16u16.to_be_bytes());
    entry[24..28].copy_from_slice(&(48_000u32 << 16).to_be_bytes());
    entry.extend(box_with(b"esds", esds));
    let mut out = vec![0, 0, 0, 0, 0, 0, 0, 1];
    out.extend(box_with(b"mp4a", &entry));
    out
}

fn config_bits(fields: &[(u32, u8)]) -> Vec<u8> {
    let mut out = Vec::<u8>::new();
    let mut at = 0usize;
    for &(value, width) in fields {
        for bit in (0..width).rev() {
            if at.is_multiple_of(8) {
                out.push(0);
            }
            out[at / 8] |= (((value >> bit) & 1) as u8) << (7 - at % 8);
            at += 1;
        }
    }
    out
}

#[test]
fn aac_frequency_descriptors_stay_bounded_and_keep_ordinary_output_rates() {
    let explicit = config_bits(&[(2, 5), (15, 4), (88_200, 24), (1, 4)]);
    let sbr = config_bits(&[(5, 5), (6, 4), (1, 4), (0, 4), (2, 5)]);
    let ps = config_bits(&[(29, 5), (6, 4), (1, 4), (0, 4), (2, 5)]);
    for (config, expected) in [
        (vec![0x10, 0x08], 96_000),
        (vec![0x10, 0x88], 88_200),
        (explicit, 88_200),
        (sbr, 96_000),
        (ps, 96_000),
        (vec![0x13, 0x08], 48_000), // HE-AAC core 24k must not replace output 48k.
        (vec![0x10], 48_000),
        (vec![0x16, 0x88], 48_000), // Reserved frequency index 13.
    ] {
        for flags in [0, 0xe0] {
            for padded in [false, true] {
                let esds = aac_esds(&config, flags, padded, 0x40);
                let mut tags = RawTags::default();
                read_stsd(&aac_description(&esds), &mut tags);
                assert_eq!(tags.properties.sample_rate, Some(expected));
                assert_eq!(tags.properties.channels, Some(1));
            }
        }
    }
    let esds = aac_esds(&[0x10, 0x08], 0xe0, true, 0x40);
    for end in 0..esds.len() {
        let mut tags = RawTags::default();
        read_stsd(&aac_description(&esds[..end]), &mut tags);
        assert_eq!(tags.properties.sample_rate, Some(48_000));
    }
    for invalid in [
        aac_esds(&[0x10, 0x08], 0, false, 0x69), // MPEG audio, not AAC.
        vec![0, 0, 0, 0, 3, 0x80, 0x80, 0x80, 0x80, 0],
    ] {
        let mut tags = RawTags::default();
        read_stsd(&aac_description(&invalid), &mut tags);
        assert_eq!(tags.properties.sample_rate, Some(48_000));
    }
    let mut description = aac_description(&[]);
    // A valid-looking ESDS outside the first sample entry is not its config.
    description.extend(box_with(b"esds", &aac_esds(&[0x10, 0x08], 0, false, 0x40)));
    let mut tags = RawTags::default();
    read_stsd(&description, &mut tags);
    assert_eq!(tags.properties.sample_rate, Some(48_000));
}
