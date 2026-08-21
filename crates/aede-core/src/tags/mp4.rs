//! MP4 / M4A container: walk over ISO-BMFF boxes, iTunes atoms.
//!
//! It carries AAC (lossy) as well as ALAC (lossless); the actual codec is read
//! from the `stsd` sample description.
//!
//! Reference: ISO/IEC 14496-12.

use std::fs::File;

use super::bytes::{Cursor, read_at, read_at_most};
use super::{RawTags, TagError};

/// Maximum nesting depth: a guard against malicious or circular files.
const MAX_DEPTH: u8 = 12;
/// Maximum size of a chunk loaded into memory (cover art included).
const MAX_BOX_LOAD: u64 = 32 * 1024 * 1024;

/// Reads the tags and the audio properties of an open MP4 / M4A file.
///
/// `file_size` bounds the box walk, since a top-level box may declare a length
/// running past the end of the file. When the `stsd` description names no
/// codec, the stream falls back to `aac`, the overwhelmingly common case.
pub fn read(file: &mut File, file_size: u64) -> Result<RawTags, TagError> {
    let mut tags = RawTags::default();
    tags.properties.container = "mp4".into();

    walk(file, 0, file_size, 0, &mut tags)?;

    if tags.properties.codec.is_empty() {
        tags.properties.codec = "aac".into();
    }
    Ok(tags)
}

/// Walks the boxes of a `[start, end)` range.
fn walk(
    file: &mut File,
    start: u64,
    end: u64,
    depth: u8,
    tags: &mut RawTags,
) -> Result<(), TagError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let mut offset = start;

    while offset + 8 <= end {
        let header = read_at(file, offset, 8)?;
        let mut size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let kind = [header[4], header[5], header[6], header[7]];
        let mut body = offset + 8;

        if size == 1 {
            // Size extended to 64 bits.
            let large = read_at(file, offset + 8, 8)?;
            size = u64::from_be_bytes([
                large[0], large[1], large[2], large[3], large[4], large[5], large[6], large[7],
            ]);
            body = offset + 16;
        } else if size == 0 {
            size = end - offset; // the box runs to the end
        }

        if size < 8 || offset + size > end {
            break; // inconsistent structure: stop without failing
        }
        let body_end = offset + size;

        match &kind {
            // Pure containers: descend straight into them.
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"udta" => {
                walk(file, body, body_end, depth + 1, tags)?;
            }
            // `meta` carries 4 bytes of version/flags before its children.
            b"meta" => {
                walk(file, body + 4, body_end, depth + 1, tags)?;
            }
            b"mvhd" => {
                let data = read_at_most(file, body, (body_end - body).min(120) as usize)?;
                read_mvhd(&data, tags);
            }
            b"stsd" => {
                let len = (body_end - body).min(4096) as usize;
                let data = read_at_most(file, body, len)?;
                read_stsd(&data, tags);
            }
            b"ilst" if body_end - body <= MAX_BOX_LOAD => {
                let data = read_at_most(file, body, (body_end - body) as usize)?;
                read_ilst(&data, tags);
            }
            _ => {}
        }

        offset = body_end;
    }
    Ok(())
}

fn read_mvhd(data: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(data);
    let Some(version) = c.u8() else { return };
    c.skip(3); // flags
    let (timescale, duration) = if version == 1 {
        c.skip(16);
        match (c.u32_be(), c.u64_be()) {
            (Some(t), Some(d)) => (t, d),
            _ => return,
        }
    } else {
        c.skip(8);
        match (c.u32_be(), c.u32_be()) {
            (Some(t), Some(d)) => (t, d as u64),
            _ => return,
        }
    };
    if timescale > 0 && duration > 0 {
        tags.properties.duration_ms = Some(duration * 1000 / timescale as u64);
    }
}

/// Sample description: gives the codec, the channels, the sample rate and, for
/// ALAC, the real bit depth.
fn read_stsd(data: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(data);
    c.skip(4); // version + flags
    let Some(entries) = c.u32_be() else { return };
    if entries == 0 {
        return;
    }
    let Some(entry_size) = c.u32_be() else { return };
    let Some(format) = c.take(4) else { return };
    let format = [format[0], format[1], format[2], format[3]];

    match &format {
        b"alac" => {
            tags.properties.codec = "alac".into();
            tags.properties.lossless = true;
        }
        b"mp4a" => {
            tags.properties.codec = "aac".into();
            tags.properties.lossless = false;
        }
        b"Opus" => {
            tags.properties.codec = "opus".into();
            tags.properties.lossless = false;
        }
        b"fLaC" => {
            tags.properties.codec = "flac".into();
            tags.properties.lossless = true;
        }
        other => {
            tags.properties.codec = String::from_utf8_lossy(other).trim().to_lowercase();
        }
    }

    // AudioSampleEntry
    c.skip(6); // reserved
    c.skip(2); // data reference index
    let Some(version) = c.u16_be() else { return };
    c.skip(2 + 4); // revision, vendor
    let channels = c.u16_be();
    let sample_size = c.u16_be();
    c.skip(2 + 2); // compression identifier, packet size
    let sample_rate_fixed = c.u32_be();

    if let Some(ch) = channels
        && ch > 0
        && ch <= 64
    {
        tags.properties.channels = Some(ch);
    }
    if let Some(rate) = sample_rate_fixed {
        // 16.16 fixed point: the integer part is enough.
        let rate = rate >> 16;
        if rate > 0 {
            tags.properties.sample_rate = Some(rate);
        }
    }
    if let Some(bits) = sample_size
        && tags.properties.lossless
        && (8..=32).contains(&bits)
    {
        tags.properties.bit_depth = Some(bits);
    }
    if version == 1 {
        c.skip(16);
    }

    // The ALAC "magic cookie" gives the exact bit depth and sample rate, which
    // are more reliable than the AudioSampleEntry: it often saturates at
    // 16 bits / 44.1 kHz.
    let limit = (entry_size as usize).min(c.remaining() + c.position());
    while c.position() + 8 <= limit {
        let Some(size) = c.u32_be() else { break };
        let Some(kind) = c.take(4) else { break };
        if size < 8 {
            break;
        }
        let payload_len = (size as usize - 8).min(c.remaining());
        if kind == b"alac" {
            let Some(cookie) = c.take(payload_len) else {
                break;
            };
            read_alac_cookie(cookie, tags);
            break;
        }
        c.skip(payload_len);
    }
}

fn read_alac_cookie(data: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(data);
    c.skip(4); // version/flags
    c.skip(4); // frameLength
    c.skip(1); // compatibleVersion
    let Some(bit_depth) = c.u8() else { return };
    c.skip(3); // pb, mb, kb
    let Some(channels) = c.u8() else { return };
    c.skip(2); // maxRun
    c.skip(4); // maxFrameBytes
    let avg_bitrate = c.u32_be();
    let sample_rate = c.u32_be();

    if (8..=32).contains(&bit_depth) {
        tags.properties.bit_depth = Some(bit_depth as u16);
    }
    if channels > 0 {
        tags.properties.channels = Some(channels as u16);
    }
    if let Some(rate) = sample_rate
        && rate > 0
    {
        tags.properties.sample_rate = Some(rate);
    }
    if let Some(bits) = avg_bitrate
        && bits > 0
    {
        tags.properties.bitrate_kbps = Some(bits / 1000);
    }
}

/// iTunes atom list: each entry carries one or more `data` sub-chunks.
fn read_ilst(data: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(data);
    while c.remaining() >= 8 {
        let Some(size) = c.u32_be() else { break };
        let Some(name) = c.take(4) else { break };
        let name = [name[0], name[1], name[2], name[3]];
        if size < 8 {
            break;
        }
        let payload_len = (size as usize - 8).min(c.remaining());
        let Some(payload) = c.take(payload_len) else {
            break;
        };

        if &name == b"----" {
            read_freeform(payload, tags);
            continue;
        }
        if &name == b"covr" {
            tags.has_embedded_art = true;
            continue;
        }
        let Some(key) = atom_key(&name) else { continue };
        for (data_type, value) in data_atoms(payload) {
            apply_atom(key, data_type, value, tags);
        }
    }
}

/// Extracts the `data` sub-chunks of an atom: `(type, content)`.
fn data_atoms(payload: &[u8]) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let mut c = Cursor::new(payload);
    while c.remaining() >= 8 {
        let Some(size) = c.u32_be() else { break };
        let Some(kind) = c.take(4) else { break };
        if size < 16 {
            break;
        }
        let inner_len = (size as usize - 8).min(c.remaining());
        let start = c.position();
        c.skip(inner_len);
        if kind != b"data" {
            continue;
        }
        let inner = &payload[start..start + inner_len];
        if inner.len() < 8 {
            continue;
        }
        let data_type = u32::from_be_bytes([inner[0], inner[1], inner[2], inner[3]]) & 0x00FF_FFFF;
        out.push((data_type, &inner[8..]));
    }
    out
}

fn apply_atom(key: &str, data_type: u32, value: &[u8], tags: &mut RawTags) {
    match key {
        "tracknumber" | "discnumber" => {
            // 2 reserved bytes, then number and total on 2 bytes each.
            if value.len() >= 6 {
                let number = u16::from_be_bytes([value[2], value[3]]);
                let total = u16::from_be_bytes([value[4], value[5]]);
                if number > 0 {
                    tags.insert(key, number.to_string());
                }
                if total > 0 {
                    let total_key = if key == "tracknumber" {
                        "tracktotal"
                    } else {
                        "disctotal"
                    };
                    tags.insert(total_key, total.to_string());
                }
            }
        }
        "gnre" => {
            // Numeric genre inherited from ID3v1 (index shifted by 1).
            if value.len() >= 2 {
                let index = u16::from_be_bytes([value[0], value[1]]);
                if index > 0
                    && let Some(name) = id3v1_genre(index - 1)
                {
                    tags.insert("genre", name);
                }
            }
        }
        "compilation" => {
            let flag = value.first().copied().unwrap_or(0);
            if flag != 0 {
                tags.insert("compilation", "1");
            }
        }
        _ => match data_type {
            1 => tags.insert(key, String::from_utf8_lossy(value).into_owned()),
            21 | 22 => {
                let mut number: u64 = 0;
                for &b in value.iter().take(8) {
                    number = (number << 8) | b as u64;
                }
                tags.insert(key, number.to_string());
            }
            13 | 14 => tags.has_embedded_art = true,
            _ => tags.insert(key, String::from_utf8_lossy(value).into_owned()),
        },
    }
}

/// `----` atom: free-form key, shaped as `mean` (domain) + `name` (key).
fn read_freeform(payload: &[u8], tags: &mut RawTags) {
    let mut c = Cursor::new(payload);
    let mut key: Option<String> = None;

    while c.remaining() >= 8 {
        let Some(size) = c.u32_be() else { break };
        let Some(kind) = c.take(4) else { break };
        if size < 8 {
            break;
        }
        let inner_len = (size as usize - 8).min(c.remaining());
        let start = c.position();
        c.skip(inner_len);
        let inner = &payload[start..start + inner_len];

        match kind {
            b"name" if inner.len() > 4 => {
                key = Some(String::from_utf8_lossy(&inner[4..]).trim().to_string());
            }
            b"data" if inner.len() > 8 => {
                if let Some(ref k) = key {
                    let canonical = freeform_key(k);
                    tags.insert(
                        &canonical,
                        String::from_utf8_lossy(&inner[8..]).into_owned(),
                    );
                }
            }
            _ => {}
        }
    }
}

fn freeform_key(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match lower.as_str() {
        "musicbrainz album id" => "musicbrainz_albumid".into(),
        "musicbrainz artist id" => "musicbrainz_artistid".into(),
        "musicbrainz album artist id" => "musicbrainz_albumartistid".into(),
        "musicbrainz release group id" => "musicbrainz_releasegroupid".into(),
        "musicbrainz track id" => "musicbrainz_recordingid".into(),
        "acoustid id" => "acoustid_id".into(),
        "catalognumber" => "catalognumber".into(),
        "label" => "label".into(),
        "isrc" => "isrc".into(),
        "barcode" => "barcode".into(),
        other => super::canonical_key(other),
    }
}

fn atom_key(name: &[u8; 4]) -> Option<&'static str> {
    // The © prefix is 0xA9 in ISO-8859-1.
    Some(match name {
        b"\xa9nam" => "title",
        b"\xa9ART" => "artist",
        b"aART" => "albumartist",
        b"\xa9alb" => "album",
        b"\xa9day" => "date",
        b"\xa9gen" => "genre",
        b"gnre" => "gnre",
        b"trkn" => "tracknumber",
        b"disk" => "discnumber",
        b"\xa9wrt" => "composer",
        b"\xa9cmt" => "comment",
        b"\xa9lyr" => "lyrics",
        b"\xa9grp" => "grouping",
        b"cprt" => "copyright",
        b"cpil" => "compilation",
        b"tmpo" => "bpm",
        b"soar" => "artistsort",
        b"soaa" => "albumartistsort",
        b"soal" => "albumsort",
        b"\xa9too" => "encodedby",
        b"\xa9wrk" => "work",
        b"\xa9mvn" => "movement",
        _ => return None,
    })
}

fn id3v1_genre(index: u16) -> Option<&'static str> {
    // The ID3 table is reused rather than duplicated here: delegating keeps a
    // single source of truth for the genre names.
    super::id3::genre_name_public(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_with(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn data_box(data_type: u32, payload: &[u8]) -> Vec<u8> {
        let mut inner = data_type.to_be_bytes().to_vec();
        inner.extend_from_slice(&0u32.to_be_bytes()); // locale
        inner.extend_from_slice(payload);
        box_with(b"data", &inner)
    }

    #[test]
    fn text_atoms() {
        let mut ilst = box_with(b"\xa9nam", &data_box(1, b"So What"));
        ilst.extend(box_with(b"\xa9ART", &data_box(1, "Björk".as_bytes())));
        ilst.extend(box_with(b"\xa9alb", &data_box(1, b"Kind of Blue")));
        let mut tags = RawTags::default();
        read_ilst(&ilst, &mut tags);
        assert_eq!(tags.first("title"), Some("So What"));
        assert_eq!(tags.first("artist"), Some("Björk"));
        assert_eq!(tags.first("album"), Some("Kind of Blue"));
    }

    #[test]
    fn track_number_and_total() {
        let payload = [0u8, 0, 0, 3, 0, 9, 0, 0];
        let ilst = box_with(b"trkn", &data_box(0, &payload));
        let mut tags = RawTags::default();
        read_ilst(&ilst, &mut tags);
        assert_eq!(tags.first("tracknumber"), Some("3"));
        assert_eq!(tags.first("tracktotal"), Some("9"));
    }

    #[test]
    fn cover_art_detected() {
        let ilst = box_with(b"covr", &data_box(13, &[0xFF, 0xD8]));
        let mut tags = RawTags::default();
        read_ilst(&ilst, &mut tags);
        assert!(tags.has_embedded_art);
    }

    #[test]
    fn freeform_atom_musicbrainz() {
        let mut payload = box_with(b"mean", b"\0\0\0\0com.apple.iTunes");
        payload.extend(box_with(b"name", b"\0\0\0\0MusicBrainz Album Id"));
        payload.extend(data_box(1, b"abc-123"));
        let ilst = box_with(b"----", &payload);
        let mut tags = RawTags::default();
        read_ilst(&ilst, &mut tags);
        assert_eq!(tags.first("musicbrainz_albumid"), Some("abc-123"));
    }

    #[test]
    fn mvhd_version_0() {
        let mut payload = vec![0u8, 0, 0, 0]; // version 0 + flags
        payload.extend_from_slice(&0u32.to_be_bytes()); // creation
        payload.extend_from_slice(&0u32.to_be_bytes()); // modification
        payload.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        payload.extend_from_slice(&2500u32.to_be_bytes()); // duration
        let mut tags = RawTags::default();
        read_mvhd(&payload, &mut tags);
        assert_eq!(tags.properties.duration_ms, Some(2500));
    }
}
