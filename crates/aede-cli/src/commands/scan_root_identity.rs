//! Scan-only resolution of declared unavailable roots without losing catalog identity.

use aede_core::model::Catalog;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
};

const MAX_LINKS: usize = 40;
const MAX_COMPONENTS: usize = 4096;

pub(super) fn resolve(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    resolve_path(path).map_err(|error| identity_error(path, &error.to_string()))
}

fn identity_error(path: &Path, reason: &str) -> Box<dyn Error> {
    format!(
        "cannot establish root identity for {path:?}: {reason}; reconnect the folder or select its real path before replacing the catalog"
    )
    .into()
}

fn resolve_path(path: &Path) -> io::Result<PathBuf> {
    if let Ok(resolved) = path.canonicalize() {
        return utf8_path(resolved);
    }
    let absolute = std::path::absolute(path)?;
    let mut pending: VecDeque<OsString> = absolute
        .components()
        .map(|part| part.as_os_str().to_owned())
        .collect();
    let mut resolved = PathBuf::new();
    let mut missing = false;
    let mut links = 0;
    let mut components = 0;
    while let Some(part) = pending.pop_front() {
        components += 1;
        if components > MAX_COMPONENTS {
            return Err(io::Error::other("too many path components"));
        }
        match Path::new(&part).components().next() {
            Some(Component::Prefix(_) | Component::RootDir) => resolved.push(&part),
            Some(Component::CurDir) => {}
            Some(Component::ParentDir) => {
                // An absent component could previously have been a link. Its
                // parent cannot be inferred by flattening the spelling.
                if missing {
                    return Err(io::Error::other("parent of an absent component is unknown"));
                }
                resolved.pop();
            }
            Some(Component::Normal(_)) => {
                if missing {
                    resolved.push(&part);
                    continue;
                }
                resolved.push(&part);
                match fs::symlink_metadata(&resolved) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        links += 1;
                        if links > MAX_LINKS {
                            return Err(io::Error::other("too many symbolic links"));
                        }
                        let target = fs::read_link(&resolved)?;
                        if target.is_absolute() {
                            resolved.clear();
                        } else {
                            resolved.pop();
                        }
                        for part in target.components().rev() {
                            pending.push_front(part.as_os_str().to_owned());
                        }
                    }
                    Ok(metadata) if !metadata.is_dir() && !pending.is_empty() => {
                        return Err(io::Error::other("path ancestor is not a directory"));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        resolved.pop();
                        // Preserve the platform's canonical spelling of every
                        // existing ancestor before appending missing names.
                        resolved = resolved.canonicalize()?;
                        resolved.push(&part);
                        missing = true;
                    }
                    Err(error) => return Err(error),
                }
            }
            None => {}
        }
    }
    utf8_path(if missing {
        resolved
    } else {
        resolved.canonicalize()?
    })
}

fn utf8_path(path: PathBuf) -> io::Result<PathBuf> {
    if path.to_str().is_none() {
        return Err(io::Error::other("path cannot be represented as UTF-8"));
    }
    Ok(path)
}

pub(super) fn validate_selection(
    previous: Option<&Catalog>,
    roots: &[PathBuf],
    unavailable: &[(PathBuf, PathBuf)],
) -> Result<(), Box<dyn Error>> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if previous
        .roots
        .iter()
        .all(|old| roots.iter().any(|root| Path::new(old).starts_with(root)))
    {
        return Ok(());
    }
    for (raw, identity) in unavailable {
        // The catalog retains canonical paths, not a map of configured
        // aliases. A retargeted or removed link cannot identify which old root
        // was intentionally dropped, even if its new target is also known.
        if raw != identity || !known_identity(previous, identity) {
            return Err(identity_error(
                raw,
                "the previous root association is unknown",
            ));
        }
    }
    Ok(())
}

fn known_identity(previous: &Catalog, identity: &Path) -> bool {
    previous.roots.iter().any(|old| Path::new(old) == identity)
        || previous
            .files
            .iter()
            .any(|file| Path::new(&file.path).starts_with(identity))
}

pub(super) fn validate_result(
    previous: Option<&Catalog>,
    current: &Catalog,
    unavailable: &[(PathBuf, PathBuf)],
    excluded: &[PathBuf],
) -> Result<(), Box<dyn Error>> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let Some((raw, _)) = unavailable
        .iter()
        .find(|(raw, identity)| raw != identity || !known_identity(previous, identity))
    else {
        return Ok(());
    };
    let present: BTreeSet<_> = current
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut excluded_directories = BTreeMap::new();
    // Root deduplication can hide an alias's former target beneath another
    // watched parent. Root coverage alone cannot establish that its files
    // survived; refuse before publishing any graph or private-store changes.
    if previous.files.iter().any(|old| {
        !present.contains(old.path.as_str())
            && !excluded_file(Path::new(&old.path), excluded, &mut excluded_directories)
    }) {
        return Err(identity_error(
            raw,
            "the scan would remove files while the previous root association is unknown",
        ));
    }
    Ok(())
}

fn excluded_file(
    path: &Path,
    excluded: &[PathBuf],
    directories: &mut BTreeMap<PathBuf, Option<PathBuf>>,
) -> bool {
    if excluded.is_empty() {
        return false;
    }
    if excluded.iter().any(|root| path.starts_with(root)) {
        return true;
    }
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    let Some(identity) = directories
        .entry(directory.to_path_buf())
        .or_insert_with(|| resolve_path(directory).ok())
        .as_ref()
    else {
        return false;
    };
    // Only an established exclusion can waive the missing-file guard. Unknown
    // historical aliases still require recovery before any store is replaced.
    let identity = identity.join(name);
    if excluded.iter().any(|root| identity.starts_with(root)) {
        return true;
    }
    if fs::symlink_metadata(&identity).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return resolve_path(&identity)
            .is_ok_and(|identity| excluded.iter().any(|root| identity.starts_with(root)));
    }
    false
}

#[cfg(test)]
#[path = "scan_root_identity_tests.rs"]
mod tests;
