//! Isolated persisted data and terminal probes for destructive CLI modes.

#[path = "../fetch_output_support/mod.rs"]
#[allow(dead_code)]
mod library;

use aede_core::analysis::FileAnalysis;
use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{ReviewDecision, SourceReview, Sources};
use aede_core::user::{EntityRef, LOCAL_USER, UserData};
use aede_core::{sources, store, user};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

pub struct Fixture {
    pub library: library::Library,
}

pub type Snapshot = Vec<(Option<Vec<u8>>, Option<SystemTime>)>;

impl Fixture {
    pub fn new() -> Self {
        let library = library::Library::new("Confirmation", false);
        let mut catalog = store::load(&store::catalog_path(&library.data))
            .unwrap()
            .unwrap();
        catalog.analyses = [
            (library.audio.clone(), "flaccompagnon"),
            (library.music.join("waiting/02.flac"), "flaccompagnon"),
            (library.music.join("waiting/03.flac"), "another-tool"),
        ]
        .into_iter()
        .map(|(path, source)| FileAnalysis {
            path: path.to_str().unwrap().into(),
            source: source.into(),
            ..Default::default()
        })
        .collect();
        store::save(&catalog, &store::catalog_path(&library.data)).unwrap();
        let mut held = sources::load_all(&sources::sources_path(&library.data))
            .unwrap()
            .unwrap();
        let mut other = held.records[0].clone();
        other.source = "another-provider".into();
        held.set(other);
        for record in held.records.clone() {
            held.set_review(SourceReview {
                entity: record.entity(),
                source: record.source,
                source_id: record.source_id,
                decision: ReviewDecision::Rejected,
                reviewed_at: 1,
            });
        }
        held.exclude_credit(
            aede_core::graph::RelationRef {
                source: EntityRef::new(EntityKind::Artist, "miles davis"),
                kind: "credit:performer".into(),
                target: EntityRef::new(EntityKind::Recording, "recording-fixture"),
                provenance: sources::MUSICBRAINZ.into(),
                source_id: None,
            },
            1,
        );
        sources::save(&held, &sources::sources_path(&library.data)).unwrap();
        let mut personal = UserData::default();
        personal
            .entry(
                LOCAL_USER,
                &EntityRef::of(&catalog, EntityKind::Track, 0).unwrap(),
                1,
            )
            .note = Some("preserve this personal note".into());
        user::save(&personal, &user::user_path(&library.data)).unwrap();
        Self { library }
    }

    pub fn data(&self) -> &Path {
        &self.library.data
    }

    pub fn catalog(&self) -> Catalog {
        store::load(&store::catalog_path(self.data()))
            .unwrap()
            .unwrap()
    }

    pub fn sources(&self) -> Sources {
        sources::load_all(&sources::sources_path(self.data()))
            .unwrap()
            .unwrap()
    }

    pub fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
        command
            .args(arguments)
            .env("AEDE_HOME", self.data())
            .env("NO_COLOR", "1")
            .env_remove("AEDE_DELEGATED_CHILD")
            .env_remove("AEDE_DELEGATED_DATA_DIR")
            .stdin(Stdio::null());
        command
    }

    pub fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }

    pub fn snapshot(&self) -> Snapshot {
        [
            "catalog.json",
            "conclusions.json",
            "sources.json",
            "user.json",
            "accounts.json",
            "parental.json",
        ]
        .into_iter()
        .map(|name| {
            let path = self.data().join(name);
            (
                std::fs::read(&path).ok(),
                std::fs::metadata(path).and_then(|m| m.modified()).ok(),
            )
        })
        .collect()
    }

    #[cfg(unix)]
    pub fn terminal(&self, command: &str, answer: &str) -> aede_core::json::Json {
        let output = Command::new("python3")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/forget_confirmation_support/terminal.py"),
            )
            .arg(env!("CARGO_BIN_EXE_aede"))
            .arg(self.data())
            .arg(command)
            .arg(answer)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "terminal probe: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        aede_core::json::parse(&String::from_utf8(output.stdout).unwrap()).unwrap()
    }
}

#[cfg(unix)]
pub struct Server(std::process::Child);

#[cfg(unix)]
impl Server {
    pub fn start(fixture: &Fixture) -> Self {
        let mut server = Self(
            fixture
                .command(&["serve", "--port=0"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut ready = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(server.0.stdout.as_mut().unwrap()),
            &mut ready,
        )
        .unwrap();
        assert!(ready.contains("Aède API:"), "server readiness: {ready}");
        server
    }
}

#[cfg(unix)]
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
