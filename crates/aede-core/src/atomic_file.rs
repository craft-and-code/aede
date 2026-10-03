//! Replacement writes shared by the independent JSON stores.
//!
//! Existing file permissions are preserved. New stores are created with
//! owner-only read/write permissions on Unix, independently of the umask.
//!
//! The copy staging primitive reserves a private directory beside the target;
//! a predictable old `.json.tmp` path is never opened. Publication refuses a
//! symbolic link or other non-regular target and leaves the old file intact
//! when writing fails. This does not defend against another process replacing
//! ancestor directories concurrently, or guarantee durability after power loss.

use std::io;
use std::io::Write;
use std::path::Path;

pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let permissions = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let output = crate::copy::TemporaryOutput::new(path).map_err(io::Error::other)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(output.path())?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    drop(file);
    output.publish().map_err(io::Error::other)
}

#[cfg(all(test, unix))]
#[path = "atomic_file_tests.rs"]
mod tests;
