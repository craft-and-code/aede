//! Containers Aede has no parser of its own for, read through `lofty`.
//!
//! The hand-written parsers cover the formats a music library is actually made
//! of, and they extract things a general-purpose library does not expose — the
//! LAME encoder delay, the ALAC magic cookie, the Opus pre-skip. They stay in
//! charge of those formats.
//!
//! What is left over is the long tail: WavPack, Monkey's Audio, Musepack,
//! Speex, raw AAC. Writing a parser for each would cost weeks and would be
//! exercised by a handful of files. This module hands them to `lofty` instead,
//! and maps what comes back onto [`RawTags`], so the rest of the program never
//! learns that two different readers exist.
//!
//! The fallback is only reached when the signature matches none of the native
//! parsers, so it can never take a format away from them.

use std::path::Path;

use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::tag::{Tag, TagType};

use super::{RawTags, TagError};

/// Reads a file through `lofty` and translates the result into [`RawTags`].
pub fn read(path: &Path) -> Result<RawTags, TagError> {
    let file = lofty::read_from_path(path).map_err(|_| TagError::UnrecognizedFormat)?;
    let mut tags = RawTags::default();

    describe_stream(file.file_type(), &mut tags);
    let properties = file.properties();
    tags.properties.sample_rate = properties.sample_rate();
    tags.properties.channels = properties.channels().map(u16::from);
    if tags.properties.lossless {
        tags.properties.bit_depth = properties.bit_depth().map(u16::from);
    }
    let duration = properties.duration().as_millis();
    if duration > 0 {
        tags.properties.duration_ms = Some(duration as u64);
    }
    tags.properties.bitrate_kbps = properties.audio_bitrate().or(properties.overall_bitrate());

    // Later tags do not overwrite earlier ones: `RawTags::insert` appends, and
    // the readers of the catalog take the first value. A file carrying both an
    // APE tag and an ID3v1 one therefore keeps the richer of the two, which
    // `lofty` lists first.
    for tag in file.tags() {
        collect(tag, &mut tags);
    }
    Ok(tags)
}

/// Fills in codec, container and losslessness from the detected file type.
///
/// Names follow the ones the native parsers use, since they end up in the same
/// statistics and in the same `codec` column of the catalog.
fn describe_stream(file_type: FileType, tags: &mut RawTags) {
    let (codec, container, lossless) = match file_type {
        FileType::Aac => ("aac", "adts", false),
        FileType::Ape => ("ape", "ape", true),
        FileType::Mpc => ("musepack", "musepack", false),
        FileType::WavPack => ("wavpack", "wavpack", true),
        FileType::Speex => ("speex", "ogg", false),
        FileType::Flac => ("flac", "flac", true),
        FileType::Mpeg => ("mp3", "mp3", false),
        FileType::Mp4 => ("aac", "mp4", false),
        FileType::Opus => ("opus", "ogg", false),
        FileType::Vorbis => ("vorbis", "ogg", false),
        FileType::Wav => ("pcm", "wav", true),
        FileType::Aiff => ("pcm", "aiff", true),
        // A resolver registered by a third party. Nothing sensible to say
        // about the stream, but its tags are still worth keeping.
        _ => ("", "", false),
    };
    tags.properties.codec = codec.to_string();
    tags.properties.container = container.to_string();
    tags.properties.lossless = lossless;
}

/// Copies the textual items and the presence of cover art out of one tag.
fn collect(tag: &Tag, tags: &mut RawTags) {
    if tag.picture_count() > 0 {
        tags.has_embedded_art = true;
    }
    for item in tag.items() {
        let Some(text) = item.value().text() else {
            continue;
        };
        if let Some(key) = field_name(item.key()) {
            tags.insert(key, text);
        }
    }
}

/// Spelling of a key in the vocabulary the rest of the program speaks.
///
/// `lofty` normalises every format onto its own [`ItemKey`] enumeration; asking
/// it for the Vorbis Comment spelling brings the value back into the vocabulary
/// [`super::canonical_key`] already understands. `ItemKey` carries no variant
/// for a key it does not know, so a tag `lofty` did not recognise is lost here —
/// the alternative being a file that cannot be read at all.
fn field_name(key: ItemKey) -> Option<&'static str> {
    key.map_key(TagType::VorbisComments)
        .or_else(|| key.map_key(TagType::Ape))
}
