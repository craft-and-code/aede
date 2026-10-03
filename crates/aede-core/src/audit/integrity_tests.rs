use super::*;
use crate::audit::test_support::{AudioCopy, fixture};

#[test]
fn a_complete_ogg_page_before_the_read_limit_does_not_verify_the_rest_of_the_file() {
    let mut page = vec![0; 27];
    page[..4].copy_from_slice(b"OggS");
    let checksum = crc::crc32_ogg(&page);
    page[22..26].copy_from_slice(&checksum.to_le_bytes());
    let prefix_size = page.len();
    page.extend_from_slice(b"unverified trailing bytes");
    let file = AudioCopy::new("ogg", &page);
    assert!(check_with_limit(file.path(), prefix_size).is_err());
}

#[test]
fn a_complete_flac_frame_before_the_read_limit_does_not_verify_later_frames() {
    let bytes = fixture("track.flac");
    let file = AudioCopy::new("flac", &bytes);
    let mut open = std::fs::File::open(file.path()).unwrap();
    let (info, audio_start) = crate::audit::flac::read_stream_info(&mut open).unwrap();
    let audio = &bytes[audio_start as usize..];
    let first = crate::audit::flac::read_frame(&mut BitReader::new(audio), &info).unwrap();
    assert!(
        first.span.end < audio.len(),
        "fixture must have later audio"
    );
    assert!(check_with_limit(file.path(), first.span.end).is_err());
}
