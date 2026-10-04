//! Complete local lyrics reads for clients that cannot accept a truncated result.

use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path};

use super::{Lyrics, Source, parse_complete};

const MAX_INPUT_BYTES: u64 = 256 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// Current catalog evidence for the audio to which lyrics belong.
///
/// Paths come from the catalog, never from an HTTP caller. A sidecar must have
/// the audio file's basename and parent, with a case-insensitive `.lrc` suffix.
/// Precise modification time is required: callers must rescan legacy catalogs
/// before publishing lyrics associated with an unverified audio identity.
pub struct CurrentTrack<'a> {
    /// Catalogued audio path.
    pub path: &'a Path,
    /// Catalogued audio byte length.
    pub size: u64,
    /// Catalogued modification time in whole seconds.
    pub mtime: u64,
    /// Nanosecond fraction of the catalogued modification time.
    pub mtime_subseconds: u32,
    /// First lyrics tag, when present; nonempty tag lyrics precede a sidecar.
    pub tag: Option<&'a str>,
    /// Catalogued lyrics sidecar, when present.
    pub sidecar: Option<&'a Path>,
}

/// Why a complete, current lyrics answer cannot be supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// The catalogued audio is missing or cannot be opened.
    SourceUnavailable,
    /// Audio identity changed, or its path is a link or non-regular file.
    SourceChanged,
    /// The catalogued sidecar is missing or cannot be opened.
    SidecarUnavailable,
    /// Sidecar identity, type or confinement cannot be confirmed.
    SidecarChanged,
    /// Input exceeds 256 KiB or complete decoded/expanded text exceeds 1 MiB.
    TooLarge,
    /// The complete sidecar could not be read.
    ReadFailed,
    /// Local audio tags could not be read for an uncatalogued track.
    TagsUnavailable,
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SourceUnavailable => "the catalogued audio source is unavailable",
            Self::SourceChanged => "the catalogued audio source changed; rescan it",
            Self::SidecarUnavailable => "the catalogued lyrics sidecar is unavailable",
            Self::SidecarChanged => "the lyrics sidecar changed or is not a regular adjacent file",
            Self::TooLarge => "complete lyrics exceed the supported text size",
            Self::ReadFailed => "the complete lyrics sidecar could not be read",
            Self::TagsUnavailable => "audio tags could not be read to select local lyrics",
        })
    }
}

impl std::error::Error for ReadError {}

fn regular(metadata: &Metadata) -> bool {
    metadata.is_file() && !metadata.file_type().is_symlink()
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(not(unix))]
    {
        // Stable descriptor IDs are not available through the supported
        // Windows standard-library API. Retain precise metadata checks.
        left.len() == right.len()
            && matches!((left.modified(), right.modified()), (Ok(left), Ok(right)) if left == right)
    }
}

fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    regular(after)
        && same_file(before, after)
        && before.len() == after.len()
        && matches!((before.modified(), after.modified()), (Ok(before), Ok(after)) if before == after)
}

fn audio_matches(source: &CurrentTrack<'_>, metadata: &Metadata) -> bool {
    regular(metadata)
        && metadata.len() == source.size
        && crate::clock::mtime_seconds(metadata) == source.mtime
        && crate::clock::mtime_subseconds(metadata) == source.mtime_subseconds
}

fn open_audio(source: &CurrentTrack<'_>) -> Result<(File, Metadata), ReadError> {
    if !source.path.is_absolute()
        || source
            .path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(ReadError::SourceChanged);
    }
    let before = fs::symlink_metadata(source.path).map_err(|_| ReadError::SourceUnavailable)?;
    if !audio_matches(source, &before) {
        return Err(ReadError::SourceChanged);
    }
    let file = File::open(source.path).map_err(|_| ReadError::SourceUnavailable)?;
    let opened = file.metadata().map_err(|_| ReadError::SourceUnavailable)?;
    if !audio_matches(source, &opened) || !unchanged(&before, &opened) {
        return Err(ReadError::SourceChanged);
    }
    Ok((file, opened))
}

fn confined(audio: &Path, sidecar: &Path) -> Result<(), ReadError> {
    if !audio.is_absolute()
        || !sidecar.is_absolute()
        || audio.parent() != sidecar.parent()
        || audio.file_stem() != sidecar.file_stem()
        || sidecar
            .extension()
            .and_then(|suffix| suffix.to_str())
            .is_none_or(|suffix| !suffix.eq_ignore_ascii_case(super::EXTENSION))
        || [audio, sidecar].iter().any(|path| {
            path.components()
                .any(|part| matches!(part, Component::ParentDir))
        })
    {
        return Err(ReadError::SidecarChanged);
    }
    let folder = audio.parent().ok_or(ReadError::SidecarChanged)?;
    let folder_metadata = fs::symlink_metadata(folder).map_err(|_| ReadError::SidecarChanged)?;
    if !folder_metadata.is_dir() || folder_metadata.file_type().is_symlink() {
        return Err(ReadError::SidecarChanged);
    }
    let folder = fs::canonicalize(folder).map_err(|_| ReadError::SidecarChanged)?;
    let audio = fs::canonicalize(audio).map_err(|_| ReadError::SourceChanged)?;
    let sidecar = fs::canonicalize(sidecar).map_err(|_| ReadError::SidecarUnavailable)?;
    if audio.parent() != Some(folder.as_path()) || sidecar.parent() != Some(folder.as_path()) {
        return Err(ReadError::SidecarChanged);
    }
    Ok(())
}

fn complete(origin: &Path, text: &str, source: Source) -> Result<Option<Lyrics>, ReadError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(ReadError::TooLarge);
    }
    let lines = parse_complete(text, MAX_TEXT_BYTES).map_err(|_| ReadError::TooLarge)?;
    Ok((!lines.is_empty()).then(|| Lyrics {
        source,
        origin: origin.to_string_lossy().into_owned(),
        lines,
    }))
}

fn tag_lyrics(path: &Path, tag: Option<&str>) -> Result<Option<Lyrics>, ReadError> {
    match tag {
        Some(text) if text.len() as u64 > MAX_INPUT_BYTES => Err(ReadError::TooLarge),
        Some(text) => complete(path, text, Source::Tag),
        None => Ok(None),
    }
}

fn read_sidecar(audio: &Path, path: &Path) -> Result<Option<Lyrics>, ReadError> {
    confined(audio, path)?;
    let before = fs::symlink_metadata(path).map_err(|_| ReadError::SidecarUnavailable)?;
    if !regular(&before) {
        return Err(ReadError::SidecarChanged);
    }
    if before.len() > MAX_INPUT_BYTES {
        return Err(ReadError::TooLarge);
    }
    let mut file = File::open(path).map_err(|_| ReadError::SidecarUnavailable)?;
    let opened = file.metadata().map_err(|_| ReadError::SidecarUnavailable)?;
    if !unchanged(&before, &opened) {
        return Err(ReadError::SidecarChanged);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ReadError::ReadFailed)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(ReadError::TooLarge);
    }
    let lyrics = complete(path, &String::from_utf8_lossy(&bytes), Source::Sidecar)?;
    if bytes.len() as u64 != opened.len()
        || !unchanged(
            &opened,
            &file.metadata().map_err(|_| ReadError::SidecarChanged)?,
        )
        || !unchanged(
            &opened,
            &fs::symlink_metadata(path).map_err(|_| ReadError::SidecarChanged)?,
        )
    {
        return Err(ReadError::SidecarChanged);
    }
    confined(audio, path)?;
    Ok(lyrics)
}

/// Read a complete local lyrics answer after confirming its current audio identity.
///
/// Tag lyrics take precedence; an empty tag falls back to the catalogued `.lrc`.
/// With neither source, the answer is `Ok(None)`. Empty/metadata-only lyrics
/// also return `None`. This function never fetches or writes anything.
///
/// Input text is limited to 256 KiB, complete decoded/expanded text to 1 MiB.
/// Unlike [`super::read`], excessive input and timestamp expansion are explicit
/// errors rather than a prefix or an unsynchronized fallback. Invalid UTF-8 is
/// replaced consistently with the ordinary sidecar reader. Audio and sidecar
/// descriptors, paths and precise timestamps are checked before publication;
/// this detects ordinary replacement races but cannot eliminate every concurrent
/// ancestor-directory change through pathname-based standard-library APIs.
pub fn read_current(source: CurrentTrack<'_>) -> Result<Option<Lyrics>, ReadError> {
    let (audio, opened) = open_audio(&source)?;
    let lyrics = tag_lyrics(source.path, source.tag)?;
    let lyrics = match (lyrics, source.sidecar) {
        (None, Some(path)) => read_sidecar(source.path, path)?,
        (lyrics, _) => lyrics,
    };
    if !audio_matches(
        &source,
        &audio.metadata().map_err(|_| ReadError::SourceChanged)?,
    ) || !unchanged(
        &opened,
        &fs::symlink_metadata(source.path).map_err(|_| ReadError::SourceChanged)?,
    ) {
        return Err(ReadError::SourceChanged);
    }
    Ok(lyrics)
}

/// Read complete lyrics for a local audio path without catalog evidence.
///
/// Existing tag parsers read the audio once; its descriptor and precise identity
/// are checked before and after that read. A nonempty lyrics tag wins over the
/// standard lowercase `.lrc` sidecar. This operation never scans, fetches or
/// writes. For a catalogued track, prefer [`read_current`] so stale stored tags
/// cannot be published against an audio file that changed since its scan.
pub fn read_local(path: &Path) -> Result<Option<Lyrics>, ReadError> {
    let before = fs::symlink_metadata(path).map_err(|_| ReadError::SourceUnavailable)?;
    if !regular(&before) {
        return Err(ReadError::SourceChanged);
    }
    let absolute = fs::canonicalize(path).map_err(|_| ReadError::SourceUnavailable)?;
    let source = CurrentTrack {
        path: &absolute,
        size: before.len(),
        mtime: crate::clock::mtime_seconds(&before),
        mtime_subseconds: crate::clock::mtime_subseconds(&before),
        tag: None,
        sidecar: None,
    };
    let (audio, opened) = open_audio(&source)?;
    if !unchanged(&before, &opened) {
        return Err(ReadError::SourceChanged);
    }
    let tags = crate::tags::read(&absolute).map_err(|_| ReadError::TagsUnavailable)?;
    if !unchanged(
        &opened,
        &audio.metadata().map_err(|_| ReadError::SourceChanged)?,
    ) || !unchanged(
        &opened,
        &fs::symlink_metadata(path).map_err(|_| ReadError::SourceChanged)?,
    ) {
        return Err(ReadError::SourceChanged);
    }
    let lyrics = match tag_lyrics(&absolute, tags.first("lyrics"))? {
        Some(lyrics) => Some(lyrics),
        None => {
            let sidecar = super::sidecar_of(&absolute);
            match fs::symlink_metadata(&sidecar) {
                Ok(_) => read_sidecar(&absolute, &sidecar)?,
                Err(failure) if failure.kind() == io::ErrorKind::NotFound => None,
                Err(_) => return Err(ReadError::SidecarUnavailable),
            }
        }
    };
    if !unchanged(
        &opened,
        &audio.metadata().map_err(|_| ReadError::SourceChanged)?,
    ) || !unchanged(
        &opened,
        &fs::symlink_metadata(path).map_err(|_| ReadError::SourceChanged)?,
    ) {
        return Err(ReadError::SourceChanged);
    }
    Ok(lyrics)
}

#[cfg(test)]
#[path = "lyrics_source_tests.rs"]
mod tests;
