use super::super::test_support::BinaryFile;
#[test]
fn scans_source_clock_and_end_id3_without_claiming_pcm_precision() {
    let mut bytes = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").to_vec();
    let offset = bytes.len() as u64;
    let mut body = b"TIT2\0\0\0\x06\0\0\x03Tone!".to_vec();
    let mut id3 = b"ID3\x03\0\0\0\0\0\x10".to_vec();
    id3.append(&mut body);
    bytes[20..28].copy_from_slice(&offset.to_le_bytes());
    bytes.extend(id3);
    let length = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&length.to_le_bytes());
    let temp = BinaryFile::new("dsf", &bytes);
    let tags = temp.read().unwrap();
    assert_eq!(tags.first("title"), Some("Tone!"));
    assert_eq!(tags.properties.sample_rate, Some(2_822_400));
    assert_eq!(tags.properties.bit_depth, Some(1));
    assert_eq!(tags.properties.quality_label(), "DSD64");
    assert!(super::super::is_audio_path(std::path::Path::new(
        "album/track.DSF"
    )));
}
#[test]
fn truncated_dsd_does_not_fall_back_to_a_different_parser() {
    let temp = BinaryFile::new("dsf", b"DSD bad header bytes");
    assert!(super::super::read(temp.path()).is_err());
}

#[test]
fn appended_id3_cannot_read_beyond_the_dsd_metadata_extent() {
    let mut bytes = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").to_vec();
    let offset = bytes.len() as u64;
    bytes[20..28].copy_from_slice(&offset.to_le_bytes());
    bytes.extend(b"ID3\x03\0\0\0\0\x01\0");
    let len = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&len.to_le_bytes());
    assert!(BinaryFile::new("dsf", &bytes).read().is_err());
}
