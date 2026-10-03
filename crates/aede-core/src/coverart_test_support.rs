use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const JPEG: &[u8] = include_bytes!("../tests/fixtures/images/baseline.jpg");
pub(super) const PNG: &[u8] = include_bytes!("../tests/fixtures/images/rgba.png");

pub(super) struct ImageFolder(PathBuf);

impl ImageFolder {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede-coverart-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("an exclusive test folder");
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ImageFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
