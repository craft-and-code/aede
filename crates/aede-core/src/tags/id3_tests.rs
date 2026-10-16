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
    parse_frames(&body, 3, false, &mut tags);
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
    parse_frames(&body, 3, false, &mut tags);
    assert_eq!(tags.first("artist"), Some("Björk"));
}

#[test]
fn multiple_values_v24() {
    let body = text_frame("TPE1", 3, b"Miles Davis\0John Coltrane");
    let mut tags = RawTags::default();
    parse_frames(&body, 4, false, &mut tags);
    assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
}

#[test]
fn recording_day_and_month_do_not_become_a_year() {
    for reverse in [false, true] {
        let frames = if reverse {
            [("TYER", b"1959".as_slice()), ("TDAT", b"2512".as_slice())]
        } else {
            [("TDAT", b"2512".as_slice()), ("TYER", b"1959".as_slice())]
        };
        let body: Vec<u8> = frames
            .iter()
            .flat_map(|(id, value)| text_frame(id, 0, value))
            .collect();
        let mut tags = RawTags::default();
        parse_tag_body(&body, 3, 0, &mut tags);
        assert_eq!(tags.all("date"), ["1959"]);
        assert_eq!(tags.first("recordingdaymonth"), Some("2512"));
    }

    let mut tags = RawTags::default();
    parse_tag_body(&text_frame("TDAT", 0, b"2512"), 3, 0, &mut tags);
    assert!(tags.all("date").is_empty());
    assert_eq!(tags.first("recordingdaymonth"), Some("2512"));

    // The corresponding ID3v2.2 frame has the same DDMM meaning.
    let mut tags = RawTags::default();
    parse_tag_body(
        b"TDA\0\0\x05\0\x32\x35\x31\x32TYE\0\0\x05\0\x31\x39\x35\x39",
        2,
        0,
        &mut tags,
    );
    assert_eq!(tags.all("date"), ["1959"]);
    assert_eq!(tags.first("recordingdaymonth"), Some("2512"));
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
    parse_frames(&body, 4, false, &mut tags);
    assert_eq!(tags.first("musicbrainz_albumid"), Some("abc-123"));
}

#[test]
fn release_track_identifiers_do_not_become_recording_identifiers() {
    const RECORDING: &str = "11111111-1111-4111-8111-111111111111";
    for description in ["MusicBrainz Release Track Id", "MUSICBRAINZ_RELEASETRACKID"] {
        for recording_first in [false, true] {
            let mut scanned = Vec::new();
            for (index, placement) in [
                "22222222-2222-4222-8222-222222222222",
                "33333333-3333-4333-8333-333333333333",
            ]
            .into_iter()
            .enumerate()
            {
                let release_frame =
                    text_frame("TXXX", 0, format!("{description}\0{placement}").as_bytes());
                let recording_frame = text_frame(
                    "TXXX",
                    0,
                    format!("MusicBrainz Track Id\0{RECORDING}").as_bytes(),
                );
                let body = if recording_first {
                    [recording_frame, release_frame].concat()
                } else {
                    [release_frame, recording_frame].concat()
                };
                let mut tags = RawTags::default();
                parse_frames(&body, 3, false, &mut tags);
                assert_eq!(tags.all("musicbrainz_recordingid"), [RECORDING]);
                assert_eq!(tags.all("musicbrainz_trackid"), [placement]);
                tags.insert("title", "Shared recording");
                tags.insert("artist", "Artist");
                tags.insert("album", format!("Edition {index}"));
                scanned.push(crate::model::ScannedFile {
                    path: format!("/fictional/Edition {index}/01.mp3"),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                });
                let mut placement_only = RawTags::default();
                parse_frames(
                    &text_frame("TXXX", 0, format!("{description}\0{placement}").as_bytes()),
                    3,
                    false,
                    &mut placement_only,
                );
                assert!(placement_only.all("musicbrainz_recordingid").is_empty());
            }
            let catalog = crate::model::build(scanned, Vec::new(), 1, &[]);
            assert_eq!(catalog.recordings.len(), 1);
            assert_eq!(catalog.recordings[0].mbid.as_deref(), Some(RECORDING));
            assert_eq!(catalog.recordings[0].track_ids.len(), 2);
            assert!(
                catalog
                    .tracks
                    .iter()
                    .all(|track| track.mbid.as_deref() == Some(RECORDING))
            );
        }
    }
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
    parse_frames(&body, 4, false, &mut tags);
    assert_eq!(tags.first("title"), Some("So What"));
}

#[test]
fn embedded_id3_extended_headers_do_not_hide_the_following_frames() {
    for (major, extended) in [
        (3, vec![0, 0, 0, 6, 0, 0, 0, 0, 0, 0]),
        (4, vec![0, 0, 0, 6, 1, 0]),
    ] {
        let mut body = extended;
        body.extend(text_frame("TIT2", 3, b"So What"));
        let mut block = vec![b'I', b'D', b'3', major, 0, 0x40, 0, 0, 0, body.len() as u8];
        block.extend(body);
        let mut tags = RawTags::default();
        read_id3v2_buffer(&block, &mut tags);
        assert_eq!(tags.first("title"), Some("So What"), "ID3v2.{major}");
    }
}

#[test]
fn v24_frame_unsynchronisation_preserves_the_original_text() {
    let mut body = text_frame("TIT2", 0, b"A\xff\x00\xe0B");
    body[9] = 0x02;
    body.extend(text_frame("TPE1", 3, b"Example Artist"));
    let mut tags = RawTags::default();
    parse_tag_body(&body, 4, 0, &mut tags);
    assert_eq!(tags.all("title"), ["AÿàB"]);
    assert_eq!(tags.first("artist"), Some("Example Artist"));
}

#[test]
fn v24_tag_unsynchronisation_preserves_following_frame_boundaries() {
    let mut body = text_frame("TIT2", 0, b"A\xff\x00\xe0B");
    body[9] = 0x02;
    let mut artist = text_frame("TPE1", 3, b"Example Artist");
    artist[9] = 0x02;
    body.extend(artist);
    let mut tags = RawTags::default();
    parse_tag_body(&body, 4, 0x80, &mut tags);
    assert_eq!(tags.first("artist"), Some("Example Artist"));
    assert_eq!(tags.all("title"), ["AÿàB"]);
}

#[test]
fn v23_tag_unsynchronisation_keeps_its_pre_transform_frame_sizes() {
    let mut body = text_frame("TIT2", 0, b"A\xff\xe0B");
    body.extend(text_frame("TPE1", 3, b"Example Artist"));
    let position = body.iter().position(|&byte| byte == 0xff).unwrap();
    body.insert(position + 1, 0);
    let mut tags = RawTags::default();
    parse_tag_body(&body, 3, 0x80, &mut tags);
    assert_eq!(tags.all("title"), ["AÿàB"]);
    assert_eq!(tags.first("artist"), Some("Example Artist"));
}

#[test]
fn grouped_frames_keep_the_text_encoding_and_following_frames() {
    for (major, flags) in [(3, 0x20), (4, 0x40), (4, 0x41), (4, 0x43)] {
        let mut text = vec![1, 0xff, 0xfe];
        for unit in "Jóga".encode_utf16() {
            text.extend_from_slice(&unit.to_le_bytes());
        }
        let mut payload = Vec::new();
        if flags & 1 != 0 {
            payload.extend_from_slice(&[0, 0, 0, text.len() as u8]);
        }
        payload.extend_from_slice(&text);
        let mut grouped = text_frame("TIT2", 0x80, &payload);
        grouped[9] = flags;
        if flags & 2 != 0 {
            let escaped = grouped.iter().position(|&byte| byte == 0xff).unwrap();
            grouped.insert(escaped + 1, 0);
            let encoded_size = (grouped.len() - 10) as u32;
            grouped[4..8].copy_from_slice(&encoded_size.to_be_bytes());
        }
        let mut body = text_frame("GRID", b'h', b"ttps://example.invalid/group\0\x80");
        body.extend(grouped);
        body.extend(text_frame("TPE1", 0, b"Example Artist"));
        let mut tags = RawTags::default();
        parse_tag_body(&body, major, 0, &mut tags);
        assert_eq!(tags.all("title"), ["Jóga"], "v2.{major} flags={flags:#x}");
        assert_eq!(tags.first("artist"), Some("Example Artist"));
    }

    let mut truncated = text_frame("TIT2", 0x80, b"");
    truncated[9] = 0x41;
    truncated.extend(text_frame("TPE1", 0, b"Example Artist"));
    let mut tags = RawTags::default();
    parse_tag_body(&truncated, 4, 0, &mut tags);
    assert!(tags.all("title").is_empty());
    assert_eq!(tags.first("artist"), Some("Example Artist"));
}
