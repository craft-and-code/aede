//! Isolated binary files for metadata-reader regressions.

use std::path::PathBuf;

pub(super) struct BinaryFile {
    directory: PathBuf,
    path: PathBuf,
}

impl BinaryFile {
    pub(super) fn new(extension: &str, bytes: &[u8]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "aede_metadata_{}_{nonce}_{serial}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join(format!("track.{extension}"));
        std::fs::write(&path, bytes).unwrap();
        Self { directory, path }
    }

    pub(super) fn read(&self) -> Result<super::RawTags, super::TagError> {
        super::read(&self.path)
    }
}

impl Drop for BinaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
