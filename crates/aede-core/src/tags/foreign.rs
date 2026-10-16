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

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::{Tag, TagType};
use lofty::wavpack::WavPackFile;

use super::{RawTags, TagError};

/// Reads a file through `lofty` and translates the result into [`RawTags`].
pub fn read(path: &Path) -> Result<RawTags, TagError> {
    let probe = Probe::open(path).map_err(|_| TagError::UnrecognizedFormat)?;
    read_probe(probe)
}

pub(super) fn read_from_file(file: &mut std::fs::File) -> Result<RawTags, TagError> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    read_probe(Probe::new(file))
}

fn read_probe<R: std::io::Read + std::io::Seek>(probe: Probe<R>) -> Result<RawTags, TagError> {
    let probe = probe
        .guess_file_type()
        .map_err(|_| TagError::UnrecognizedFormat)?;
    if probe.file_type() == Some(FileType::WavPack) {
        let mut reader = probe.into_inner();
        // The first audio block's FLOAT_DATA flag is not exposed by Lofty.
        let flags = wavpack_audio_flags(&mut reader)?;
        reader
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|_| TagError::UnrecognizedFormat)?;
        let wavpack = WavPackFile::read_from(&mut reader, ParseOptions::default())
            .map_err(|_| TagError::UnrecognizedFormat)?;
        // Generic FileProperties drops WavPack's hybrid/lossless distinction.
        let lossless = wavpack.properties().is_lossless();
        let mut tags = translate(wavpack.into());
        tags.properties.lossless = lossless;
        // Shared properties cannot describe compressed floating-point PCM;
        // unknown precision makes lossless exporters refuse quantization.
        if !lossless || flags.is_none_or(|flags| flags & 0x80 != 0) {
            tags.properties.bit_depth = None;
        }
        return Ok(tags);
    }
    let file = probe.read().map_err(|_| TagError::UnrecognizedFormat)?;
    Ok(translate(file))
}

/// Reads only headers, bounding non-audio prefixes without loading their bodies.
/// Unknown flags must not be treated as evidence of integer PCM precision.
fn wavpack_audio_flags<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> Result<Option<u32>, TagError> {
    let mut offset = 0u64;
    for _ in 0..64 {
        reader.seek(std::io::SeekFrom::Start(offset))?;
        let mut bytes = [0; 32];
        match reader.read_exact(&mut bytes) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let mut header = super::bytes::Cursor::new(&bytes);
        if header.take(4) != Some(b"wvpk".as_slice()) {
            return Ok(None);
        }
        let Some(size @ 24..=1_048_576) = header.u32_le() else {
            return Ok(None);
        };
        header.skip(12);
        let (Some(samples), Some(flags)) = (header.u32_le(), header.u32_le()) else {
            return Ok(None);
        };
        if samples > 0 {
            return Ok(Some(flags));
        }
        offset = offset
            .checked_add(u64::from(size) + 8)
            .ok_or(TagError::Malformed("WavPack block offset overflow"))?;
    }
    Ok(None)
}

fn translate(file: TaggedFile) -> RawTags {
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

    // Catalogue readers take the first value. Lofty does not always list the
    // format's primary tag first; keep it ahead of legacy alternatives while
    // retaining their unique values and fields missing from the primary tag.
    let primary = file.primary_tag_type();
    for tag in file
        .tags()
        .iter()
        .filter(|tag| tag.tag_type() == primary)
        .chain(file.tags().iter().filter(|tag| tag.tag_type() != primary))
    {
        collect(tag, &mut tags);
    }
    tags
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

#[cfg(test)]
#[path = "foreign_tests.rs"]
mod tests;
