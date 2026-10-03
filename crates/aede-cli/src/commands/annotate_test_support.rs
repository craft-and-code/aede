//! Temporary personal store for CLI annotation tests.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct NotesStore {
    pub path: PathBuf,
    pub from: EntityRef,
    pub to: EntityRef,
}

impl NotesStore {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "aede-notes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
        let to = EntityRef::new(EntityKind::Track, "/new/a.flac");
        let file = aede_core::model::ScannedFile {
            path: to.key.clone(),
            size: 1,
            mtime: 1,
            tags: aede_core::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        };
        let catalog = aede_core::model::build(vec![file], vec!["/new".into()], 1, &[]);
        store::save(&catalog, &store::catalog_path(&path)).unwrap();
        let mut data = UserData::default();
        data.entry(LOCAL_USER, &from, 1).note = Some("original wording\n\n".into());
        user::save(&data, &user::user_path(&path)).unwrap();
        Self { path, from, to }
    }

    pub fn args(&self, options: &[String]) -> Args {
        Args::parse(
            [
                "notes".to_string(),
                format!("--data={}", self.path.display()),
            ]
            .into_iter()
            .chain(options.iter().cloned()),
        )
    }

    pub fn bytes(&self) -> Vec<u8> {
        std::fs::read(user::user_path(&self.path)).unwrap()
    }
    pub fn data(&self) -> UserData {
        user::load(&user::user_path(&self.path)).unwrap().unwrap()
    }
}

impl Drop for NotesStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
