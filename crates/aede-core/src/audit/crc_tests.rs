use super::*;

#[test]
fn crc8_matches_the_reference_vector() {
    // "123456789" is the check value every CRC catalogue publishes.
    assert_eq!(crc8(b"123456789"), 0xF4);
    assert_eq!(crc8(b""), 0x00, "an empty input leaves the seed");
}

#[test]
fn crc16_matches_the_reference_vector() {
    // CRC-16/UMTS, the variant FLAC frames use.
    assert_eq!(crc16(b"123456789"), 0xFEE8);
}

#[test]
fn crc32_matches_the_ogg_variant() {
    // CRC-32/MPEG-2 without the final inversion; this is not the CRC-32 of
    // zip, which would give 0xCBF43926 here.
    assert_eq!(crc32_ogg(b"123456789"), 0x89A1_897F);
}

#[test]
fn a_single_flipped_bit_changes_every_result() {
    let clean = b"the quick brown fox";
    let mut damaged = *clean;
    damaged[7] ^= 0x01;
    assert_ne!(crc8(clean), crc8(&damaged));
    assert_ne!(crc16(clean), crc16(&damaged));
    assert_ne!(crc32_ogg(clean), crc32_ogg(&damaged));
}
