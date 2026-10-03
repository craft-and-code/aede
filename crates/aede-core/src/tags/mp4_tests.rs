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
