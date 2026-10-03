//! Disposable copies for the stream-audit regressions.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct AudioCopy {
    directory: PathBuf,
    path: PathBuf,
}

impl AudioCopy {
    pub(super) fn new(extension: &str, bytes: &[u8]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede-stream-audit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("copy.{extension}"));
        std::fs::write(&path, bytes).unwrap();
        Self { directory, path }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for AudioCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(super) fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
