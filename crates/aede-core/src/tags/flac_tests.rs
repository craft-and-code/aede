use super::*;

fn vorbis_block(entries: &[(&str, &str)]) -> Vec<u8> {
    let vendor = b"aede-test";
    let mut out = Vec::new();
    out.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    out.extend_from_slice(vendor);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (k, v) in entries {
        let entry = format!("{k}={v}");
        out.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        out.extend_from_slice(entry.as_bytes());
    }
    out
}

#[test]
fn vorbis_comments() {
    let block = vorbis_block(&[
        ("ARTIST", "Miles Davis"),
        ("ARTIST", "John Coltrane"),
        ("ALBUM", "Kind of Blue"),
        ("DATE", "1959"),
    ]);
    let mut tags = RawTags::default();
    parse_vorbis_comment(&block, &mut tags);
    assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
    assert_eq!(tags.first("album"), Some("Kind of Blue"));
    assert_eq!(tags.first("date"), Some("1959"));
}

#[test]
fn truncated_vorbis_comments() {
    let mut block = vorbis_block(&[("ARTIST", "Miles Davis"), ("ALBUM", "Kind of Blue")]);
    block.truncate(block.len() - 5);
    let mut tags = RawTags::default();
    parse_vorbis_comment(&block, &mut tags); // must not panic
    assert_eq!(tags.first("artist"), Some("Miles Davis"));
}

#[test]
fn streaminfo_cd() {
    // 44100 Hz, 2 channels, 16 bits, 44100 samples = 1 second.
    let packed: u64 = (44_100u64 << 44) | (1u64 << 41) | (15u64 << 36) | 44_100;
    let mut body = vec![0u8; 10];
    body.extend_from_slice(&packed.to_be_bytes());
    let mut tags = RawTags::default();
    read_streaminfo(&body, &mut tags).unwrap();
    assert_eq!(tags.properties.sample_rate, Some(44_100));
    assert_eq!(tags.properties.channels, Some(2));
    assert_eq!(tags.properties.bit_depth, Some(16));
    assert_eq!(tags.properties.duration_ms, Some(1000));
}

#[test]
fn leading_id3_fields_follow_native_flac_comments() {
    let normal = super::super::test_support::BinaryFile::new(
        "flac",
        include_bytes!("../../tests/fixtures/track.flac"),
    );
    for tags in [normal.read().unwrap(), normal.read_from_file().unwrap()] {
        assert_eq!(tags.first("title"), Some("So What"));
        assert_eq!(tags.first("conductor"), None);
    }
    let syncsafe = |size: usize| {
        [
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]
    };
    let mut frames = Vec::new();
    for (key, value) in [
        ("TIT2", "Legacy title"),
        ("TPE1", "Legacy Artist"),
        ("TALB", "Legacy Album"),
        ("TPE3", "Legacy Conductor"),
        ("TCON", "Jazz"),
    ] {
        frames.extend_from_slice(key.as_bytes());
        frames.extend_from_slice(&syncsafe(value.len() + 1));
        frames.extend_from_slice(&[0, 0, 3]);
        frames.extend_from_slice(value.as_bytes());
    }
    let mut id3 = b"ID3\x04\0\0".to_vec();
    id3.extend_from_slice(&syncsafe(frames.len()));
    id3.extend_from_slice(&frames);

    for native_comments in [false, true] {
        let mut audio = include_bytes!("../../tests/fixtures/track.flac").to_vec();
        if !native_comments {
            let mut offset = 4;
            loop {
                let last = audio[offset] & 0x80 != 0;
                let length = u32::from_be_bytes([
                    0,
                    audio[offset + 1],
                    audio[offset + 2],
                    audio[offset + 3],
                ]) as usize;
                if audio[offset] & 0x7f == BLOCK_VORBIS_COMMENT {
                    audio[offset] = (audio[offset] & 0x80) | 1;
                    audio[offset + 4..offset + 4 + length].fill(0);
                }
                offset += 4 + length;
                if last {
                    break;
                }
            }
        }
        let mut bytes = id3.clone();
        bytes.extend_from_slice(&audio);
        let fixture = super::super::test_support::BinaryFile::new("flac", &bytes);
        let titles = match native_comments {
            true => vec!["So What", "Legacy title"],
            false => vec!["Legacy title"],
        };
        for tags in [fixture.read().unwrap(), fixture.read_from_file().unwrap()] {
            assert_eq!(tags.all("title"), titles);
            assert_eq!(tags.first("conductor"), Some("Legacy Conductor"));
            assert_eq!(tags.all("genre"), ["Jazz"]);
            assert_eq!(tags.properties.codec, "flac");
            assert_eq!(tags.properties.sample_rate, Some(44_100));
            assert_eq!(tags.properties.bit_depth, Some(16));
            assert_eq!(tags.properties.duration_ms, Some(1000));
        }
    }
}
