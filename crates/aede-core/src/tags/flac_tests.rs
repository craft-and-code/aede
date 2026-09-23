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
