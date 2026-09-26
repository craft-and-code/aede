//! Process-wide coordination for the JSON documents in one Aède data folder.
//!
//! The lock is separate from the atomically replaced JSON files. Never remove
//! it: unlinking a locked inode would let another process lock a new inode.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const LOCK_FILE: &str = ".aede.lock";

/// Holds the exclusive data-folder lock until dropped.
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

fn open_lock(data_dir: &Path) -> io::Result<File> {
    std::fs::create_dir_all(data_dir)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join(LOCK_FILE))
}

#[cfg(test)]
#[path = "store_lock_tests.rs"]
mod tests;
