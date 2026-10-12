//! Filesystem playlists are shared library files, independent of private exports.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod metadata;
use metadata::metadata;
mod read;
pub use read::{Document, read};

/// A complete inspection refuses capacity/deadline limits instead of hiding files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The requested identity is malformed.
    InvalidIdentity,
    /// No currently permitted file has this identity.
    NotFound,
    /// A path or opened file changed during inspection.
    Changed,
    /// A filesystem operation failed.
    Unavailable,
    /// A path cannot be represented losslessly as bounded UTF-8.
    InvalidPath,
    /// Traversal, file content or entry count exceeds its limit.
    TooLarge,
    /// The cooperative inspection deadline was reached.
    Timeout,
    /// Content is not UTF-8 M3U text.
    InvalidText,
}

/// Metadata only: discovery never opens playlist content.
#[derive(Clone, Debug)]
pub struct File {
    /// Opaque path selector; authorization always comes from the current roots.
    pub id: String,
    /// Original filename without its extension.
    pub name: String,
    /// Original filename, including its `.m3u` or `.m3u8` extension.
    pub filename: String,
    /// Canonical path, deduplicated across overlapping roots.
    pub path: String,
}

/// Sorted files plus explicit diagnostics for inaccessible parts of the roots.
pub struct Discovery {
    /// Files in deterministic canonical path order.
    pub files: Vec<File>,
    /// Watched roots that cannot currently be inspected.
    pub unavailable_roots: usize,
    /// Child directories or directory entries that could not be inspected.
    pub unreadable_directories: usize,
}

#[derive(Clone, Copy)]
struct Limits {
    files: usize,
    entries: usize,
    depth: usize,
    elapsed: Duration,
}
const LIMITS: Limits = Limits {
    files: 10_000,
    entries: 250_000,
    depth: 64,
    elapsed: Duration::from_secs(3),
};

struct Inspection {
    roots: Vec<PathBuf>,
    excluded: Vec<PathBuf>,
    result: Discovery,
    entries: usize,
    started: Instant,
    limits: Limits,
    seen_files: BTreeMap<String, Vec<usize>>,
}

impl Inspection {
    fn new(roots: &[String], excluded: &[String], limits: Limits) -> Result<Self, Error> {
        let mut inspection = Self {
            roots: Vec::new(),
            excluded: Vec::new(),
            result: Discovery {
                files: Vec::new(),
                unavailable_roots: 0,
                unreadable_directories: 0,
            },
            entries: 0,
            started: Instant::now(),
            limits,
            seen_files: BTreeMap::new(),
        };
        for path in excluded {
            inspection.check()?;
            let path = PathBuf::from(path);
            inspection
                .excluded
                .push(path.canonicalize().unwrap_or(path));
        }
        for root in roots {
            inspection.check()?;
            match Path::new(root).canonicalize() {
                Ok(root) if root.is_dir() => inspection.roots.push(root),
                _ => inspection.result.unavailable_roots += 1,
            }
        }
        inspection.roots.sort();
        inspection.roots.dedup();
        Ok(inspection)
    }

    fn check(&self) -> Result<(), Error> {
        if self.started.elapsed() >= self.limits.elapsed {
            Err(Error::Timeout)
        } else {
            Ok(())
        }
    }

    fn excluded(&self, path: &Path) -> bool {
        self.excluded
            .iter()
            .any(|excluded| path.starts_with(excluded))
    }

    fn walk(&mut self, folder: &Path, depth: usize) -> Result<(), Error> {
        self.check()?;
        if self.excluded(folder) {
            return Ok(());
        }
        if depth > self.limits.depth {
            return Err(Error::TooLarge);
        }
        // Root aliases are canonicalized once; no symlink beneath them is admitted.
        let directory_metadata = match fs::symlink_metadata(folder) {
            Ok(metadata) => metadata,
            Err(_) => {
                self.result.unreadable_directories += 1;
                return Ok(());
            }
        };
        let canonical = match folder.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => {
                self.result.unreadable_directories += 1;
                return Ok(());
            }
        };
        if !directory_metadata.is_dir()
            || directory_metadata.file_type().is_symlink()
            || canonical != folder
        {
            return Err(Error::Changed);
        }
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(_) => {
                self.result.unreadable_directories += 1;
                return Ok(());
            }
        };
        for entry in entries {
            self.check()?;
            self.entries += 1;
            if self.entries > self.limits.entries {
                return Err(Error::TooLarge);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.result.unreadable_directories += 1;
                    continue;
                }
            };
            let path = entry.path();
            if self.excluded(&path) {
                continue;
            }
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => {
                    self.result.unreadable_directories += 1;
                    continue;
                }
            };
            if kind.is_dir() {
                self.walk(&path, depth + 1)?;
            }
            if kind.is_file() && playlist_path(&path) {
                let canonical = match path.canonicalize() {
                    Ok(canonical) => canonical,
                    Err(_) => {
                        self.result.unreadable_directories += 1;
                        continue;
                    }
                };
                if canonical != path {
                    return Err(Error::Changed);
                }
                let file = metadata(&path)?;
                if self.seen_files.get(&file.id).is_some_and(|indices| {
                    indices
                        .iter()
                        .any(|&index| self.result.files[index].path == file.path)
                }) {
                    continue;
                }
                if self.result.files.len() >= self.limits.files {
                    return Err(Error::TooLarge);
                }
                self.seen_files
                    .entry(file.id.clone())
                    .or_default()
                    .push(self.result.files.len());
                self.result.files.push(file);
            }
        }
        self.check()
    }
}

fn playlist_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("m3u") || extension.eq_ignore_ascii_case("m3u8")
        })
}

/// Discover all regular M3U files, including folders without catalogued audio.
/// A filesystem syscall may outlast the cooperative three-second deadline.
pub fn discover(roots: &[String], excluded: &[String]) -> Result<Discovery, Error> {
    discover_with_limits(roots, excluded, LIMITS)
}

fn discover_with_limits(
    roots: &[String],
    excluded: &[String],
    limits: Limits,
) -> Result<Discovery, Error> {
    let mut inspection = Inspection::new(roots, excluded, limits)?;
    let mut covered = Vec::<PathBuf>::new();
    for root in inspection.roots.clone() {
        if covered.iter().any(|parent| root.starts_with(parent)) {
            continue;
        }
        let unreadable = inspection.result.unreadable_directories;
        inspection.walk(&root, 0)?;
        // An inaccessible parent cannot cover an explicitly watched readable
        // child. Revisit such roots; only distinct file paths consume slots.
        if unreadable == inspection.result.unreadable_directories {
            covered.push(root);
        }
    }
    inspection
        .result
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inspection.result)
}

#[cfg(test)]
#[path = "files_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
