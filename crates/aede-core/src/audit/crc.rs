//! The three cyclic redundancy checks the audio containers carry.
//!
//! Each is defined by its polynomial and by whether the bits are reflected, and
//! the three used here disagree on both counts — so they are written out rather
//! than derived from one another.
//!
//! | Where | Width | Polynomial | Covers |
//! |---|---|---|---|
//! | FLAC frame header | 8 | `0x07` | the header, up to the byte before the CRC |
//! | FLAC frame | 16 | `0x8005` | the whole frame, up to the two bytes before the CRC |
//! | Ogg page | 32 | `0x04C11DB7` | the page, its own CRC field read as zero |
//!
//! Tables are built at first use rather than written out: 256 entries computed
//! in a few microseconds are easier to trust than 256 constants copied by hand.

use std::sync::OnceLock;

/// CRC-8 as FLAC uses it: polynomial `0x07`, no reflection, zero seed.
pub fn crc8(data: &[u8]) -> u8 {
    static TABLE: OnceLock<[u8; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u8; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let mut value = index as u8;
            for _ in 0..8 {
                value = if value & 0x80 != 0 {
                    (value << 1) ^ 0x07
                } else {
                    value << 1
                };
            }
            *entry = value;
        }
        table
    });

    data.iter()
        .fold(0u8, |crc, &byte| table[(crc ^ byte) as usize])
}

/// CRC-16 as FLAC uses it: polynomial `0x8005`, no reflection, zero seed.
pub fn crc16(data: &[u8]) -> u16 {
    static TABLE: OnceLock<[u16; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u16; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let mut value = (index as u16) << 8;
            for _ in 0..8 {
                value = if value & 0x8000 != 0 {
                    (value << 1) ^ 0x8005
                } else {
                    value << 1
                };
            }
            *entry = value;
        }
        table
    });

    data.iter().fold(0u16, |crc, &byte| {
        (crc << 8) ^ table[((crc >> 8) as u8 ^ byte) as usize]
    })
}

/// CRC-32 as Ogg uses it: polynomial `0x04C11DB7`, **no** reflection and no
/// final inversion — which is what sets it apart from the CRC-32 of zip and
/// PNG, and why a general-purpose implementation cannot be reused here.
pub fn crc32_ogg(data: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0u32; 256];
        for (index, entry) in table.iter_mut().enumerate() {
            let mut value = (index as u32) << 24;
            for _ in 0..8 {
                value = if value & 0x8000_0000 != 0 {
                    (value << 1) ^ 0x04C1_1DB7
                } else {
                    value << 1
                };
            }
            *entry = value;
        }
        table
    });

    data.iter().fold(0u32, |crc, &byte| {
        (crc << 8) ^ table[((crc >> 24) as u8 ^ byte) as usize]
    })
}

#[cfg(test)]
mod tests {
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
}
