//! Process-wide coordination for the JSON documents in one Aède data folder.
//!
//! The lock is separate from the atomically replaced JSON files. Never remove
//! it: unlinking a locked inode would let another process lock a new inode.
//! Symbolic links and special lock files are refused. New lock files are
//! owner-only on Unix, while existing permissions are preserved.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const LOCK_FILE: &str = ".aede.lock";

/// Holds the exclusive data-folder lock until dropped.
///
/// Drop explicitly unlocks before closing the file. Closing alone may retain
/// the lock while a helper process briefly holds an inherited descriptor.
pub struct StoreLock {
    _file: File,
}

impl StoreLock {
    /// Waits for another Aède operation to finish, then locks this data folder.
    pub fn acquire(data_dir: &Path) -> io::Result<Self> {
        let file = open_lock(data_dir)?;
        file.lock()?;
        Ok(Self { _file: file })
    }

    /// Fails with [`io::ErrorKind::WouldBlock`] if another operation owns it.
    pub fn try_acquire(data_dir: &Path) -> io::Result<Self> {
        let file = open_lock(data_dir)?;
        file.try_lock()?;
        Ok(Self { _file: file })
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        // File locks otherwise live until every duplicated/inherited handle
        // closes. Drop cannot return an unlock error; closing our File remains
        // the fallback, as with File's own destructor.
        let _ = self._file.unlock();
    }
}

fn open_lock(data_dir: &Path) -> io::Result<File> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join(LOCK_FILE);
    let inspected = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => {
            check_regular_lock(&metadata)?;
            Some(metadata)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    let opened = file.metadata()?;
    check_regular_lock(&opened)?;
    let published = std::fs::symlink_metadata(&path)?;
    check_regular_lock(&published)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let same = |metadata: &std::fs::Metadata| {
            metadata.dev() == opened.dev() && metadata.ino() == opened.ino()
        };
        if !same(&published) || inspected.as_ref().is_some_and(|metadata| !same(metadata)) {
            return Err(io::Error::other("writer lock changed while being opened"));
        }
    }
    #[cfg(not(unix))]
    let _ = inspected;
    Ok(file)
}

fn check_regular_lock(metadata: &std::fs::Metadata) -> io::Result<()> {
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "writer lock must be an ordinary non-symlink file",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_lock_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "persistence_test_support.rs"]
pub(crate) mod test_support;
