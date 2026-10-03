//! One file holding everything Aède knows, so that a disk failure costs a scan
//! and not a year of listening.
//!
//! Five independently versioned stores are nested whole: the mostly derived
//! catalog (which also holds watched roots), costly byte-level conclusions,
//! irreplaceable user statements, slowly re-fetchable external sources, and
//! private account credentials. Versions 1 and 2 carry no accounts.
//! A version-1 envelope is still accepted: its embedded legacy conclusions
//! are extracted into the fourth store before restoration.
//!
//! # A document of documents, and why that matters
//!
//! The five are nested **as they are**, each keeping its own `format_version`
//! and read back by its own `from_json`. Nothing here re-encodes a catalog, so
//! a field added to the catalog tomorrow is in the backup tomorrow, with no
//! second writer to forget it — the fault that would otherwise be discovered
//! the day somebody restored one.
//!
//! The consequence worth stating: **each store is refused on its own.** A
//! backup written by an Aède whose catalog format has since moved still
//! restores the notes and the fetched facts, because the catalog's version
//! check has nothing to say about `user.json`. A single check at the top would
//! have thrown away irreplaceable data to protect the rebuildable catalog.
//!
//! # It stores, it never fetches or scans
//!
//! Reading a backup produces documents; writing one consumes them. Nothing
//! here touches the library, and — as everywhere in this program — nothing
//! here touches an audio file.

use std::io::Read;
use std::path::Path;

use crate::accounts::Accounts;
use crate::conclusions::Conclusions;
use crate::json::Json;
use crate::model::Catalog;
use crate::sources::Sources;
use crate::store::StoreError;
use crate::user::UserData;

/// The version of the wrapper, which is not the version of what it wraps.
///
/// It changes when the *shape of the envelope* changes — a fourth store, a
/// renamed field — and not when a store inside it moves, because each of those
/// carries its own version and answers for itself.
pub const BACKUP_FORMAT_VERSION: u32 = 3;

/// One store inside a backup, and what this build can do with it.
///
/// Three states rather than an `Option`, because "the backup holds no
/// `sources.json`" and "the backup holds one this build refuses" call for
/// opposite things to be said and opposite things to be done. Collapsed into
/// one, the second would restore as though the file had never existed — which
/// is the quietest possible way to lose a year of fetching.
#[derive(Debug, Clone)]
pub enum Part<T> {
    /// Held, and readable by this build.
    Held(T),
    /// There was no such store when the backup was made.
    Empty,
    /// Present, and refused by that store's own rule. Carries what it said.
    Unreadable(String),
}

impl<T> Part<T> {
    /// The store, when there is one this build can use.
    pub fn held(&self) -> Option<&T> {
        match self {
            Part::Held(value) => Some(value),
            _ => None,
        }
    }

    /// Reads one nested document, keeping the three states apart.
    ///
    /// A missing key and a `null` are both "there was none": a store that did
    /// not exist is written as `null` rather than left out, so that a reader
    /// can tell a complete backup from a truncated one, and both spellings are
    /// accepted because only one of them is this program's own.
    fn read(
        value: &Json,
        key: &str,
        parse: impl Fn(&Json) -> Result<T, StoreError>,
    ) -> Result<Part<T>, StoreError> {
        match value.get(key) {
            None | Some(Json::Null) => Ok(Part::Empty),
            Some(document) => match parse(document) {
                Ok(store) => Ok(Part::Held(store)),
                Err(why) => Ok(Part::Unreadable(why.to_string())),
            },
        }
    }
}

/// Everything Aède knows, at one moment.
///
/// Not comparable, and not by omission: a `Catalog` is not `Eq` — two catalogs
/// of the same library are equal in no useful sense — so a test that wanted to
/// prove a round trip compares what the documents say rather than the structs.
#[derive(Debug, Clone)]
pub struct Backup {
    /// When it was written, in seconds since the epoch.
    pub made_at: u64,
    /// The version of Aède that wrote it.
    ///
    /// For a reader, not for the program: nothing branches on it, because the
    /// five stores each state their own format and a version string is a poor
    /// substitute for that. What it answers is "which build made this", which
    /// is the first question asked of a file that will not restore.
    pub made_by: String,
    /// The scanned graph and watched roots.
    pub catalog: Part<Catalog>,
    /// Byte-level verdicts and imported measurements, independently restorable.
    pub conclusions: Part<Conclusions>,
    /// What the user said. The part nothing can rebuild.
    pub user: Part<UserData>,
    /// What other sources said. Re-fetchable, slowly.
    pub sources: Part<Sources>,
    /// Private salted password verifiers and stable personal owners; no sessions.
    pub accounts: Part<Accounts>,
}

impl Backup {
    /// `true` when there is nothing in here worth writing.
    ///
    /// A backup of an empty data folder would restore an empty data folder, and
    /// reporting success for it teaches the reader that the command worked —
    /// which is exactly the belief that costs them the library later.
    pub fn is_empty(&self) -> bool {
        self.catalog.held().is_none()
            && self.conclusions.held().is_none()
            && self.user.held().is_none()
            && self.sources.held().is_none()
            && self.accounts.held().is_none()
    }
}

/// The envelope, with the five documents nested unchanged.
pub fn to_json(backup: &Backup) -> Json {
    let mut root = Json::obj();
    root.set("format_version", BACKUP_FORMAT_VERSION.into());
    root.set("made_at", backup.made_at.into());
    root.set("made_by", backup.made_by.clone().into());
    // Written even when there is nothing to write, as `null`. A key that is
    // simply absent cannot be told from a file that was cut short.
    root.set(
        "catalog",
        match backup.catalog.held() {
            Some(catalog) => crate::store::to_json(catalog),
            None => Json::Null,
        },
    );
    root.set(
        "conclusions",
        match backup.conclusions.held() {
            Some(conclusions) => crate::conclusions::to_json(conclusions),
            None => Json::Null,
        },
    );
    root.set(
        "user",
        match backup.user.held() {
            Some(user) => crate::user::to_json(user),
            None => Json::Null,
        },
    );
    root.set(
        "sources",
        match backup.sources.held() {
            Some(sources) => crate::sources::to_json(sources),
            None => Json::Null,
        },
    );
    root.set(
        "accounts",
        match backup.accounts.held() {
            Some(accounts) => crate::accounts::to_json(accounts),
            None => Json::Null,
        },
    );
    root
}

/// Reads back what [`to_json`] wrote.
///
/// Versions 1, 2 and 3 are understood; other envelope shapes are refused rather
/// than guessed at. A *store* inside a readable
/// envelope is never fatal: it comes back as [`Part::Unreadable`] with what its
/// own reader said, and the other stores remain restorable.
pub fn from_json(value: &Json) -> Result<Backup, StoreError> {
    let found = value.field_u32("format_version").unwrap_or(0);
    if !matches!(found, 1..=3) {
        return Err(StoreError::Version {
            found,
            expected: BACKUP_FORMAT_VERSION,
        });
    }
    let catalog = Part::read(value, "catalog", crate::store::from_json)?;
    let conclusions = if found == 1 {
        match catalog.held() {
            Some(catalog) => {
                let gathered = Conclusions::from_catalog(catalog);
                if gathered.files.is_empty() && gathered.analyses.is_empty() {
                    Part::Empty
                } else {
                    Part::Held(gathered)
                }
            }
            None => Part::Empty,
        }
    } else {
        Part::read(value, "conclusions", crate::conclusions::from_json)?
    };
    Ok(Backup {
        made_at: value.field_u64("made_at").unwrap_or(0),
        made_by: value.field_str("made_by").unwrap_or_default(),
        catalog,
        conclusions,
        user: Part::read(value, "user", crate::user::from_json)?,
        sources: Part::read(value, "sources", crate::sources::from_json)?,
        accounts: if found >= 3 {
            Part::read(value, "accounts", crate::accounts::from_json)?
        } else {
            Part::Empty
        },
    })
}

/// Writes a backup through isolated temporary output and atomic replacement.
///
/// The parent folder must already exist. Final symbolic links and special
/// destinations are refused; replacing a hard-link alias never writes through
/// to its original inode. New files are private to their owner on Unix;
/// existing destination permissions are preserved. Caller confirmation belongs
/// to the CLI. This does not guarantee power-loss durability or protect against
/// another process concurrently replacing ancestor directories.
pub fn write(backup: &Backup, path: &Path) -> Result<(), StoreError> {
    if !matches!(backup.accounts, Part::Empty) {
        crate::accounts::check_private_file(path)?;
    }
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !std::fs::metadata(parent)?.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "backup parent is not a folder",
        )
        .into());
    }
    // Pretty rather than compact, and deliberately: this file exists to be
    // opened by a worried person, and half of what it is for is that they can
    // see their notes are in there.
    crate::atomic_file::write(path, to_json(backup).to_string_pretty().as_bytes())?;
    Ok(())
}

/// Reads a backup from an ordinary local file, refusing blocking special files.
/// A backup with credentials also requires a private, non-symlink source.
/// The opened file's identity and permissions are checked, and changes during
/// reading are refused. Reading cannot grow past its inspected length.
pub fn read(path: &Path) -> Result<Backup, StoreError> {
    read_with_open(path, |path| std::fs::File::open(path))
}

fn read_with_open(
    path: &Path,
    open: impl FnOnce(&Path) -> std::io::Result<std::fs::File>,
) -> Result<Backup, StoreError> {
    let path_metadata = std::fs::symlink_metadata(path)?;
    let inspected = std::fs::metadata(path)?;
    check_regular_source(&inspected)?;
    let mut file = open(path)?;
    let opened = file.metadata()?;
    check_regular_source(&opened)?;
    check_unchanged_source(&inspected, &opened)?;
    let text = read_document(&mut file, opened.len())?;
    check_unchanged_source(&opened, &file.metadata()?)?;
    check_unchanged_source(&opened, &std::fs::metadata(path)?)?;
    let value = crate::json::parse(&text).map_err(StoreError::Parse)?;
    let backup = from_json(&value)?;
    if !matches!(backup.accounts, Part::Empty) {
        crate::accounts::validate_private_read(&path_metadata, &opened)?;
        crate::accounts::validate_private_read(&opened, &file.metadata()?)?;
        crate::accounts::validate_private_read(&opened, &std::fs::symlink_metadata(path)?)?;
    }
    Ok(backup)
}

fn check_regular_source(metadata: &std::fs::Metadata) -> Result<(), StoreError> {
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "backup source is not an ordinary file",
        )
        .into());
    }
    Ok(())
}

fn check_unchanged_source(
    inspected: &std::fs::Metadata,
    current: &std::fs::Metadata,
) -> Result<(), StoreError> {
    let changed =
        inspected.len() != current.len() || inspected.modified()? != current.modified()?;
    #[cfg(unix)]
    let changed = {
        use std::os::unix::fs::MetadataExt;
        changed || inspected.dev() != current.dev() || inspected.ino() != current.ino()
    };
    if changed {
        return Err(std::io::Error::other("backup source changed while reading; retry").into());
    }
    Ok(())
}

fn read_document(input: &mut impl Read, expected: u64) -> Result<String, StoreError> {
    let limit = expected.checked_add(1).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "backup source is too large",
        )
    })?;
    let mut text = String::new();
    input.take(limit).read_to_string(&mut text)?;
    if text.len() as u64 != expected {
        return Err(std::io::Error::other("backup source changed while reading; retry").into());
    }
    Ok(text)
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
