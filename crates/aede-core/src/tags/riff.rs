//! WAV (RIFF) and AIFF (IFF).
//!
//! Two very similar chunk containers, one little-endian, the other
//! big-endian. Both can carry a full ID3v2 chunk on top of their own fields,
//! which are historically poor.

use std::fs::File;

use super::bytes::{Cursor, extended80_to_f64, read_at_most};
use super::{RawTags, TagError};

/// Maximum size of a metadata chunk loaded into memory.
const MAX_META_CHUNK: usize = 16 * 1024 * 1024;

/// Reads the tags and the audio properties of an open WAV file.
///
/// `file_size` bounds the chunk walk rather than the `RIFF` header, which is
/// routinely wrong on files written by a recorder that never came back to fix
/// it. Tags are gathered from the `LIST`/`INFO` chunk and from an embedded
/// ID3v2 chunk alike; both contribute to the same fields.
pub fn read_wav(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let mut tags = RawTags::default();
    tags.properties.container = "wav".into();
    tags.properties.codec = "pcm".into();
    tags.properties.lossless = true;

    let mut byte_rate = 0u32;
    let mut offset = 12u64; // "RIFF" + size + "WAVE"

    while offset + 8 <= file_size {
        let header = read_at_most(file, offset, 8)?;
        if header.len() < 8 {
            break;
        }
        let id = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let body = offset + 8;
        if body + size > file_size + 1 {
            break;
        }

        match &id {
            b"fmt " => {
                let data = read_at_most(file, body, size.min(40) as usize)?;
                let mut c = Cursor::new(&data);
                let format = c.u16_le().unwrap_or(1);
                let channels = c.u16_le().unwrap_or(0);
                let rate = c.u32_le().unwrap_or(0);
                byte_rate = c.u32_le().unwrap_or(0);
                c.skip(2); // block alignment
                let bits = c.u16_le().unwrap_or(0);

                if channels > 0 {
                    tags.properties.channels = Some(channels);
                }
                if rate > 0 {
                    tags.properties.sample_rate = Some(rate);
                }
                if bits > 0 {
                    tags.properties.bit_depth = Some(bits);
                }
                tags.properties.codec = match format {
                    1 => "pcm".into(),
                    3 => "pcm_float".into(),
                    0xFFFE => "pcm".into(),
                    other => format!("wav_{other:#x}"),
                };
                tags.properties.lossless = matches!(format, 1 | 3 | 0xFFFE);
            }
            b"data" if byte_rate > 0 => {
                tags.properties.duration_ms = Some(size * 1000 / byte_rate as u64);
            }
            b"LIST" => {
                let capped = size.min(MAX_META_CHUNK as u64) as usize;
                let data = read_at_most(file, body, capped)?;
                if data.len() >= 4 && &data[..4] == b"INFO" {
                    read_info_list(&data[4..], &mut tags);
                }
            }
            b"id3 " | b"ID3 " => {
                let capped = size.min(MAX_META_CHUNK as u64) as usize;
                let data = read_at_most(file, body, capped)?;
                super::id3::read_id3v2_buffer(&data, &mut tags);
            }
            _ => {}
        }

        // RIFF chunks are aligned on an even number of bytes.
        offset = body + size + (size % 2);
    }

    if let Some(rate) = byte_rate.checked_mul(8)
        && rate > 0
    {
        tags.properties.bitrate_kbps = Some(rate / 1000);
    }
    Ok(tags)
}

/// `LIST/INFO` chunk: the handful of standard WAV fields.
fn read_info_list(data: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(data);
    while c.remaining() >= 8 {
        let Some(id) = c.take(4) else { break };
        let id = [id[0], id[1], id[2], id[3]];
        let Some(size) = c.u32_le() else { break };
        let size = size as usize;
        if size > c.remaining() {
            break;
        }
        let Some(value) = c.utf8(size) else { break };
        if size % 2 == 1 {
            c.skip(1);
        }
        let value = value.trim_end_matches('\0').trim().to_string();
        let key = match &id {
            b"INAM" => "title",
            b"IART" => "artist",
            b"IPRD" => "album",
            b"ICRD" => "date",
            b"IGNR" => "genre",
            b"ITRK" => "tracknumber",
            b"ICMT" => "comment",
            b"IPUB" => "label",
            b"ICOP" => "copyright",
            b"IENG" => "engineer",
            b"ISFT" => "encodedby",
            _ => continue,
        };
        tags.insert(key, value);
    }
}

/// Reads the tags and the audio properties of an open AIFF or AIFC file.
///
/// `file_size` bounds the chunk walk, the `FORM` header being no more reliable
/// here than in WAV. The native `NAME`, `AUTH` and `ANNO` chunks carry very
/// little, so most real files are described by their `ID3 ` chunk. An AIFC
/// compression name in `COMM` overrides the `pcm` default and marks the stream
/// as lossy.
pub fn read_aiff(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let mut tags = RawTags::default();
    tags.properties.container = "aiff".into();
    tags.properties.codec = "pcm".into();
    tags.properties.lossless = true;

    let mut offset = 12u64; // "FORM" + size + "AIFF"

    while offset + 8 <= file_size {
        let header = read_at_most(file, offset, 8)?;
        if header.len() < 8 {
            break;
        }
        let id = [header[0], header[1], header[2], header[3]];
        let size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as u64;
        let body = offset + 8;
        if body + size > file_size + 1 {
            break;
        }

        match &id {
            b"COMM" => {
                let data = read_at_most(file, body, size.min(64) as usize)?;
                let mut c = Cursor::new(&data);
                let channels = c.u16_be().unwrap_or(0);
                let frames = c.u32_be().unwrap_or(0);
                let bits = c.u16_be().unwrap_or(0);
                let rate = c.take(10).and_then(extended80_to_f64).unwrap_or(0.0);

                if channels > 0 {
                    tags.properties.channels = Some(channels);
                }
                if bits > 0 {
                    tags.properties.bit_depth = Some(bits);
                }
                if rate > 0.0 {
                    tags.properties.sample_rate = Some(rate.round() as u32);
                    tags.properties.duration_ms = Some((frames as f64 * 1000.0 / rate) as u64);
                    let bitrate = rate * channels as f64 * bits as f64 / 1000.0;
                    tags.properties.bitrate_kbps = Some(bitrate as u32);
                }
                // AIFC: the real codec follows, on 4 bytes.
                if let Some(codec) = c.take(4) {
                    let name = String::from_utf8_lossy(codec).trim().to_lowercase();
                    if !name.is_empty() && name != "none" && name != "sowt" {
                        tags.properties.codec = name;
                        tags.properties.lossless = false;
                    }
                }
            }
            b"NAME" => insert_iff_text(file, body, size, "title", &mut tags)?,
            b"AUTH" => insert_iff_text(file, body, size, "artist", &mut tags)?,
            b"ANNO" => insert_iff_text(file, body, size, "comment", &mut tags)?,
            b"(c) " => insert_iff_text(file, body, size, "copyright", &mut tags)?,
            b"ID3 " | b"id3 " => {
                let capped = size.min(MAX_META_CHUNK as u64) as usize;
                let data = read_at_most(file, body, capped)?;
                super::id3::read_id3v2_buffer(&data, &mut tags);
            }
            _ => {}
        }

        offset = body + size + (size % 2);
    }
    Ok(tags)
}

fn insert_iff_text(
    file: &mut File,
    offset: u64,
    size: u64,
    key: &str,
    tags: &mut RawTags,
) -> Result<(), TagError> {
    let data = read_at_most(file, offset, size.min(4096) as usize)?;
    tags.insert(key, String::from_utf8_lossy(&data).into_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_info_list() {
        let mut data = Vec::new();
        for (id, value) in [(b"INAM", "So What"), (b"IART", "Miles Davis")] {
            data.extend_from_slice(id);
            data.extend_from_slice(&(value.len() as u32).to_le_bytes());
            data.extend_from_slice(value.as_bytes());
            if value.len() % 2 == 1 {
                data.push(0);
            }
        }
        let mut tags = RawTags::default();
        read_info_list(&data, &mut tags);
        assert_eq!(tags.first("title"), Some("So What"));
        assert_eq!(tags.first("artist"), Some("Miles Davis"));
    }

    #[test]
    fn truncated_info_list_does_not_panic() {
        let mut data = b"INAM".to_vec();
        data.extend_from_slice(&999u32.to_le_bytes()); // dishonest size
        data.extend_from_slice(b"short");
        let mut tags = RawTags::default();
        read_info_list(&data, &mut tags);
        assert!(tags.is_empty());
    }
}
