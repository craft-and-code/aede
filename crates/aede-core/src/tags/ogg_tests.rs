use super::*;

fn page(serial: u32, granule: u64, continued: bool, segments: &[&[u8]]) -> Vec<u8> {
    let mut out = b"OggS".to_vec();
    out.push(0); // version
    out.push(if continued { 0x01 } else { 0x00 });
    out.extend_from_slice(&granule.to_le_bytes());
    out.extend_from_slice(&serial.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // page number
    out.extend_from_slice(&0u32.to_le_bytes()); // CRC (not verified)
    out.push(segments.len() as u8);
    for s in segments {
        out.push(s.len() as u8);
    }
    for s in segments {
        out.extend_from_slice(s);
    }
    out
}

#[test]
fn vorbis_identification() {
    let mut packet = b"\x01vorbis".to_vec();
    packet.extend_from_slice(&0u32.to_le_bytes()); // version
    packet.push(2); // channels
    packet.extend_from_slice(&44_100u32.to_le_bytes());
    packet.extend_from_slice(&0u32.to_le_bytes()); // max
    packet.extend_from_slice(&192_000u32.to_le_bytes()); // nominal
    let mut tags = RawTags::default();
    let rate = read_vorbis_identification(&packet, &mut tags);
    assert_eq!(rate, Some(44_100));
    assert_eq!(tags.properties.channels, Some(2));
    assert_eq!(tags.properties.bitrate_kbps, Some(192));
}

#[test]
fn opus_identification() {
    let mut packet = b"OpusHead".to_vec();
    packet.push(1); // version
    packet.push(2); // channels
    packet.extend_from_slice(&312u16.to_le_bytes()); // pre-skip
    packet.extend_from_slice(&44_100u32.to_le_bytes());
    let mut tags = RawTags::default();
    let rate = read_opus_identification(&packet, &mut tags);
    assert_eq!(rate, Some(48_000));
    assert_eq!(tags.pre_skip(), 312);
}

#[test]
fn packet_reassembly() {
    let long = vec![0xAAu8; 255];
    let rest = vec![0xBBu8; 10];
    let data = [
        page(7, 0, false, &[b"\x01vorbis-ident"]),
        page(7, 0, false, &[&long, &rest]),
    ]
    .concat();
    let pages = parse_pages(&data);
    let packets = reassemble_packets(&pages, 7);
    assert_eq!(packets.len(), 2);
    assert_eq!(packets[0], b"\x01vorbis-ident");
    assert_eq!(packets[1].len(), 265);
}

#[test]
fn last_granule_position() {
    let data = [
        page(7, 1000, false, &[b"a"]),
        page(7, 44_100, false, &[b"b"]),
        page(9, 99_999, false, &[b"c"]), // other stream: ignored
    ]
    .concat();
    assert_eq!(last_granule(&data, 7), Some(44_100));
}

#[test]
fn a_large_ogg_granule_does_not_overflow_duration_conversion() {
    let mut identification = b"OpusHead".to_vec();
    identification.extend_from_slice(&[1, 2]);
    identification.extend_from_slice(&0u16.to_le_bytes());
    identification.extend_from_slice(&48_000u32.to_le_bytes());
    identification.extend_from_slice(&[0; 3]);
    let samples = i64::MAX as u64;
    let bytes = page(7, samples, false, &[&identification]);
    let fixture = super::super::test_support::BinaryFile::new("opus", &bytes);
    let tags = fixture.read().unwrap();
    assert_eq!(tags.properties.duration_ms, Some(samples / 48));
}
