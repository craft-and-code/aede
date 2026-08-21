//! Ogg container: Vorbis and Opus streams.
//!
//! The duration is read from the granule position of the last page, which
//! requires seeking back to the end of the file — this is the only format here
//! where reading is not purely sequential.
//!
//! References: RFC 3533 (Ogg), RFC 7845 (Opus in Ogg).

use std::fs::File;

use super::bytes::{Cursor, read_at_most};
use super::{RawTags, TagError};

/// Head window large enough to hold the identification header and the comment
/// block, embedded cover art included.
const HEAD_WINDOW: usize = 512 * 1024;
/// Tail window used to locate the last page.
const TAIL_WINDOW: usize = 64 * 1024;

/// Reads the tags and the audio properties of an open Ogg file.
///
/// `file_size` positions the tail window in which the last page is looked for,
/// the granule position of which gives the duration. A file with no readable
/// page in its first window fails with [`TagError::UnrecognizedFormat`].
pub fn read(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let mut tags = RawTags::default();
    tags.properties.container = "ogg".into();

    let head = read_at_most(file, 0, HEAD_WINDOW)?;
    let pages = parse_pages(&head);
    if pages.is_empty() {
        return Err(TagError::UnrecognizedFormat);
    }
    let serial = pages[0].serial;
    let packets = reassemble_packets(&pages, serial);

    let Some(identification) = packets.first() else {
        return Err(TagError::Malformed(
            "Ogg stream without identification header",
        ));
    };

    let rate_for_granule = if identification.starts_with(b"\x01vorbis") {
        read_vorbis_identification(identification, &mut tags)
    } else if identification.starts_with(b"OpusHead") {
        read_opus_identification(identification, &mut tags)
    } else if identification.starts_with(b"\x7fFLAC") {
        tags.properties.codec = "flac".into();
        tags.properties.lossless = true;
        None
    } else {
        // Speex and the rest of the Ogg family. Claiming the file and
        // returning an empty result would hide it from the fallback, which
        // knows some of them.
        return Err(TagError::UnrecognizedFormat);
    };

    // The second packet carries the comments.
    if let Some(comment) = packets.get(1) {
        if let Some(body) = comment.strip_prefix(b"\x03vorbis") {
            super::flac::parse_vorbis_comment(body, &mut tags);
        } else if let Some(body) = comment.strip_prefix(b"OpusTags") {
            super::flac::parse_vorbis_comment(body, &mut tags);
        }
    }

    // Duration: the final granule position converted to seconds.
    if let Some(rate) = rate_for_granule {
        let tail_start = file_size.saturating_sub(TAIL_WINDOW as u64);
        let tail = read_at_most(file, tail_start, TAIL_WINDOW)?;
        if let Some(granule) = last_granule(&tail, serial) {
            let samples = granule.saturating_sub(tags.pre_skip());
            if rate > 0 {
                tags.properties.duration_ms = Some(samples * 1000 / rate as u64);
            }
        }
    }

    // Working fields prefixed with `__` are not metadata.
    tags.fields.retain(|key, _| !key.starts_with("__"));

    Ok(tags)
}

trait PreSkip {
    fn pre_skip(&self) -> u64;
}

impl PreSkip for RawTags {
    /// Opus inserts a pre-skip that must be subtracted from the announced
    /// duration.
    fn pre_skip(&self) -> u64 {
        self.first("__opus_pre_skip")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    }
}

struct Page<'a> {
    serial: u32,
    /// Whether the first packet of this page continues the previous packet.
    continued: bool,
    segments: Vec<&'a [u8]>,
    /// Whether the last segment is exactly 255 bytes (incomplete packet).
    last_segment_full: bool,
}

fn parse_pages(data: &[u8]) -> Vec<Page<'_>> {
    let mut pages = Vec::new();
    let mut offset = 0usize;

    while offset + 27 <= data.len() {
        if &data[offset..offset + 4] != b"OggS" {
            // Resynchronization: look for the next capture pattern.
            match find(&data[offset + 1..], b"OggS") {
                Some(next) => {
                    offset += 1 + next;
                    continue;
                }
                None => break,
            }
        }
        let header_type = data[offset + 5];
        let serial = u32::from_le_bytes([
            data[offset + 14],
            data[offset + 15],
            data[offset + 16],
            data[offset + 17],
        ]);
        let segment_count = data[offset + 26] as usize;
        let table_start = offset + 27;
        if table_start + segment_count > data.len() {
            break;
        }
        let table = &data[table_start..table_start + segment_count];
        let mut body = table_start + segment_count;

        let mut segments = Vec::with_capacity(segment_count);
        let mut last_full = false;
        for &len in table {
            let len = len as usize;
            if body + len > data.len() {
                return pages;
            }
            segments.push(&data[body..body + len]);
            body += len;
            last_full = len == 255;
        }

        pages.push(Page {
            serial,
            continued: header_type & 0x01 != 0,
            segments,
            last_segment_full: last_full,
        });
        offset = body;
    }
    pages
}

/// Glues the segments back into logical packets. A packet ends on a segment of
/// fewer than 255 bytes.
fn reassemble_packets(pages: &[Page<'_>], serial: u32) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut pending = false;

    for page in pages.iter().filter(|p| p.serial == serial) {
        if !page.continued && pending {
            // Next page continues nothing: close whatever was left hanging.
            packets.push(std::mem::take(&mut current));
            pending = false;
        }
        for (i, segment) in page.segments.iter().enumerate() {
            current.extend_from_slice(segment);
            let is_last = i + 1 == page.segments.len();
            let ends_packet = segment.len() < 255 || (is_last && !page.last_segment_full);
            if ends_packet {
                packets.push(std::mem::take(&mut current));
                pending = false;
            } else {
                pending = true;
            }
        }
        if packets.len() >= 2 {
            break; // identification + comments are enough
        }
    }
    if pending && !current.is_empty() {
        packets.push(current);
    }
    packets
}

fn read_vorbis_identification(packet: &[u8], tags: &mut RawTags) -> Option<u32> {
    tags.properties.codec = "vorbis".into();
    tags.properties.lossless = false;

    let mut c = Cursor::new(&packet[7..]);
    c.skip(4); // version
    let channels = c.u8()?;
    let rate = c.u32_le()?;
    c.skip(4); // maximum bitrate
    let nominal = c.u32_le()?;

    if channels > 0 {
        tags.properties.channels = Some(channels as u16);
    }
    if rate > 0 {
        tags.properties.sample_rate = Some(rate);
    }
    if nominal > 0 && nominal < 10_000_000 {
        tags.properties.bitrate_kbps = Some(nominal / 1000);
    }
    Some(rate)
}

fn read_opus_identification(packet: &[u8], tags: &mut RawTags) -> Option<u32> {
    tags.properties.codec = "opus".into();
    tags.properties.lossless = false;

    let mut c = Cursor::new(&packet[8..]);
    c.skip(1); // version
    let channels = c.u8()?;
    let pre_skip = c.u16_le()?;
    let input_rate = c.u32_le()?;

    if channels > 0 {
        tags.properties.channels = Some(channels as u16);
    }
    // Opus always decodes at 48 kHz; the original sample rate is kept aside.
    tags.properties.sample_rate = Some(48_000);
    if input_rate > 0 && input_rate != 48_000 {
        tags.insert("__opus_input_rate", input_rate.to_string());
    }
    tags.insert("__opus_pre_skip", pre_skip.to_string());
    Some(48_000)
}

/// Granule position of the last page of the wanted stream.
fn last_granule(tail: &[u8], serial: u32) -> Option<u64> {
    let mut found = None;
    let mut offset = 0usize;
    while let Some(pos) = find(&tail[offset..], b"OggS") {
        let start = offset + pos;
        if start + 27 > tail.len() {
            break;
        }
        let page_serial = u32::from_le_bytes([
            tail[start + 14],
            tail[start + 15],
            tail[start + 16],
            tail[start + 17],
        ]);
        if page_serial == serial {
            let granule = u64::from_le_bytes([
                tail[start + 6],
                tail[start + 7],
                tail[start + 8],
                tail[start + 9],
                tail[start + 10],
                tail[start + 11],
                tail[start + 12],
                tail[start + 13],
            ]);
            // -1 marks a page with no completed sample.
            if granule != u64::MAX {
                found = Some(granule);
            }
        }
        offset = start + 4;
    }
    found
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
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
}
