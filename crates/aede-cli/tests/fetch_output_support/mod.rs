//! Isolated local catalogs for public M1 command tests.

use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{self, ArtistFacts, Confidence, Facts, SourceRecord, Sources};
use aede_core::tags::RawTags;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Library {
    pub directory: PathBuf,
    pub music: PathBuf,
    pub data: PathBuf,
    pub audio: PathBuf,
}

impl Library {
    pub fn new(folder: &str, embedded: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede_m1_public_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let music = directory.join("music").join(folder);
        let data = directory.join("data");
        std::fs::create_dir_all(&music).unwrap();
        let music = music.canonicalize().unwrap();
        let audio = music.join("01.flac");
        std::fs::write(&audio, b"unchanged audio sentinel").unwrap();
        let mut tags = RawTags::default();
        for (field, value) in [
            ("artist", "Miles Davis"),
            ("albumartist", "Miles Davis"),
            ("album", folder),
            ("title", "So What"),
            (
                "musicbrainz_artistid",
                "561d854a-6a28-4aa7-8c99-323e6ce46c2a",
            ),
            (
                "musicbrainz_albumid",
                "59211ea4-ffd2-4ad9-9a4e-941d3148024a",
            ),
            (
                "musicbrainz_recordingid",
                "73eac6f8-522a-4eab-8cf5-1e708f666e31",
            ),
            ("label", "Columbia"),
        ] {
            tags.insert(field, value);
        }
        tags.properties.duration_ms = Some(300_000);
        tags.has_embedded_art = embedded;
        let catalog = build(
            vec![ScannedFile {
                path: audio.to_string_lossy().into_owned(),
                size: 24,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: Some(aede_core::fingerprint::Fingerprint {
                    data: "fingerprint-sentinel".into(),
                    seconds: 300,
                }),
            }],
            vec![music.to_string_lossy().into_owned()],
            1,
            &[],
        );
        aede_core::store::save(&catalog, &aede_core::store::catalog_path(&data)).unwrap();
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: "miles davis".into(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("561d854a-6a28-4aa7-8c99-323e6ce46c2a".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                wikidata: Some("https://www.wikidata.org/wiki/Q93341".into()),
                ..Default::default()
            }),
        });
        sources::save(&held, &sources::sources_path(&data)).unwrap();
        Self {
            directory,
            music,
            data,
            audio,
        }
    }

    pub fn run(&self, arguments: &[&str]) -> std::process::Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(arguments)
            .arg(format!("--data={}", self.data.display()))
            .env("NO_COLOR", "1")
            .env("AEDE_ACOUSTID_KEY", "private-test-acoustid")
            .env("AEDE_FANARTTV_KEY", "private-test-fanart")
            .output()
            .unwrap()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
