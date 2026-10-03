//! Filesystem checks and isolated temporary output shared by copying and encoding.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Refuses missing or special sources without opening a potentially blocking
/// FIFO/device. Source symlinks may resolve to an ordinary readable file.
pub fn validate_source(source: &Path) -> Result<(), String> {
    let metadata =
        std::fs::metadata(source).map_err(|error| format!("{}: {error}", source.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "{}: source is not an ordinary file",
            source.display()
        ));
    }
    Ok(())
}

/// Tests hard-link publication inside an isolated directory, before transfer.
///
/// Call only for a real transfer with new outputs and without explicit
/// replacement. No-write previews and existing-only resumes do not need it.
pub fn validate_new_publication(base: &Path) -> Result<(), String> {
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temporary = TemporaryOutput::new(
        &base.join(format!(".aede-link-check-{}-{serial}", std::process::id())),
    )?;
    std::fs::write(temporary.path(), b"").map_err(|error| error.to_string())?;
    std::fs::hard_link(temporary.path(), temporary.directory.join("published"))
        .map_err(|error| new_publication_error(base, error))
}

fn new_publication_error(destination: &Path, error: std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::AlreadyExists => format!(
            "{}: output appeared before publication; use --replace to refresh it",
            destination.display()
        ),
        _ => format!(
            "{}: cannot safely publish a new output (filesystem hard links required; --replace explicitly allows rename publication): {error}",
            destination.display()
        ),
    }
}

/// Checks every existing component below a canonical destination base.
///
/// Symbolic links are refused even when they currently resolve inside the base:
/// following them would make a later replacement depend on a different tree.
/// Missing components are allowed; only ordinary directories may be created.
/// This is a local filesystem safety check, not protection against a process
/// concurrently replacing directories while the command runs.
pub fn validate_destination(base: &Path, relative: &Path) -> Result<(), String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "{}: output must be a relative file path",
            relative.display()
        ));
    }
    let mut path = base.to_path_buf();
    let count = relative.components().count();
    for (index, part) in relative.components().enumerate() {
        path.push(part.as_os_str());
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "{}: symbolic links are refused in a copy destination",
                    path.display()
                ));
            }
            Ok(meta) if index + 1 < count && !meta.is_dir() => {
                return Err(format!("{}: output parent is not a folder", path.display()));
            }
            Ok(meta) if index + 1 == count && !meta.is_file() => {
                return Err(format!(
                    "{}: output is not an ordinary file",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    Ok(())
}

/// Checks planned names against the destination's actual filesystem rules.
///
/// The namespace is mirrored inside one isolated temporary directory, then
/// removed. Exclusive file and directory creation detects case/Unicode aliases
/// missed by the portable planner before any real output is changed. Callers
/// must omit this write-based preflight for a no-write dry run.
pub fn validate_filesystem_names(base: &Path, plan: &super::Plan) -> Result<(), String> {
    use std::collections::BTreeSet;
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temporary = TemporaryOutput::new(
        &base.join(format!(".aede-names-check-{}-{serial}", std::process::id())),
    )?;
    let tree = temporary.directory.join("tree");
    std::fs::create_dir(&tree).map_err(|error| error.to_string())?;
    let mut directories = BTreeSet::new();
    for item in &plan.items {
        validate_destination(base, &item.relative)?;
        let parts: Vec<_> = item.relative.components().collect();
        let mut relative = PathBuf::new();
        for component in &parts[..parts.len() - 1] {
            relative.push(component.as_os_str());
            if directories.insert(relative.clone()) {
                std::fs::create_dir(tree.join(&relative)).map_err(|error| format!(
                    "{}: destination aliases a different planned folder or refuses its name: {error}", relative.display()
                ))?;
            }
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(tree.join(&item.relative))
            .map_err(|error| {
                format!(
                    "{}: destination aliases another planned output or refuses its name: {error}",
                    item.relative.display()
                )
            })?;
    }
    Ok(())
}

pub(super) fn regular_destination(destination: &Path) -> Result<Option<std::fs::Metadata>, String> {
    match std::fs::symlink_metadata(destination) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => Err(format!(
            "{}: refusing to replace or follow a non-regular destination",
            destination.display()
        )),
        Ok(meta) => Ok(Some(meta)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", destination.display())),
    }
}

/// One exclusively created temporary directory beside a destination file.
///
/// The output lives inside a private directory, so an old `.aede-partial` file
/// cannot redirect either a copy or ffmpeg. The final extension is retained for
/// encoder muxer selection. Dropping the object removes unfinished output.
pub struct TemporaryOutput {
    directory: PathBuf,
    path: PathBuf,
    destination: PathBuf,
}

impl TemporaryOutput {
    /// Prepares an isolated output after refusing a non-regular destination.
    ///
    /// Call [`validate_destination`] with the chosen base before this method
    /// when writing a path taken from a copy plan.
    pub fn new(destination: &Path) -> Result<Self, String> {
        regular_destination(destination)?;
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        let parent = parent
            .canonicalize()
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        let name = destination
            .file_name()
            .ok_or_else(|| "output has no file name".to_string())?;
        let destination = parent.join(name);
        let clock = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..100 {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(
                ".aede-partial-{}-{clock}-{serial}",
                std::process::id()
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&directory) {
                Ok(()) => {
                    let mut path = directory.join("output");
                    if let Some(extension) = destination.extension() {
                        path.set_extension(extension);
                    }
                    return Ok(Self {
                        directory,
                        path,
                        destination,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("{}: {error}", directory.display())),
            }
        }
        Err("could not reserve a unique temporary output".into())
    }

    /// The isolated output path, carrying the final extension.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Publishes a complete file, replacing an ordinary existing target.
    ///
    /// Use [`Self::publish_new`] unless replacement was explicitly requested.
    pub fn publish(self) -> Result<(), String> {
        regular_destination(&self.destination)?;
        std::fs::rename(&self.path, &self.destination)
            .map_err(|error| format!("{}: {error}", self.destination.display()))
    }

    /// Publishes a complete new file without replacing any existing target.
    ///
    /// Creating the hard link is atomic even if a target appears after the
    /// initial check. Filesystems without hard links are refused; an explicit
    /// replacement can instead use [`Self::publish`]. No partial final file
    /// is exposed and no check-then-rename fallback can destroy another file.
    pub fn publish_new(self) -> Result<(), String> {
        std::fs::hard_link(&self.path, &self.destination)
            .map_err(|error| new_publication_error(&self.destination, error))?;
        Ok(())
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        // Windows can copy a source's read-only bit, which otherwise prevents
        // temporary-link deletion. Restore it through the retained file
        // handle: successful no-replace publication shares that inode, and
        // a pathname can meanwhile be replaced by another writer.
        #[cfg(windows)]
        let restore = std::fs::File::open(&self.path).ok().and_then(|file| {
            let permissions = file.metadata().ok()?.permissions();
            if !permissions.readonly() {
                return None;
            }
            let mut writable = permissions.clone();
            writable.set_readonly(false);
            file.set_permissions(writable).ok()?;
            Some((file, permissions))
        });
        let _ = std::fs::remove_dir_all(&self.directory);
        #[cfg(windows)]
        if let Some((file, permissions)) = restore {
            let _ = file.set_permissions(permissions);
        }
    }
}

/// Compares file contents using fixed-size buffers, independent of file size.
pub fn files_match(left: &Path, right: &Path) -> Result<bool, String> {
    use std::io::Read;
    let mut left = std::fs::File::open(left).map_err(|error| error.to_string())?;
    let mut right = std::fs::File::open(right).map_err(|error| error.to_string())?;
    if left.metadata().map_err(|error| error.to_string())?.len()
        != right.metadata().map_err(|error| error.to_string())?.len()
    {
        return Ok(false);
    }
    let mut a = vec![0; 1 << 20];
    let mut b = vec![0; 1 << 20];
    loop {
        let n = left.read(&mut a).map_err(|error| error.to_string())?;
        if n == 0 {
            return Ok(right.read(&mut b).map_err(|error| error.to_string())? == 0);
        }
        right
            .read_exact(&mut b[..n])
            .map_err(|error| error.to_string())?;
        if a[..n] != b[..n] {
            return Ok(false);
        }
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
