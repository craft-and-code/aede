use super::*;

#[path = "cast_discovery_test_support.rs"]
mod fixtures;

#[test]
fn discovery_only_associates_complete_cast_records_with_the_responding_peer() {
    let peer = "192.168.1.20:5353".parse().unwrap();
    let packet = fixtures::packet([192, 168, 1, 20]);
    let devices = parse(&packet, peer).unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].location, "192.168.1.20:8009");
    assert_eq!(devices[0].name, "Speaker");
    assert_eq!(devices[0].protocols, ["googlecast"]);
    assert!(
        parse(&fixtures::packet([192, 168, 1, 21]), peer)
            .unwrap()
            .is_empty()
    );
    for cut in 0..packet.len() {
        assert!(parse(&packet[..cut], peer).is_err(), "{cut}");
    }
    for peer in ["8.8.8.8:5353", "192.168.1.20:1234", "[::1]:5353"] {
        assert!(parse(&packet, peer.parse().unwrap()).is_err());
    }
}

#[test]
fn dns_compression_truncation_and_record_budgets_are_defensive() {
    let mut compressed = Vec::new();
    fixtures::dns_name(&mut compressed, "host.local");
    let mut offset = compressed.len();
    compressed.extend_from_slice(&[0xc0, 0]);
    assert_eq!(name(&compressed, &mut offset).unwrap(), "host.local");
    assert_eq!(offset, compressed.len());
    for invalid in [
        vec![0xc0, 0],
        vec![0xc0],
        vec![0x40],
        vec![3, b'a'],
        vec![1, 0xff, 0],
    ] {
        assert!(name(&invalid, &mut 0).is_err());
    }
    let mut packet = fixtures::packet([192, 168, 1, 20]);
    packet[7] = 129;
    assert!(parse(&packet, "192.168.1.20:5353".parse().unwrap()).is_err());
    packet = fixtures::packet([192, 168, 1, 20]);
    packet[2] |= 2;
    assert!(parse(&packet, "192.168.1.20:5353".parse().unwrap()).is_err());
    assert!(friendly(b"\x08fn=x").is_err());
    let query = query();
    let mut offset = 12;
    assert_eq!(name(&query, &mut offset).unwrap(), SERVICE);
    assert_eq!(&query[offset..], &[0, 12, 0x80, 1]);
}
