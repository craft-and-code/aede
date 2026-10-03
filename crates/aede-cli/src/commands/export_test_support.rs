//! Isolated output destinations for export safety tests.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Outputs {
    pub directory: PathBuf,
}

impl Outputs {
    pub fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "aede-export-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        Self { directory }
    }

    pub fn args(&self, path: &std::path::Path) -> Args {
        Args::parse([
            "notes".into(),
            format!("--data={}", self.directory.display()),
            format!("--output={}", path.display()),
        ])
    }

    pub fn flac(&self, name: &str) -> PathBuf {
        let path = self.directory.join(name);
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-core/tests/fixtures/track.flac");
        std::fs::copy(fixture, &path).unwrap();
        path
    }
}

impl Drop for Outputs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
