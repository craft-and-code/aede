//! Read the selected file itself; playlist entries never trigger file/network I/O.

use super::*;
use std::io::Read;

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TRACKS: usize = 4096;

/// Exact original text and a count of nonempty, noncomment entry lines.
pub struct Document {
    /// Currently permitted file identity and path.
    pub file: File,
    /// UTF-8 content, preserving BOM, whitespace and line endings.
    pub content: String,
    /// Entry lines, including repeated paths and URLs; no availability inference.
    pub track_count: usize,
}

/// Read only an identity rediscovered under the current roots and exclusions.
pub fn read(roots: &[String], excluded: &[String], id: &str) -> Result<Document, Error> {
    read_with_open(roots, excluded, id, |path| fs::File::open(path))
}

fn read_with_open(
    roots: &[String],
    excluded: &[String],
    id: &str,
    open: impl FnOnce(&Path) -> std::io::Result<fs::File>,
) -> Result<Document, Error> {
    let suffix = id.strip_prefix("m3u-file-").ok_or(Error::InvalidIdentity)?;
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Error::InvalidIdentity);
    }
    let scope = || {
        roots
            .iter()
            .chain(excluded)
            .map(|path| Path::new(path).canonicalize().ok())
            .collect::<Vec<_>>()
    };
    let before = scope();
    let started = Instant::now();
    let mut matches = discover(roots, excluded)?
        .files
        .into_iter()
        .filter(|file| file.id == id);
    let file = matches.next().ok_or(Error::NotFound)?;
    if matches.next().is_some() {
        return Err(Error::Changed);
    }
    let document = read_file(file, open)?;
    if before != scope() {
        return Err(Error::Changed);
    }
    if started.elapsed() >= LIMITS.elapsed {
        return Err(Error::Timeout);
    }
    Ok(document)
}

fn safe_path(path: &Path) -> Result<fs::Metadata, Error> {
    let metadata = fs::symlink_metadata(path).map_err(|_| Error::Unavailable)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || path.canonicalize().map_err(|_| Error::Unavailable)? != path
    {
        return Err(Error::Changed);
    }
    if metadata.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    Ok(metadata)
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if left.dev() != right.dev() || left.ino() != right.ino() {
            return false;
        }
    }
    left.is_file()
        && right.is_file()
        && left.len() == right.len()
        && left
            .modified()
            .ok()
            .zip(right.modified().ok())
            .is_some_and(|(left, right)| left == right)
}

fn read_file(
    file: File,
    open: impl FnOnce(&Path) -> std::io::Result<fs::File>,
) -> Result<Document, Error> {
    let path = Path::new(&file.path);
    let before = safe_path(path)?;
    let handle = open(path).map_err(|_| Error::Unavailable)?;
    let opened = handle.metadata().map_err(|_| Error::Unavailable)?;
    if !same_file(&before, &opened) || !same_file(&opened, &safe_path(path)?) {
        return Err(Error::Changed);
    }
    let mut bytes = Vec::new();
    (&handle)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Unavailable)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    if !same_file(&opened, &handle.metadata().map_err(|_| Error::Unavailable)?)
        || !same_file(&opened, &safe_path(path)?)
    {
        return Err(Error::Changed);
    }
    let content = String::from_utf8(bytes).map_err(|_| Error::InvalidText)?;
    if content
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\r' | '\n'))
    {
        return Err(Error::InvalidText);
    }
    let track_count = crate::playlist::entry_lines(&content).count();
    if track_count > MAX_TRACKS {
        return Err(Error::TooLarge);
    }
    Ok(Document {
        file,
        content,
        track_count,
    })
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
