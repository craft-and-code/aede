//! Metadata-only identities for scan exclusions and unavailable followed aliases.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

const MAX_LINKS: usize = 40;
const MAX_COMPONENTS: usize = 4096;

pub(super) struct Exclusions<'a> {
    folders: &'a [PathBuf],
    /// Share parent resolution across followed entries and retained files.
    directories: HashMap<PathBuf, Option<PathBuf>>,
}

impl<'a> Exclusions<'a> {
    pub(super) fn new(folders: &'a [PathBuf]) -> Self {
        Self {
            folders,
            directories: HashMap::new(),
        }
    }

    pub(super) fn contains_file(&mut self, file: &str) -> bool {
        if self.folders.is_empty() {
            return false;
        }
        if self
            .folders
            .iter()
            .any(|folder| crate::text::is_under(file, &folder.to_string_lossy()))
        {
            return true;
        }
        let Some(identity) = self.parent_identity(Path::new(file)) else {
            // A vanished link cannot establish which old files were excluded.
            return false;
        };
        if self
            .folders
            .iter()
            .any(|folder| identity.starts_with(folder))
        {
            return true;
        }
        resolve_entry(identity).is_some_and(|identity| {
            self.folders
                .iter()
                .any(|folder| identity.starts_with(folder))
        })
    }

    pub(super) fn file_identity(&mut self, file: &Path) -> Option<PathBuf> {
        resolve_entry(self.parent_identity(file)?)
    }

    fn parent_identity(&mut self, file: &Path) -> Option<PathBuf> {
        let (Some(directory), Some(name)) = (file.parent(), file.file_name()) else {
            return None;
        };
        self.directories
            .entry(directory.to_path_buf())
            .or_insert_with(|| resolve_path(directory))
            .as_ref()
            .map(|directory| directory.join(name))
    }
}

fn resolve_entry(identity: PathBuf) -> Option<PathBuf> {
    match std::fs::symlink_metadata(&identity) {
        Ok(metadata) if metadata.file_type().is_symlink() => resolve_path(&identity),
        Ok(_) => Some(identity),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(identity),
        Err(_) => None,
    }
}

/// Resolve surviving links before appending a missing suffix, including the
/// final file link. Catalog path spelling is unchanged.
fn resolve_path(path: &Path) -> Option<PathBuf> {
    if let Ok(identity) = path.canonicalize() {
        return Some(identity);
    }
    let absolute = std::path::absolute(path).ok()?;
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
            return None;
        }
        match Path::new(&part).components().next() {
            Some(Component::Prefix(_) | Component::RootDir) => resolved.push(&part),
            Some(Component::CurDir) => {}
            Some(Component::ParentDir) => {
                // The missing component could itself have been a symlink.
                if missing {
                    return None;
                }
                resolved.pop();
            }
            Some(Component::Normal(_)) => {
                if missing {
                    resolved.push(&part);
                    continue;
                }
                resolved.push(&part);
                match std::fs::symlink_metadata(&resolved) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        links += 1;
                        if links > MAX_LINKS {
                            return None;
                        }
                        let target = std::fs::read_link(&resolved).ok()?;
                        if target.is_absolute() {
                            resolved.clear();
                        } else {
                            resolved.pop();
                        }
                        for part in target.components().rev() {
                            pending.push_front(part.as_os_str().to_owned());
                        }
                    }
                    Ok(metadata) if !metadata.is_dir() && !pending.is_empty() => return None,
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        resolved.pop();
                        resolved = resolved.canonicalize().ok()?;
                        resolved.push(&part);
                        missing = true;
                    }
                    Err(_) => return None,
                }
            }
            None => {}
        }
    }
    if missing {
        Some(resolved)
    } else {
        resolved.canonicalize().ok()
    }
}
