//! Reading of FLAC files: STREAMINFO block (properties) and VORBIS_COMMENT
//! block (tags).
//!
//! Reference: <https://xiph.org/flac/format.html>

use std::fs::File;

use super::bytes::{Cursor, read_at};
use super::{RawTags, TagError};

const BLOCK_STREAMINFO: u8 = 0;
const BLOCK_VORBIS_COMMENT: u8 = 4;
const BLOCK_PICTURE: u8 = 6;

/// Reads the tags and the audio properties of an open FLAC file.
///
/// `file_size` serves to estimate the bitrate from the length of the audio
/// data, which the format never states. An ID3v2 tag placed ahead of the
/// `fLaC` signature by an old encoder is skipped; if no signature turns up
/// there either, the call fails with [`TagError::UnrecognizedFormat`].
pub fn read(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let mut tags = RawTags::default();
    tags.properties.container = "flac".into();
    tags.properties.codec = "flac".into();
    tags.properties.lossless = true;

    // Some encoders place an ID3v2 tag before the fLaC signature.
    let start = super::id3::skip_id3v2(file)?;

    let header = read_at(file, start, 4)?;
    if &header[..4] != b"fLaC" {
        return Err(TagError::UnrecognizedFormat);
    }

    let mut offset = start + 4;
    let mut audio_start = offset;

    loop {
        if offset + 4 > file_size {
            break;
        }
        let head = read_at(file, offset, 4)?;
        let is_last = head[0] & 0x80 != 0;
        let block_type = head[0] & 0x7F;
        let length = ((head[1] as u64) << 16) | ((head[2] as u64) << 8) | head[3] as u64;
        offset += 4;

        if offset + length > file_size {
            return Err(TagError::Malformed("truncated FLAC metadata block"));
        }

        match block_type {
            BLOCK_STREAMINFO => {
                let body = read_at(file, offset, length.min(34) as usize)?;
                read_streaminfo(&body, &mut tags)?;
            }
            BLOCK_VORBIS_COMMENT => {
                // Guard: an oversized comment block is suspect.
                let capped = length.min(8 * 1024 * 1024) as usize;
                let body = read_at(file, offset, capped)?;
                parse_vorbis_comment(&body, &mut tags);
            }
            BLOCK_PICTURE => tags.has_embedded_art = true,
            _ => {}
        }

        offset += length;
        audio_start = offset;
        if is_last {
            break;
        }
    }

    // Actual bitrate: size of the audio alone, excluding metadata.
    if let Some(ms) = tags.properties.duration_ms
        && ms > 0
        && file_size > audio_start
    {
        let audio_bytes = file_size - audio_start;
        tags.properties.bitrate_kbps = Some(((audio_bytes * 8) / ms) as u32);
    }

    Ok(tags)
}

/// STREAMINFO: 34 bytes including a 64-bit field that packs sample rate
/// (20 b), channels (3 b), bit depth (5 b) and the total number of samples
/// (36 b).
fn read_streaminfo(body: &[u8], tags: &mut RawTags) -> Result<(), TagError> {
    let mut c = Cursor::new(body);
    c.skip(2 + 2 + 3 + 3); // blocksize min/max, framesize min/max
    let packed = c
        .u64_be()
        .ok_or(TagError::Malformed("STREAMINFO too short"))?;

    let sample_rate = (packed >> 44) as u32 & 0x0F_FFFF;
    let channels = ((packed >> 41) & 0x07) as u16 + 1;
    let bit_depth = ((packed >> 36) & 0x1F) as u16 + 1;
    let total_samples = packed & 0x0F_FFFF_FFFF;

    if sample_rate == 0 {
        return Err(TagError::Malformed("zero sample rate"));
    }
    tags.properties.sample_rate = Some(sample_rate);
    tags.properties.channels = Some(channels);
    tags.properties.bit_depth = Some(bit_depth);
    if total_samples > 0 {
        tags.properties.duration_ms = Some(total_samples * 1000 / sample_rate as u64);
    }
    Ok(())
}

/// Vorbis comment block, shared by FLAC, Ogg Vorbis and Opus.
///
/// Format: `vendor_len` (u32 LE) + vendor, then `count` (u32 LE) followed by
/// that many entries of `len` (u32 LE) + `KEY=value` in UTF-8.
pub(crate) fn parse_vorbis_comment(body: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(body);
    let Some(vendor_len) = c.u32_le() else { return };
    c.skip(vendor_len as usize);
    let Some(count) = c.u32_le() else { return };

    // An absurd count signals a corrupted block: stop cleanly.
    let count = count.min(10_000);
    for _ in 0..count {
        let Some(len) = c.u32_le() else { return };
        if len as usize > c.remaining() {
            return;
        }
        let Some(entry) = c.utf8(len as usize) else {
            return;
        };
        if let Some((key, value)) = entry.split_once('=') {
            if key.eq_ignore_ascii_case("metadata_block_picture") {
                tags.has_embedded_art = true;
                continue;
            }
            tags.insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
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
}
