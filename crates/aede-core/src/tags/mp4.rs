//! MP4 / M4A container: walk over ISO-BMFF boxes, iTunes atoms.
//!
//! It carries AAC (lossy) as well as ALAC (lossless); the actual codec is read
//! from the `stsd` sample description.
//!
//! Reference: ISO/IEC 14496-12.

use std::fs::File;

use super::bytes::{Cursor, duration_ms, read_at, read_at_most};
use super::{RawTags, TagError};

/// Maximum nesting depth: a guard against malicious or circular files.
const MAX_DEPTH: u8 = 12;
/// Maximum size of a chunk loaded into memory (cover art included).
const MAX_BOX_LOAD: u64 = 32 * 1024 * 1024;

/// Reads the tags and the audio properties of an open MP4 / M4A file.
///
/// `file_size` bounds the box walk, since a top-level box may declare a length
/// running past the end of the file. Sizes shorter than their header or outside
/// their enclosing range are malformed. Metadata beyond the nesting limit is
/// skipped, and durations that cannot fit in milliseconds remain unknown.
/// When the `stsd` description names no codec, the stream falls back to `aac`,
/// the overwhelmingly common case.
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

    while let Some(BoxRange {
        kind,
        body,
        end: body_end,
    }) = read_box_range(file, offset, end)?
    {
        match &kind {
            // Pure containers: descend straight into them.
            b"moov" | b"mdia" | b"minf" | b"stbl" | b"udta" => {
                walk(file, body, body_end, depth + 1, tags)?;
            }
            // Video and chapter tracks have sample descriptions too. Their
            // layouts must never be interpreted as AudioSampleEntry fields.
            b"trak" if track_is_audio(file, body, body_end)? => {
                walk(file, body, body_end, depth + 1, tags)?;
            }
            // `meta` carries 4 bytes of version/flags before its children.
            b"meta" => {
                let children = body
                    .checked_add(4)
                    .filter(|&start| start <= body_end)
                    .ok_or(TagError::Malformed("truncated MP4 meta flags"))?;
                walk(file, children, body_end, depth + 1, tags)?;
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

struct BoxRange {
    kind: [u8; 4],
    body: u64,
    end: u64,
}

fn read_box_range(file: &mut File, offset: u64, end: u64) -> Result<Option<BoxRange>, TagError> {
    if end.saturating_sub(offset) < 8 {
        return Ok(None);
    }
    let header = read_at(file, offset, 8)?;
    let mut size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
    let kind = [header[4], header[5], header[6], header[7]];
    let mut body = offset + 8;
    if size == 1 {
        if end - offset < 16 {
            return Err(TagError::Malformed("truncated extended MP4 box header"));
        }
        let large = read_at(file, offset + 8, 8)?;
        size = u64::from_be_bytes([
            large[0], large[1], large[2], large[3], large[4], large[5], large[6], large[7],
        ]);
        body = offset + 16;
    } else if size == 0 {
        size = end - offset;
    }
    let end = offset
        .checked_add(size)
        .filter(|&limit| size >= body - offset && limit <= end)
        .ok_or(TagError::Malformed("invalid MP4 box size"))?;
    Ok(Some(BoxRange { kind, body, end }))
}

fn track_is_audio(file: &mut File, start: u64, end: u64) -> Result<bool, TagError> {
    let mut offset = start;
    while let Some(track_child) = read_box_range(file, offset, end)? {
        if &track_child.kind == b"mdia" {
            let mut media_offset = track_child.body;
            while let Some(media_child) = read_box_range(file, media_offset, track_child.end)? {
                if &media_child.kind == b"hdlr" {
                    let handler = read_at_most(
                        file,
                        media_child.body,
                        (media_child.end - media_child.body).min(12) as usize,
                    )?;
                    return Ok(handler.get(8..12) == Some(b"soun"));
                }
                media_offset = media_child.end;
            }
        }
        offset = track_child.end;
    }
    Ok(false)
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
        tags.properties.duration_ms = duration_ms(duration, timescale);
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
    let Some(body_size) = entry_size.checked_sub(8) else {
        return;
    };
    let Some(format) = c.take(4) else { return };
    let format = [format[0], format[1], format[2], format[3]];
    // The loaded stsd window may stop before a large entry ends. Preserve
    // readable header fields, but never borrow bytes beyond a shorter entry.
    let Some(body) = c.take((body_size as usize).min(c.remaining())) else {
        return;
    };
    let mut c = Cursor::new(body);

    // Each selected audio description stands on its own. A later AAC or
    // unknown codec must not inherit ALAC precision or missing stream fields.
    tags.properties.codec.clear();
    tags.properties.lossless = false;
    tags.properties.channels = None;
    tags.properties.sample_rate = None;
    tags.properties.bit_depth = None;
    tags.properties.bitrate_kbps = None;

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
    // All fields and child boxes share the bounded sample-entry body, so a
    // following description cannot supply either header fields or a cookie.
    let limit = body.len();
    while c.position() + 8 <= limit {
        let Some(size) = c.u32_be() else { break };
        let Some(kind) = c.take(4) else { break };
        if size < 8 {
            break;
        }
        let payload_len = size as usize - 8;
        if payload_len > limit.saturating_sub(c.position()) {
            break;
        }
        // Codec-specific configuration cannot override another format's fields.
        if kind == b"alac" && format == *b"alac" {
            let Some(cookie) = c.take(payload_len) else {
                break;
            };
            read_alac_cookie(cookie, tags);
            break;
        }
        if kind == b"esds" && format == *b"mp4a" {
            let Some(config) = c.take(payload_len) else {
                break;
            };
            if let Some(rate) = high_aac_rate(config) {
                tags.properties.sample_rate = Some(rate);
            }
            continue;
        }
        c.skip(payload_len);
    }
}

// The 16.16 sample-entry field cannot represent rates above 65535 Hz. Read
// those from AAC's decoder configuration. For lower rates retain the existing
// entry: implicit HE-AAC can declare a core rate below its actual output rate,
// and parsing that extension requires more than this metadata correction.
fn high_aac_rate(data: &[u8]) -> Option<u32> {
    use symphonia::core::io::{BitReaderLtr, ReadBitsLtr};

    fn frequency(bits: &mut BitReaderLtr<'_>) -> Option<u32> {
        const RATES: [u32; 13] = [
            96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
            8_000, 7_350,
        ];
        let index = bits.read_bits_leq32(4).ok()?;
        if index == 15 {
            bits.read_bits_leq32(24).ok().filter(|rate| *rate > 0)
        } else {
            RATES.get(index as usize).copied()
        }
    }

    let mut outer = Cursor::new(data);
    if outer.u32_be()? != 0 {
        return None;
    }
    let mut elementary = Cursor::new(descriptor(outer.take(outer.remaining())?, 3)?);
    elementary.u16_be()?;
    let flags = elementary.u8()?;
    if flags & 0x80 != 0 {
        elementary.take(2)?;
    }
    if flags & 0x40 != 0 {
        let length = elementary.u8()? as usize;
        elementary.take(length)?;
    }
    if flags & 0x20 != 0 {
        elementary.take(2)?;
    }
    let mut decoder = Cursor::new(descriptor(elementary.take(elementary.remaining())?, 4)?);
    if !matches!(decoder.u8()?, 0x40 | 0x66 | 0x67 | 0x68) {
        return None;
    }
    decoder.take(12)?; // stream type, buffer size, maximum/average bitrate
    let config = descriptor(decoder.take(decoder.remaining())?, 5)?;
    let mut bits = BitReaderLtr::new(config);
    let mut object = bits.read_bits_leq32(5).ok()?;
    let mut rate = frequency(&mut bits)?;
    bits.read_bits_leq32(4).ok()?; // channel configuration must be present
    if matches!(object, 5 | 29) {
        rate = frequency(&mut bits)?;
        object = bits.read_bits_leq32(5).ok()?;
    }
    // Only the AAC object types whose frequency prefix is handled here.
    (matches!(object, 1..=4 | 17 | 19 | 20 | 23) && rate > u16::MAX as u32).then_some(rate)
}

// MPEG-4 descriptor sizes carry seven bits in at most four bytes. Each level
// is sliced before inspecting children; no recursive walk or resynchronisation
// into an unrelated descriptor can turn truncated data into a frequency.
fn descriptor(data: &[u8], wanted: u8) -> Option<&[u8]> {
    let mut cursor = Cursor::new(data);
    while cursor.remaining() > 0 {
        let kind = cursor.u8()?;
        let mut size = 0usize;
        let mut complete = false;
        for _ in 0..4 {
            let part = cursor.u8()?;
            size = (size << 7) | usize::from(part & 0x7f);
            if part & 0x80 == 0 {
                complete = true;
                break;
            }
        }
        if !complete {
            return None;
        }
        let value = cursor.take(size)?;
        if kind == wanted {
            return Some(value);
        }
    }
    None
}

fn read_alac_cookie(data: &[u8], tags: &mut RawTags) {
    let Some(config) = data
        .get(4..)
        .and_then(aede_audio::decode::read_alac_configuration)
    else {
        return;
    };
    let bit_depth = config.bit_depth;
    let channels = config.channels;
    let avg_bitrate = config.average_bitrate;
    let sample_rate = config.sample_rate;

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

// Keep raw aliases until insertion: normalizing a release-track ID twice
// would reinterpret it as the recording identifier.
fn freeform_key(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    match lower.as_str() {
        "musicbrainz album id" => "musicbrainz_albumid".into(),
        "musicbrainz artist id" => "musicbrainz_artistid".into(),
        "musicbrainz album artist id" => "musicbrainz_albumartistid".into(),
        "musicbrainz release group id" => "musicbrainz_releasegroupid".into(),
        "musicbrainz track id" => "musicbrainz_recordingid".into(),
        "musicbrainz release track id" => "musicbrainz_releasetrackid".into(),
        "musicbrainz work id" => "musicbrainz_workid".into(),
        "musicbrainz label id" => "musicbrainz_labelid".into(),
        "acoustid id" => "acoustid_id".into(),
        "catalognumber" => "catalognumber".into(),
        "label" => "label".into(),
        "isrc" => "isrc".into(),
        "barcode" => "barcode".into(),
        other => other.to_string(),
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
#[path = "mp4_tests.rs"]
mod tests;
