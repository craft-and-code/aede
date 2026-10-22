//! Shared in-memory catalog fixture for personal-data tests.

use crate::model::{self, Catalog};

pub(super) fn owner_scoped_rows() -> [(&'static str, crate::json::Json); 7] {
    [
        ("annotations", r#"{"target":"artist:example","note":"Private note"}"#),
        (
            "relation_annotations",
            r#"{"source":"artist:example","relation":"member of","target":"artist:ensemble","provenance":"manual","note":"Private relation note"}"#,
        ),
        ("plays", r#"{"track":"track:/music/01.flac","at":10,"ms_played":1000,"completed":true}"#),
        ("counts", r#"{"track":"track:/music/01.flac","count":1,"completed_count":1,"last_played":10}"#),
        ("collections", r#"{"name":"Private selection","query":"genre:jazz"}"#),
        ("set_aside", r#"{"release_group":"example-release-group","title":"Example release"}"#),
        ("same_artist", r#"{"spelling":"example alias","filed_as":"example artist"}"#),
    ]
    .map(|(table, text)| (table, crate::json::parse(text).unwrap()))
}

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
