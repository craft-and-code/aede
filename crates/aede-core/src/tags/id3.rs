//! ID3v2 (versions 2.2, 2.3 and 2.4) and ID3v1.
//!
//! This module serves MP3, but also WAV, AIFF and some FLAC files, all of
//! which may embed an ID3v2 block.
//!
//! Reference: <https://id3.org/id3v2.4.0-structure>

use std::fs::File;

use super::bytes::{Cursor, read_at, read_at_most, syncsafe};
use super::{RawTags, TagError};

/// Returns the offset of the first byte located **after** a possible ID3v2
/// tag, that is the real start of the audio stream. Returns 0 when there is no
/// tag.
pub fn skip_id3v2(file: &mut File) -> Result<u64, TagError> {
    let header = read_at_most(file, 0, 10)?;
    if header.len() < 10 || &header[..3] != b"ID3" {
        return Ok(0);
    }
    let size = syncsafe(&header[6..10]) as u64;
    let has_footer = header[5] & 0x10 != 0;
    Ok(10 + size + if has_footer { 10 } else { 0 })
}

/// Reads the ID3v2 tag at the head of the file, if any.
pub fn read_id3v2(file: &mut File, tags: &mut RawTags) -> Result<(), TagError> {
    let header = read_at_most(file, 0, 10)?;
    if header.len() < 10 || &header[..3] != b"ID3" {
        return Ok(());
    }
    let major = header[3];
    let flags = header[5];
    let size = syncsafe(&header[6..10]) as usize;
    if size == 0 || size > 64 * 1024 * 1024 {
        return Ok(());
    }

    let mut body = read_at_most(file, 10, size)?;

    // Global unsynchronisation (ID3v2.3): the bytes 0xFF 0x00 encode 0xFF.
    if flags & 0x80 != 0 {
        body = deunsynchronize(&body);
    }

    // Possible extended header: skip the size it announces.
    let mut start = 0usize;
    if flags & 0x40 != 0 && body.len() >= 4 {
        let ext_size = if major >= 4 {
            syncsafe(&body[0..4]) as usize
        } else {
            u32::from_be_bytes([body[0], body[1], body[2], body[3]]) as usize + 4
        };
        start = ext_size.min(body.len());
    }

    parse_frames(&body[start..], major, tags);
    Ok(())
}

/// Reads the ID3v1 tag from the last 128 bytes. Used only as a fallback, when
/// ID3v2 is absent or silent about a field.
pub fn read_id3v1(file: &mut File, file_size: u64, tags: &mut RawTags) -> Result<(), TagError> {
    if file_size < 128 {
        return Ok(());
    }
    let block = read_at(file, file_size - 128, 128)?;
    if &block[..3] != b"TAG" {
        return Ok(());
    }
    let field = |start: usize, len: usize| -> String {
        String::from_utf8_lossy(&block[start..start + len])
            .trim_end_matches('\0')
            .trim()
            .to_string()
    };

    for (key, value) in [
        ("title", field(3, 30)),
        ("artist", field(33, 30)),
        ("album", field(63, 30)),
        ("date", field(93, 4)),
    ] {
        if tags.first(key).is_none() {
            tags.insert(key, value);
        }
    }

    // ID3v1.1: when byte 125 is zero, byte 126 carries the track number.
    if block[125] == 0 && block[126] != 0 && tags.first("tracknumber").is_none() {
        tags.insert("tracknumber", block[126].to_string());
    }
    if tags.first("genre").is_none()
        && let Some(name) = genre_name(block[127])
    {
        tags.insert("genre", name);
    }
    Ok(())
}

/// Reads an ID3v2 block already loaded in memory (the `id3 ` chunks of WAV and
/// AIFF, where the tag is not at the head of the file).
pub(crate) fn read_id3v2_buffer(buf: &[u8], tags: &mut RawTags) {
    if buf.len() < 10 || &buf[..3] != b"ID3" {
        return;
    }
    let major = buf[3];
    let flags = buf[5];
    let size = syncsafe(&buf[6..10]) as usize;
    let end = (10 + size).min(buf.len());
    let mut body = buf[10..end].to_vec();
    if flags & 0x80 != 0 {
        body = deunsynchronize(&body);
    }
    parse_frames(&body, major, tags);
}

/// ID3v1 genre table, exposed to the other formats that reuse it (notably the
/// `gnre` atom of MP4).
pub(crate) fn genre_name_public(code: u16) -> Option<&'static str> {
    if code > u8::MAX as u16 {
        return None;
    }
    genre_name(code as u8)
}

fn deunsynchronize(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        out.push(input[i]);
        if input[i] == 0xFF && i + 1 < input.len() && input[i + 1] == 0x00 {
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn parse_frames(body: &[u8], major: u8, tags: &mut RawTags) {
    let (id_len, size_len, flags_len) = if major <= 2 { (3, 3, 0) } else { (4, 4, 2) };
    let mut c = Cursor::new(body);

    while c.remaining() >= id_len + size_len + flags_len {
        let Some(id_bytes) = c.take(id_len) else {
            break;
        };
        // A padding area marks the end of the frames.
        if id_bytes.iter().all(|&b| b == 0) {
            break;
        }
        let id: String = id_bytes.iter().map(|&b| b as char).collect();

        let Some(size_bytes) = c.take(size_len) else {
            break;
        };
        let size = match (major, size_len) {
            (4, 4) => syncsafe(size_bytes) as usize,
            (_, 4) => {
                u32::from_be_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]])
                    as usize
            }
            _ => {
                ((size_bytes[0] as usize) << 16)
                    | ((size_bytes[1] as usize) << 8)
                    | size_bytes[2] as usize
            }
        };
        let frame_flags = if flags_len == 2 {
            c.take(2)
                .map(|f| ((f[0] as u16) << 8) | f[1] as u16)
                .unwrap_or(0)
        } else {
            0
        };

        if size == 0 || size > c.remaining() {
            break;
        }
        let Some(mut payload) = c.take(size) else {
            break;
        };

        // Compressed or encrypted frame: we cannot read it, so skip it.
        let compressed = if major >= 4 {
            frame_flags & 0x0008 != 0
        } else {
            frame_flags & 0x0080 != 0
        };
        let encrypted = if major >= 4 {
            frame_flags & 0x0004 != 0
        } else {
            frame_flags & 0x0040 != 0
        };
        if compressed || encrypted {
            continue;
        }
        // Data length indicator: 4 bytes to skip.
        let owned;
        if major >= 4 && frame_flags & 0x0001 != 0 && payload.len() > 4 {
            owned = payload[4..].to_vec();
            payload = &owned;
        }

        handle_frame(&id, payload, tags);
    }
}

fn handle_frame(id: &str, payload: &[u8], tags: &mut RawTags) {
    match id {
        "APIC" | "PIC" => {
            tags.has_embedded_art = true;
        }
        "TXXX" | "TXX" => {
            let values = decode_text_frame(payload);
            if values.len() >= 2 {
                let key = user_key(&values[0]);
                for value in &values[1..] {
                    tags.insert(&key, value.clone());
                }
            }
        }
        "COMM" | "COM" | "USLT" | "ULT" => {
            // encoding(1) + language(3) + description\0 + text
            if payload.len() > 4 {
                let encoding = payload[0];
                let mut rest = vec![encoding];
                rest.extend_from_slice(&payload[4..]);
                let values = decode_text_frame(&rest);
                if let Some(text) = values.last() {
                    let key = if id.starts_with('U') {
                        "lyrics"
                    } else {
                        "comment"
                    };
                    tags.insert(key, text.clone());
                }
            }
        }
        "UFID" | "UFI" => {
            // owner\0 + binary identifier
            if let Some(sep) = payload.iter().position(|&b| b == 0) {
                let owner = String::from_utf8_lossy(&payload[..sep]).to_lowercase();
                if owner.contains("musicbrainz") {
                    let id = String::from_utf8_lossy(&payload[sep + 1..])
                        .trim()
                        .to_string();
                    tags.insert("musicbrainz_recordingid", id);
                }
            }
        }
        _ => {
            let Some(key) = frame_key(id) else { return };
            for value in decode_text_frame(payload) {
                match key {
                    "genre" => {
                        for genre in expand_genre(&value) {
                            tags.insert("genre", genre);
                        }
                    }
                    "compilation" => {
                        if value.trim() == "1" {
                            tags.insert("compilation", "1");
                        }
                    }
                    _ => tags.insert(key, value),
                }
            }
        }
    }
}

fn frame_key(id: &str) -> Option<&'static str> {
    Some(match id {
        "TIT2" | "TT2" => "title",
        "TPE1" | "TP1" => "artist",
        "TPE2" | "TP2" => "albumartist",
        "TPE3" | "TP3" => "conductor",
        "TPE4" | "TP4" => "remixer",
        "TALB" | "TAL" => "album",
        "TCON" | "TCO" => "genre",
        "TRCK" | "TRK" => "tracknumber",
        "TPOS" | "TPA" => "discnumber",
        "TCOM" | "TCM" => "composer",
        "TEXT" | "TXT" => "lyricist",
        "TPUB" | "TPB" => "label",
        "TSRC" | "TRC" => "isrc",
        "TBPM" | "TBP" => "bpm",
        "TCMP" | "TCP" => "compilation",
        "TDRC" | "TYER" | "TYE" | "TDAT" => "date",
        "TDOR" | "TORY" => "originaldate",
        "TSOP" | "TSP" => "artistsort",
        "TSO2" => "albumartistsort",
        "TSOA" => "albumsort",
        "TIT1" | "TT1" => "grouping",
        "TMED" | "TMT" => "media",
        "TLAN" | "TLA" => "language",
        "TENC" | "TEN" => "encodedby",
        "TSSE" | "TSS" => "encodersettings",
        "TOPE" | "TOA" => "originalartist",
        "TCOP" | "TCR" => "copyright",
        _ => return None,
    })
}

/// TXXX frames carry a free-form key; the known conventions (Picard,
/// foobar2000) are mapped back onto the canonical vocabulary.
fn user_key(description: &str) -> String {
    let lower = description.trim().to_lowercase();
    match lower.as_str() {
        "musicbrainz album id" => "musicbrainz_albumid".into(),
        "musicbrainz artist id" => "musicbrainz_artistid".into(),
        "musicbrainz album artist id" => "musicbrainz_albumartistid".into(),
        "musicbrainz release group id" => "musicbrainz_releasegroupid".into(),
        "musicbrainz release track id" => "musicbrainz_trackid".into(),
        "acoustid id" => "acoustid_id".into(),
        "acoustid fingerprint" => "acoustid_fingerprint".into(),
        "catalognumber" | "catalog number" => "catalognumber".into(),
        "barcode" => "barcode".into(),
        "originalyear" => "originaldate".into(),
        "totaltracks" => "tracktotal".into(),
        "totaldiscs" => "disctotal".into(),
        other => super::canonical_key(other),
    }
}

/// Decodes a text frame: one encoding byte followed by one or more values
/// separated by null bytes.
fn decode_text_frame(payload: &[u8]) -> Vec<String> {
    if payload.is_empty() {
        return Vec::new();
    }
    let encoding = payload[0];
    let data = &payload[1..];
    let raw = match encoding {
        0 => split_latin1(data),
        1 => split_utf16(data, None),
        2 => split_utf16(data, Some(true)),
        3 => split_utf8(data),
        _ => split_latin1(data),
    };
    raw.into_iter()
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn split_latin1(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == 0)
        .map(|chunk| chunk.iter().map(|&b| b as char).collect())
        .collect()
}

fn split_utf8(data: &[u8]) -> Vec<String> {
    data.split(|&b| b == 0)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

/// UTF-16 with or without BOM; the value separator is a null `u16`.
fn split_utf16(data: &[u8], force_big_endian: Option<bool>) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = Vec::<u16>::new();
    let mut i = 0usize;
    let mut big_endian = force_big_endian.unwrap_or(false);
    let mut bom_seen = force_big_endian.is_some();

    while i + 1 < data.len() {
        let (a, b) = (data[i], data[i + 1]);
        i += 2;
        if !bom_seen {
            if a == 0xFF && b == 0xFE {
                big_endian = false;
                bom_seen = true;
                continue;
            }
            if a == 0xFE && b == 0xFF {
                big_endian = true;
                bom_seen = true;
                continue;
            }
            bom_seen = true;
        }
        let unit = if big_endian {
            u16::from_be_bytes([a, b])
        } else {
            u16::from_le_bytes([a, b])
        };
        if unit == 0 {
            out.push(String::from_utf16_lossy(&current));
            current.clear();
            bom_seen = force_big_endian.is_some();
            big_endian = force_big_endian.unwrap_or(false);
        } else {
            current.push(unit);
        }
    }
    if !current.is_empty() {
        out.push(String::from_utf16_lossy(&current));
    }
    out
}

/// `TCON` accepts `Rock`, `(17)`, `(17)Rock`, or several stacked genres.
fn expand_genre(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw.trim();
    while let Some(stripped) = rest.strip_prefix('(') {
        match stripped.find(')') {
            Some(end) => {
                let code = &stripped[..end];
                if let Ok(n) = code.parse::<u8>() {
                    if let Some(name) = genre_name(n) {
                        out.push(name.to_string());
                    }
                } else if code.eq_ignore_ascii_case("RX") {
                    out.push("Remix".to_string());
                } else if code.eq_ignore_ascii_case("CR") {
                    out.push("Cover".to_string());
                }
                rest = stripped[end + 1..].trim();
            }
            None => break,
        }
    }
    if !rest.is_empty() {
        if let Ok(n) = rest.parse::<u8>()
            && let Some(name) = genre_name(n)
        {
            out.push(name.to_string());
            return out;
        }
        out.push(rest.to_string());
    }
    out
}

/// ID3v1 genre table (0 to 125), taken over as is by TCON.
fn genre_name(code: u8) -> Option<&'static str> {
    const GENRES: [&str; 126] = [
        "Blues",
        "Classic Rock",
        "Country",
        "Dance",
        "Disco",
        "Funk",
        "Grunge",
        "Hip-Hop",
        "Jazz",
        "Metal",
        "New Age",
        "Oldies",
        "Other",
        "Pop",
        "R&B",
        "Rap",
        "Reggae",
        "Rock",
        "Techno",
        "Industrial",
        "Alternative",
        "Ska",
        "Death Metal",
        "Pranks",
        "Soundtrack",
        "Euro-Techno",
        "Ambient",
        "Trip-Hop",
        "Vocal",
        "Jazz+Funk",
        "Fusion",
        "Trance",
        "Classical",
        "Instrumental",
        "Acid",
        "House",
        "Game",
        "Sound Clip",
        "Gospel",
        "Noise",
        "Alt. Rock",
        "Bass",
        "Soul",
        "Punk",
        "Space",
        "Meditative",
        "Instrumental Pop",
        "Instrumental Rock",
        "Ethnic",
        "Gothic",
        "Darkwave",
        "Techno-Industrial",
        "Electronic",
        "Pop-Folk",
        "Eurodance",
        "Dream",
        "Southern Rock",
        "Comedy",
        "Cult",
        "Gangsta Rap",
        "Top 40",
        "Christian Rap",
        "Pop/Funk",
        "Jungle",
        "Native American",
        "Cabaret",
        "New Wave",
        "Psychedelic",
        "Rave",
        "Showtunes",
        "Trailer",
        "Lo-Fi",
        "Tribal",
        "Acid Punk",
        "Acid Jazz",
        "Polka",
        "Retro",
        "Musical",
        "Rock & Roll",
        "Hard Rock",
        "Folk",
        "Folk/Rock",
        "National Folk",
        "Swing",
        "Fast-Fusion",
        "Bebop",
        "Latin",
        "Revival",
        "Celtic",
        "Bluegrass",
        "Avantgarde",
        "Gothic Rock",
        "Progressive Rock",
        "Psychedelic Rock",
        "Symphonic Rock",
        "Slow Rock",
        "Big Band",
        "Chorus",
        "Easy Listening",
        "Acoustic",
        "Humour",
        "Speech",
        "Chanson",
        "Opera",
        "Chamber Music",
        "Sonata",
        "Symphony",
        "Booty Bass",
        "Primus",
        "Porn Groove",
        "Satire",
        "Slow Jam",
        "Club",
        "Tango",
        "Samba",
        "Folklore",
        "Ballad",
        "Power Ballad",
        "Rhythmic Soul",
        "Freestyle",
        "Duet",
        "Punk Rock",
        "Drum Solo",
        "A Cappella",
        "Euro-House",
        "Dance Hall",
    ];
    GENRES.get(code as usize).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_frame(id: &str, encoding: u8, text: &[u8]) -> Vec<u8> {
        let mut payload = vec![encoding];
        payload.extend_from_slice(text);
        let mut frame = id.as_bytes().to_vec();
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&payload);
        frame
    }

    #[test]
    fn latin1_and_utf8_text_frames() {
        let mut body = text_frame("TIT2", 0, b"So What");
        body.extend(text_frame("TPE1", 3, "Miles Davis".as_bytes()));
        body.extend(text_frame("TALB", 3, "Kind of Blue".as_bytes()));
        let mut tags = RawTags::default();
        parse_frames(&body, 3, &mut tags);
        assert_eq!(tags.first("title"), Some("So What"));
        assert_eq!(tags.first("artist"), Some("Miles Davis"));
        assert_eq!(tags.first("album"), Some("Kind of Blue"));
    }

    #[test]
    fn utf16_frame_with_bom() {
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "Björk".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let body = text_frame("TPE1", 1, &utf16);
        let mut tags = RawTags::default();
        parse_frames(&body, 3, &mut tags);
        assert_eq!(tags.first("artist"), Some("Björk"));
    }

    #[test]
    fn multiple_values_v24() {
        let body = text_frame("TPE1", 3, b"Miles Davis\0John Coltrane");
        let mut tags = RawTags::default();
        parse_frames(&body, 4, &mut tags);
        assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
    }

    #[test]
    fn numeric_genres() {
        assert_eq!(expand_genre("(17)"), vec!["Rock"]);
        assert_eq!(expand_genre("(17)Rock"), vec!["Rock", "Rock"]);
        assert_eq!(expand_genre("32"), vec!["Classical"]);
        assert_eq!(expand_genre("Post-Rock"), vec!["Post-Rock"]);
        assert_eq!(expand_genre("(8)(32)"), vec!["Jazz", "Classical"]);
    }

    #[test]
    fn txxx_musicbrainz() {
        let body = text_frame("TXXX", 3, b"MusicBrainz Album Id\0abc-123");
        let mut tags = RawTags::default();
        parse_frames(&body, 4, &mut tags);
        assert_eq!(tags.first("musicbrainz_albumid"), Some("abc-123"));
    }

    #[test]
    fn unsynchronisation() {
        assert_eq!(deunsynchronize(&[0xFF, 0x00, 0xE0]), vec![0xFF, 0xE0]);
        assert_eq!(deunsynchronize(&[0x01, 0x02]), vec![0x01, 0x02]);
    }

    #[test]
    fn padding_stops_reading() {
        let mut body = text_frame("TIT2", 3, b"So What");
        body.extend(vec![0u8; 64]);
        let mut tags = RawTags::default();
        parse_frames(&body, 4, &mut tags);
        assert_eq!(tags.first("title"), Some("So What"));
    }
}
