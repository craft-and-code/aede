//! Real scanned audio and stdout-terminal probes for literal folder output.

use std::path::{Path, PathBuf};
use std::process::Command;

use aede_core::{accounts, conclusions, json, parental, sources, store};

use super::support::PersonalLibrary;

struct LiteralFolder {
    library: PersonalLibrary,
    folder: PathBuf,
    audio: PathBuf,
    title: String,
    stores: [Option<Vec<u8>>; 6],
    audio_bytes: Vec<u8>,
}

impl LiteralFolder {
    fn new() -> Self {
        let library = PersonalLibrary::new();
        store::save(&Default::default(), &store::catalog_path(&library.dir)).unwrap();
        let folder = library.dir.join("folderliteral\u{1b}[31mred\u{1b}[0m");
        std::fs::create_dir(&folder).unwrap();
        let audio = folder.join("track.flac");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
            &audio,
        )
        .unwrap();
        let audio_bytes = std::fs::read(&audio).unwrap();
        let result = super::run(&library.dir, &["scan", folder.to_str().unwrap()]);
        assert!(result.status.success(), "scan failed: {result:?}");
        let folder = folder.canonicalize().unwrap();
        let audio = audio.canonicalize().unwrap();
        let catalog = store::load(&store::catalog_path(&library.dir))
            .unwrap()
            .unwrap();
        assert_eq!(catalog.releases.len(), 1);
        assert_eq!(catalog.files.len(), 1);
        assert_eq!(Path::new(&catalog.releases[0].folder), folder);
        assert_eq!(Path::new(&catalog.files[0].path), audio);
        let title = catalog.releases[0].title.clone();

        // Present stores protect actual saved bytes, rather than only absence.
        conclusions::save(
            &Default::default(),
            &conclusions::conclusions_path(&library.dir),
        )
        .unwrap();
        sources::save(&Default::default(), &sources::sources_path(&library.dir)).unwrap();
        let identities = accounts::Accounts::bootstrap("operator", "a test password", 1).unwrap();
        accounts::save(&identities, &accounts::accounts_path(&library.dir)).unwrap();
        parental::ParentalStore::default()
            .save(&parental::path(&library.dir))
            .unwrap();
        let stores = super::snapshots(&library.dir);
        assert!(stores.iter().all(Option::is_some));
        Self {
            library,
            folder,
            audio,
            title,
            stores,
            audio_bytes,
        }
    }

    fn assert_preserved(&self) {
        assert_eq!(super::snapshots(&self.library.dir), self.stores);
        assert_eq!(std::fs::read(&self.audio).unwrap(), self.audio_bytes);
        let catalog = store::load(&store::catalog_path(&self.library.dir))
            .unwrap()
            .unwrap();
        assert_eq!(Path::new(&catalog.releases[0].folder), self.folder);
        assert_eq!(Path::new(&catalog.files[0].path), self.audio);
    }

    fn terminal(&self, command: &str) -> String {
        let result = Command::new("python3")
            .arg("-B")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/cli_validation_support/terminal.py"),
            )
            .arg(env!("CARGO_BIN_EXE_aede"))
            .arg(&self.library.dir)
            .arg(command)
            .arg(&self.title)
            .output()
            .unwrap();
        assert!(result.status.success(), "terminal probe failed: {result:?}");
        let result = json::parse(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
        assert!(result.field_bool("stdout_is_terminal"), "{result:?}");
        assert_eq!(result.field_u64("code"), Some(0), "{result:?}");
        assert_eq!(
            result.field_str("stderr").as_deref(),
            Some(""),
            "{result:?}"
        );
        result.field_str("stdout").unwrap()
    }
}

pub fn assert_literal_folder(command: &str, terminal: bool) {
    let fixture = LiteralFolder::new();
    let output = if terminal {
        fixture.terminal(command)
    } else {
        let result = super::run(&fixture.library.dir, &[command, &fixture.title]);
        assert!(result.status.success(), "{command}: {result:?}");
        assert!(result.stderr.is_empty(), "{command}: {result:?}");
        String::from_utf8(result.stdout).unwrap()
    };
    fixture.assert_preserved();
    let escaped = fixture
        .folder
        .to_str()
        .unwrap()
        .replace('\u{1b}', "\\u{1b}");
    let expected = if terminal {
        format!("  \u{1b}[2m{escaped}\u{1b}[0m")
    } else {
        assert!(!super::unsafe_controls(&output), "{command}: {output:?}");
        format!("  {escaped}")
    };
    assert!(
        output.lines().any(|line| line == expected),
        "{command} did not show the complete literal folder (terminal={terminal}); expected {expected:?}, actual {output:?}"
    );
}

impl LiteralFolder {
    fn output(&self, command: &str, terminal: bool) -> String {
        if terminal {
            self.terminal(command)
        } else {
            let result = super::run(&self.library.dir, &[command, &self.title]);
            assert!(result.status.success(), "{command}: {result:?}");
            assert!(result.stderr.is_empty(), "{command}: {result:?}");
            let output = String::from_utf8(result.stdout).unwrap();
            assert!(!super::unsafe_controls(&output), "{command}: {output:?}");
            output
        }
    }

    fn with_waiting_credits(mut self, scope: &str) -> Self {
        let path = store::catalog_path(&self.library.dir);
        let mut catalog = store::load(&path).unwrap().unwrap();
        assert_eq!(catalog.recordings.len(), 1);
        match scope {
            "recording" => {
                catalog.recordings[0].mbid = Some("11111111-1111-4111-8111-111111111111".into());
            }
            "edition" => {
                catalog.releases[0].mbid = Some("22222222-2222-4222-8222-222222222222".into());
            }
            _ => panic!("unknown fixture credit scope: {scope}"),
        }
        store::save(&catalog, &path).unwrap();
        self.stores = super::snapshots(&self.library.dir);

        let result = super::run(&self.library.dir, &["credits", &self.title, "--json"]);
        assert!(result.status.success(), "{result:?}");
        assert!(result.stderr.is_empty(), "{result:?}");
        let report = json::parse(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
        let albums = report.as_arr().unwrap();
        assert_eq!(albums.len(), 1, "{report:?}");
        let album = &albums[0];
        let coverage = album.get("coverage").unwrap();
        assert_eq!(coverage.field_u64("total"), Some(1), "{report:?}");
        assert_eq!(
            coverage.field_u64("waiting"),
            Some(u64::from(scope == "recording")),
            "{report:?}"
        );
        let edition = album.get("edition").unwrap();
        assert_eq!(
            edition.field_str("status").as_deref(),
            Some(if scope == "edition" {
                "waiting"
            } else {
                "unidentified"
            }),
            "{report:?}"
        );
        assert_eq!(
            Path::new(album.get("folder").unwrap().as_str().unwrap()),
            self.folder,
            "machine output must retain the original path"
        );
        self.assert_preserved();
        self
    }
}

pub fn assert_literal_navigation(terminal: bool) {
    let fixture = LiteralFolder::new();
    let output = fixture.output("album", terminal);
    fixture.assert_preserved();
    if terminal {
        assert!(
            output.contains("\u{1b}[36m"),
            "the real terminal must retain presentation colors: {output:?}"
        );
    }
    let escaped = fixture.audio.to_str().unwrap().replace('\u{1b}', "\\u{1b}");
    let expected = format!("aede recording 'local:{escaped}'");
    let navigation = output.split_once("Continue").unwrap().1;
    assert!(
        navigation.lines().any(|line| {
            line.trim_start().starts_with("Recording ") && line.trim_end().ends_with(&expected)
        }),
        "album navigation did not preserve the complete literal local path (terminal={terminal}); expected {expected:?}, actual {navigation:?}"
    );
}

pub fn assert_literal_waiting_hint(terminal: bool) {
    let mut failures = Vec::new();
    for scope in ["recording", "edition"] {
        let fixture = LiteralFolder::new().with_waiting_credits(scope);
        let output = fixture.output("credits", terminal);
        fixture.assert_preserved();
        let waiting = if scope == "recording" {
            "1 waiting"
        } else {
            "Edition: waiting"
        };
        assert!(output.contains(waiting), "{scope}: {output:?}");
        let escaped = fixture
            .folder
            .to_str()
            .unwrap()
            .replace('\u{1b}', "\\u{1b}");
        let hint =
            format!("Fetch waiting recording or edition credits: aede fetch --credits '{escaped}'");
        let expected = if terminal {
            format!("  \u{1b}[2m{hint}\u{1b}[0m")
        } else {
            format!("  {hint}")
        };
        if !output.lines().any(|line| line == expected) {
            failures.push(format!(
                "{scope} waiting hint did not preserve the complete literal folder (terminal={terminal}); expected {expected:?}, actual {output:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
