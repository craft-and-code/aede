//! Shared in-memory catalog fixture for personal-data tests.

use crate::model::{self, Catalog};

pub(super) fn library(paths: &[&str]) -> Catalog {
    let files: Vec<_> = paths
        .iter()
        .map(|p| {
            model::tests::track(
                p,
                &[
                    ("title", "T"),
                    ("artist", "Deicide"),
                    ("album", "Legion"),
                    ("date", "1992"),
                ],
                1000,
            )
        })
        .collect();
    model::build(files, vec!["/m".into()], 0, &[])
}

pub(super) struct TemporaryUser(pub(super) std::path::PathBuf);

impl TemporaryUser {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede_user_retention_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    pub(super) fn path(&self) -> std::path::PathBuf {
        super::user_path(&self.0)
    }
}

impl Drop for TemporaryUser {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
