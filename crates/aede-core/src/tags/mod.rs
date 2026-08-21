//! Reading of metadata and audio properties.
//!
//! Each format has its own submodule. They all converge on [`RawTags`], whose
//! keys are normalised on the Vorbis Comment vocabulary (`title`, `artist`,
//! `albumartist`, `date`…): it is the cleanest common denominator, and the one
//! MusicBrainz uses.
//!
//! The formats a library is actually made of are parsed here, from the
//! specifications: it is what allows the encoder delay, the ALAC magic cookie
//! and the Opus pre-skip to be extracted, none of which a general-purpose
//! library exposes. Anything else falls through to [`foreign`].

pub(crate) mod bytes;
pub mod flac;
pub mod foreign;
pub mod id3;
pub mod mp3;
pub mod mp4;
pub mod ogg;
pub mod riff;

use std::collections::BTreeMap;
use std::path::Path;

/// Technical properties of the audio stream.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioProperties {
    /// Stream codec: `flac`, `mp3`, `alac`, `aac`, `vorbis`, `opus`, `pcm`.
    pub codec: String,
    /// Container: `flac`, `mp3`, `mp4`, `ogg`, `wav`, `aiff`.
    pub container: String,
    /// Sampling frequency in hertz, as declared by the stream header.
    pub sample_rate: Option<u32>,
    /// Bits per sample. Always absent for a lossy codec, which has no such
    /// notion.
    pub bit_depth: Option<u16>,
    /// Number of channels: 1 for mono, 2 for stereo, more for multichannel.
    pub channels: Option<u16>,
    /// Playing time in milliseconds. Absent when the file is truncated or its
    /// header is unusable.
    pub duration_ms: Option<u64>,
    /// Bitrate in kbit/s: measured when the container states it, estimated from
    /// the size of the audio data otherwise.
    pub bitrate_kbps: Option<u32>,
    /// Whether the encoding preserves the original signal. Decides which
    /// quality tier the file lands in.
    pub lossless: bool,
}

impl AudioProperties {
    /// `true` if the stream goes beyond CD format (44.1 kHz / 16 bits).
    pub fn is_hi_res(&self) -> bool {
        self.lossless
            && (self.sample_rate.unwrap_or(0) > 48_000 || self.bit_depth.unwrap_or(0) > 16)
    }

    /// Readable label: `FLAC 24/96`, `MP3 320`, `AAC`…
    pub fn quality_label(&self) -> String {
        let codec = self.codec.to_uppercase();
        match (self.lossless, self.bit_depth, self.sample_rate) {
            (true, Some(bits), Some(rate)) => {
                let khz = rate as f64 / 1000.0;
                if khz.fract() == 0.0 {
                    format!("{codec} {bits}/{}", khz as u32)
                } else {
                    format!("{codec} {bits}/{khz:.1}")
                }
            }
            _ => match self.bitrate_kbps {
                Some(kbps) => format!("{codec} {kbps}"),
                None => codec,
            },
        }
    }
}

/// Raw metadata read from a file, before any entity matching.
#[derive(Debug, Clone, Default)]
pub struct RawTags {
    /// Normalised fields; a key may carry several values
    /// (several artists, several genres…).
    pub fields: BTreeMap<String, Vec<String>>,
    /// Technical characteristics of the stream, filled in even when the file
    /// carries no tag at all.
    pub properties: AudioProperties,
    /// Is cover art embedded in the file?
    pub has_embedded_art: bool,
}

impl RawTags {
    /// Adds a value under `key`, after normalising the key to the Vorbis
    /// Comment vocabulary.
    ///
    /// The value is trimmed of whitespace and of the null bytes that some
    /// taggers leave behind; if nothing remains, or if the same value is
    /// already present under that key, the call is a no-op.
    pub fn insert(&mut self, key: &str, value: impl Into<String>) {
        let value: String = value.into();
        let value = value
            .trim_matches(|c: char| c == '\0' || c.is_whitespace())
            .to_string();
        if value.is_empty() {
            return;
        }
        let entry = self.fields.entry(canonical_key(key)).or_default();
        if !entry.contains(&value) {
            entry.push(value);
        }
    }

    /// First value of a field.
    pub fn first(&self, key: &str) -> Option<&str> {
        self.fields
            .get(key)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }

    /// All values of a field.
    pub fn all(&self, key: &str) -> &[String] {
        self.fields.get(key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// First value among several candidate keys, in the given order.
    pub fn first_of(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|k| self.first(k))
    }

    /// `true` when no textual field was found. The audio properties may still
    /// be filled in: an untagged file is not an unreadable one.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// Error while reading an audio file.
#[derive(Debug)]
pub enum TagError {
    /// The file could not be opened or a read failed part-way through; the
    /// content itself is not implicated.
    Io(std::io::Error),
    /// The file does not carry the signature expected for its extension.
    UnrecognizedFormat,
    /// Inconsistent internal structure (truncated or corrupted file).
    Malformed(&'static str),
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagError::Io(e) => write!(f, "input/output error: {e}"),
            TagError::UnrecognizedFormat => write!(f, "unrecognised file signature"),
            TagError::Malformed(what) => write!(f, "malformed file: {what}"),
        }
    }
}

impl std::error::Error for TagError {}

impl From<std::io::Error> for TagError {
    fn from(e: std::io::Error) -> Self {
        TagError::Io(e)
    }
}

/// Extensions treated as audio tracks.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    // Parsed here.
    "flac", "mp3", "m4a", "m4b", "mp4", "alac", "ogg", "oga", "opus", "wav", "wave", "aif", "aiff",
    "aifc", // Handed to `lofty`.
    "aac", "ape", "wv", "mpc", "mp+", "mpp", "spx",
];

/// `true` if the path extension is one of the supported audio formats.
pub fn is_audio_path(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            AUDIO_EXTENSIONS.contains(&ext.as_str())
        }
        None => false,
    }
}

/// Reads the tags and properties of an audio file.
///
/// Dispatch is driven first by the **actual signature** of the file, and only
/// then by the extension: a real library always contains a few `.mp3` files
/// that are in fact FLAC.
pub fn read(path: &Path) -> Result<RawTags, TagError> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let magic = read_magic(&mut file)?;

    let native = match detect(&magic) {
        Some(Container::Flac) => flac::read(&mut file, size),
        Some(Container::Ogg) => ogg::read(&mut file, size),
        Some(Container::Mp4) => mp4::read(&mut file, size),
        Some(Container::Riff) => riff::read_wav(&mut file, size),
        Some(Container::Aiff) => riff::read_aiff(&mut file, size),
        Some(Container::Mp3) => mp3::read(&mut file, size),
        None => Err(TagError::UnrecognizedFormat),
    };

    // A signature none of the parsers claims is not necessarily an unreadable
    // file: it may simply be one of the formats left to `lofty`. A parser that
    // recognised the file and then failed keeps its own diagnosis, which is
    // more precise than anything the fallback would say.
    let mut tags = match native {
        Err(TagError::UnrecognizedFormat) => foreign::read(path)?,
        other => other?,
    };

    // Bitrate approximated from the total size, for the formats that do not
    // announce it themselves.
    if tags.properties.bitrate_kbps.is_none() {
        tags.properties.bitrate_kbps = tags
            .properties
            .duration_ms
            .and_then(|ms| (size * 8).checked_div(ms))
            .map(|kbps| kbps as u32);
    }
    Ok(tags)
}

enum Container {
    Flac,
    Ogg,
    Mp4,
    Riff,
    Aiff,
    Mp3,
}

fn detect(magic: &[u8; 16]) -> Option<Container> {
    if &magic[0..4] == b"fLaC" {
        return Some(Container::Flac);
    }
    if &magic[0..4] == b"OggS" {
        return Some(Container::Ogg);
    }
    if &magic[4..8] == b"ftyp" {
        return Some(Container::Mp4);
    }
    if &magic[0..4] == b"RIFF" && &magic[8..12] == b"WAVE" {
        return Some(Container::Riff);
    }
    if &magic[0..4] == b"FORM" && (&magic[8..12] == b"AIFF" || &magic[8..12] == b"AIFC") {
        return Some(Container::Aiff);
    }
    // A leading ID3v2 tag may precede MP3 as well as FLAC; mp3::read handles
    // both cases by resynchronising after the tag.
    if &magic[0..3] == b"ID3" {
        return Some(Container::Mp3);
    }
    // An MPEG frame and an ADTS frame share the same eleven-bit sync word and
    // are told apart by the layer field, which ADTS leaves at zero — a value
    // MPEG reserves. Without this, raw AAC would be handed to the MP3 parser.
    if magic[0] == 0xFF && (magic[1] & 0xE0) == 0xE0 && (magic[1] & 0x06) != 0 {
        return Some(Container::Mp3);
    }
    None
}

fn read_magic(file: &mut std::fs::File) -> Result<[u8; 16], TagError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut magic = [0u8; 16];
    let read = read_up_to(file, &mut magic)?;
    if read < 12 {
        return Err(TagError::UnrecognizedFormat);
    }
    file.seek(SeekFrom::Start(0))?;
    return Ok(magic);

    fn read_up_to(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut total = 0;
        while total < buf.len() {
            match file.read(&mut buf[total..])? {
                0 => break,
                n => total += n,
            }
        }
        Ok(total)
    }
}

/// Maps the field names specific to each format onto a single vocabulary.
pub fn canonical_key(key: &str) -> String {
    let lower = key.trim().to_ascii_lowercase();
    let mapped = match lower.as_str() {
        // Vorbis / FLAC
        "tracknumber" | "track" => "tracknumber",
        "totaltracks" | "tracktotal" => "tracktotal",
        "discnumber" | "disc" => "discnumber",
        "totaldiscs" | "disctotal" => "disctotal",
        "album artist" | "album_artist" | "albumartist" => "albumartist",
        "albumartistsort" | "album artist sort" => "albumartistsort",
        "artistsort" | "artist sort" => "artistsort",
        "organization" | "publisher" | "label" => "label",
        "catalognumber" | "catalog" | "catalogid" => "catalognumber",
        "originaldate" | "originalyear" | "original_year" => "originaldate",
        "releasedate" | "date" | "year" => "date",
        "unsyncedlyrics" | "unsynced lyrics" | "lyrics" => "lyrics",
        "musicbrainz_trackid" | "musicbrainz track id" => "musicbrainz_recordingid",
        "musicbrainz_releasetrackid" => "musicbrainz_trackid",
        "musicbrainz_albumid" | "musicbrainz album id" => "musicbrainz_albumid",
        "musicbrainz_artistid" | "musicbrainz artist id" => "musicbrainz_artistid",
        "musicbrainz_albumartistid" => "musicbrainz_albumartistid",
        "musicbrainz_releasegroupid" => "musicbrainz_releasegroupid",
        "musicbrainz release group id" => "musicbrainz_releasegroupid",
        "acoustid_id" | "acoustid id" => "acoustid_id",
        "acoustid_fingerprint" => "acoustid_fingerprint",
        "replaygain_track_gain" => "replaygain_track_gain",
        "replaygain_album_gain" => "replaygain_album_gain",
        "compilation" | "itunescompilation" => "compilation",
        "media" | "mediatype" => "media",
        "grouping" | "contentgroup" | "work" => "grouping",
        other => other,
    };
    mapped.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_keys() {
        assert_eq!(canonical_key("TRACKNUMBER"), "tracknumber");
        assert_eq!(canonical_key("Album Artist"), "albumartist");
        assert_eq!(canonical_key("PUBLISHER"), "label");
        assert_eq!(canonical_key("YEAR"), "date");
    }

    #[test]
    fn insertion_deduplicates_and_cleans() {
        let mut tags = RawTags::default();
        tags.insert("ARTIST", "  Miles Davis  ");
        tags.insert("artist", "Miles Davis");
        tags.insert("artist", "John Coltrane");
        tags.insert("artist", "   ");
        assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
    }

    #[test]
    fn quality_label() {
        let hires = AudioProperties {
            codec: "flac".into(),
            lossless: true,
            bit_depth: Some(24),
            sample_rate: Some(96_000),
            ..Default::default()
        };
        assert_eq!(hires.quality_label(), "FLAC 24/96");
        assert!(hires.is_hi_res());

        let cd = AudioProperties {
            codec: "flac".into(),
            lossless: true,
            bit_depth: Some(16),
            sample_rate: Some(44_100),
            ..Default::default()
        };
        assert_eq!(cd.quality_label(), "FLAC 16/44.1");
        assert!(!cd.is_hi_res());

        let lossy = AudioProperties {
            codec: "mp3".into(),
            bitrate_kbps: Some(320),
            ..Default::default()
        };
        assert_eq!(lossy.quality_label(), "MP3 320");
        assert!(!lossy.is_hi_res());
    }

    #[test]
    fn extension_recognition() {
        assert!(is_audio_path(Path::new("/music/a.FLAC")));
        assert!(is_audio_path(Path::new("/music/a.mp3")));
        assert!(!is_audio_path(Path::new("/music/cover.jpg")));
        assert!(!is_audio_path(Path::new("/music/folder")));
    }
}
