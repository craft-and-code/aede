use std::path::{Path, PathBuf};

pub(super) struct Sandbox(pub(super) PathBuf);

impl Sandbox {
    pub(super) fn new() -> Self {
        static NEXT_SANDBOX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT_SANDBOX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("aede_scan_{}_{nonce}_{serial}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    pub(super) fn track(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac"),
            &path,
        )
        .unwrap();
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
