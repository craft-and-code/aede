//! A synthetic catalog and an isolated personal store for command regressions.

use aede_core::model::{self, Catalog, EntityKind, ScannedFile};
use aede_core::user::{self, EntityRef, LOCAL_USER, UserData};
use aede_core::{store, tags};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct PersonalLibrary {
    pub dir: PathBuf,
    pub catalog: Catalog,
    pub track: EntityRef,
}

impl PersonalLibrary {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede-personal-commands-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut tags = tags::RawTags::default();
        tags.insert("title", "Song");
        tags.insert("artist", "Artist");
        tags.insert("album", "Album");
        let catalog = model::build(
            vec![ScannedFile {
                path: "/music/Album/01.flac".into(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec!["/music".into()],
            1,
            &[],
        );
        store::save(&catalog, &store::catalog_path(&dir)).unwrap();
        let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
        let mut data = UserData::default();
        data.entry(LOCAL_USER, &track, 1).note = Some("preserve this note".into());
        user::save(&data, &user::user_path(&dir)).unwrap();
        Self {
            dir,
            catalog,
            track,
        }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .arg(format!("--data={}", self.dir.display()))
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    pub fn bytes(&self) -> Vec<u8> {
        std::fs::read(user::user_path(&self.dir)).unwrap()
    }

    pub fn data(&self) -> UserData {
        user::load(&user::user_path(&self.dir)).unwrap().unwrap()
    }

    pub fn save(&self, data: &UserData) {
        user::save(data, &user::user_path(&self.dir)).unwrap();
    }
}

impl Drop for PersonalLibrary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
