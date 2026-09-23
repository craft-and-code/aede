use super::*;

fn text_frame(id: &str, encoding: u8, text: &[u8]) -> Vec<u8> {
    let mut payload = vec![encoding];
    payload.extend_from_slice(text);
    let mut frame = id.as_bytes().to_vec();
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&payload);
    frame
}

#[test]
fn latin1_and_utf8_text_frames() {
    let mut body = text_frame("TIT2", 0, b"So What");
    body.extend(text_frame("TPE1", 3, "Miles Davis".as_bytes()));
    body.extend(text_frame("TALB", 3, "Kind of Blue".as_bytes()));
    let mut tags = RawTags::default();
    parse_frames(&body, 3, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
    assert_eq!(tags.first("artist"), Some("Miles Davis"));
    assert_eq!(tags.first("album"), Some("Kind of Blue"));
}

#[test]
fn utf16_frame_with_bom() {
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in "Björk".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    let body = text_frame("TPE1", 1, &utf16);
    let mut tags = RawTags::default();
    parse_frames(&body, 3, &mut tags);
    assert_eq!(tags.first("artist"), Some("Björk"));
}

#[test]
fn multiple_values_v24() {
    let body = text_frame("TPE1", 3, b"Miles Davis\0John Coltrane");
    let mut tags = RawTags::default();
    parse_frames(&body, 4, &mut tags);
    assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
}

#[test]
fn numeric_genres() {
    assert_eq!(expand_genre("(17)"), vec!["Rock"]);
    assert_eq!(expand_genre("(17)Rock"), vec!["Rock", "Rock"]);
    assert_eq!(expand_genre("32"), vec!["Classical"]);
    assert_eq!(expand_genre("Post-Rock"), vec!["Post-Rock"]);
    assert_eq!(expand_genre("(8)(32)"), vec!["Jazz", "Classical"]);
}

#[test]
fn txxx_musicbrainz() {
    let body = text_frame("TXXX", 3, b"MusicBrainz Album Id\0abc-123");
    let mut tags = RawTags::default();
    parse_frames(&body, 4, &mut tags);
    assert_eq!(tags.first("musicbrainz_albumid"), Some("abc-123"));
}

#[test]
fn unsynchronisation() {
    assert_eq!(deunsynchronize(&[0xFF, 0x00, 0xE0]), vec![0xFF, 0xE0]);
    assert_eq!(deunsynchronize(&[0x01, 0x02]), vec![0x01, 0x02]);
}

#[test]
fn padding_stops_reading() {
    let mut body = text_frame("TIT2", 3, b"So What");
    body.extend(vec![0u8; 64]);
    let mut tags = RawTags::default();
    parse_frames(&body, 4, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
}
