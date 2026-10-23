//! Actual CLI scans, persisted graph controls and a shared real stdout PTY.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use aede_core::model::{self, Catalog, DUPLICATE, OTHER_EDITION, ReleaseGroup};
use aede_core::{accounts, conclusions, json, parental, sources, store};

use super::personal::PersonalLibrary;

const STYLE_NAME: &str = "related\u{1b}[31mr\u{1b}[0m";
const STYLE_FILE: &str = "f\u{1b}[31mr\u{1b}[0m.flac";

struct Library {
    personal: PersonalLibrary,
    catalog: Catalog,
    files: Vec<(PathBuf, Vec<u8>)>,
    stores: [Vec<u8>; 6],
    machine: [Vec<u8>; 2],
}

fn run(data: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
    command
        .args(args)
        .env("AEDE_HOME", data)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("AEDE_DELEGATED_") {
            command.env_remove(name);
        }
    }
    command.output().unwrap()
}

fn success(data: &Path, args: &[&str]) -> Vec<u8> {
    let result = run(data, args);
    assert!(result.status.success(), "{args:?}: {result:?}");
    assert!(result.stderr.is_empty(), "{args:?}: {result:?}");
    result.stdout
}

fn snapshots(data: &Path) -> [Vec<u8>; 6] {
    [
        "catalog.json",
        "conclusions.json",
        "sources.json",
        "user.json",
        "accounts.json",
        "parental.json",
    ]
    .map(|name| std::fs::read(data.join(name)).unwrap())
}

fn load(data: &Path) -> Catalog {
    store::load(&store::catalog_path(data)).unwrap().unwrap()
}

impl Library {
    fn scanned(basename: bool, different_encoding: bool) -> Self {
        let personal = PersonalLibrary::new();
        store::save(&Catalog::default(), &store::catalog_path(&personal.dir)).unwrap();
        let music = personal.dir.join("music");
        let clean = music.join("Clean");
        std::fs::create_dir_all(&clean).unwrap();
        let mut copies = vec![(
            "track.flac",
            clean.join(if basename { STYLE_FILE } else { "track.flac" }),
        )];
        if !basename {
            let related = music.join(STYLE_NAME);
            std::fs::create_dir(&related).unwrap();
            let name = if different_encoding {
                "track.mp3"
            } else {
                "track.flac"
            };
            copies.push((name, related.join(name)));
        }
        let files: Vec<_> = copies
            .into_iter()
            .map(|(fixture, destination)| {
                std::fs::copy(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../aede-core/tests/fixtures")
                        .join(fixture),
                    &destination,
                )
                .unwrap();
                let path = destination.canonicalize().unwrap();
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        success(&personal.dir, &["scan", music.to_str().unwrap()]);
        let catalog = load(&personal.dir);
        assert_eq!(catalog.files.len(), files.len());
        assert_eq!(catalog.releases.len(), files.len());
        assert_eq!(catalog.tracks.len(), files.len());
        for release in &catalog.releases {
            assert_eq!(release.title, "Kind of Blue");
            assert_eq!(release.track_ids.len(), 1);
        }
        for (path, bytes) in &files {
            assert!(
                catalog
                    .files
                    .iter()
                    .any(|file| Path::new(&file.path) == path)
            );
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        conclusions::save(
            &Default::default(),
            &conclusions::conclusions_path(&personal.dir),
        )
        .unwrap();
        sources::save(&Default::default(), &sources::sources_path(&personal.dir)).unwrap();
        let identities = accounts::Accounts::bootstrap("operator", "a test password", 1).unwrap();
        accounts::save(&identities, &accounts::accounts_path(&personal.dir)).unwrap();
        parental::ParentalStore::default()
            .save(&parental::path(&personal.dir))
            .unwrap();
        let mut result = Self {
            personal,
            catalog,
            files,
            stores: std::array::from_fn(|_| Vec::new()),
            machine: std::array::from_fn(|_| Vec::new()),
        };
        result.freeze();
        result
    }

    fn freeze(&mut self) {
        self.stores = snapshots(&self.personal.dir);
        self.machine = self.machine_outputs();
        self.assert_machine_paths();
        self.assert_preserved();
    }

    fn machine_outputs(&self) -> [Vec<u8>; 2] {
        ["album", "credits"]
            .map(|command| success(&self.personal.dir, &[command, "Kind of Blue", "--json"]))
    }

    fn assert_machine_paths(&self) {
        let album = json::parse(std::str::from_utf8(&self.machine[0]).unwrap()).unwrap();
        let rows = album.as_arr().unwrap();
        assert_eq!(rows.len(), self.files.len());
        let paths: Vec<_> = rows
            .iter()
            .map(|row| Path::new(row.get("path").unwrap().as_str().unwrap()))
            .collect();
        for (path, _) in &self.files {
            assert_eq!(paths.iter().filter(|saved| **saved == path).count(), 1);
        }
        let credits = json::parse(std::str::from_utf8(&self.machine[1]).unwrap()).unwrap();
        let rows = credits.as_arr().unwrap();
        assert_eq!(rows.len(), self.catalog.releases.len());
        for release in &self.catalog.releases {
            let row = rows
                .iter()
                .find(|row| {
                    Path::new(row.get("folder").unwrap().as_str().unwrap())
                        == Path::new(&release.folder)
                })
                .unwrap();
            let recordings = row.get("recordings").unwrap().as_arr().unwrap();
            assert_eq!(recordings.len(), 1);
            let file = Path::new(recordings[0].get("file").unwrap().as_str().unwrap());
            let track = self.catalog.track(release.track_ids[0]).unwrap();
            assert_eq!(
                file,
                Path::new(&self.catalog.file(track.file_id).unwrap().path)
            );
        }
    }

    fn assert_preserved(&self) {
        assert_eq!(snapshots(&self.personal.dir), self.stores);
        for (path, bytes) in &self.files {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        let saved = load(&self.personal.dir);
        assert_eq!(saved.releases.len(), self.catalog.releases.len());
        assert_eq!(saved.files.len(), self.files.len());
        for release in &self.catalog.releases {
            assert_eq!(
                Path::new(&saved.release(release.id).unwrap().folder),
                Path::new(&release.folder)
            );
        }
        for file in &self.catalog.files {
            assert_eq!(
                Path::new(&saved.file(file.id).unwrap().path),
                Path::new(&file.path)
            );
        }
        assert_eq!(
            self.machine_outputs(),
            self.machine,
            "human rendering changed machine output"
        );
        assert_eq!(snapshots(&self.personal.dir), self.stores);
    }

    fn output(&self, command: &str, terminal: bool) -> String {
        let output = if terminal {
            let result = Command::new("python3")
                .arg("-B")
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/cli_validation_support/terminal.py"),
                )
                .arg(env!("CARGO_BIN_EXE_aede"))
                .arg(&self.personal.dir)
                .arg(command)
                .arg("Kind of Blue")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "real stdout PTY failed: {result:?}"
            );
            let result = json::parse(std::str::from_utf8(&result.stdout).unwrap()).unwrap();
            assert!(result.field_bool("stdout_is_terminal"), "{result:?}");
            assert_eq!(result.field_u64("code"), Some(0), "{result:?}");
            assert_eq!(
                result.field_str("stderr").as_deref(),
                Some(""),
                "{result:?}"
            );
            let output = result.field_str("stdout").unwrap();
            assert!(
                output.contains("\u{1b}[36m"),
                "presentation color missing on real stdout PTY: {output:?}"
            );
            output
        } else {
            let output =
                String::from_utf8(success(&self.personal.dir, &[command, "Kind of Blue"])).unwrap();
            assert!(
                !unsafe_controls(&output),
                "NO_COLOR pipe contains controls: {output:?}"
            );
            output
        };
        self.assert_preserved();
        output
    }

    fn assert_relation(&self, kind: &str) {
        let saved = load(&self.personal.dir);
        assert_eq!(saved.releases.len(), 2);
        let releases = [&saved.releases[0], &saved.releases[1]];
        let tracks = releases.map(|release| saved.track(release.track_ids[0]).unwrap());
        assert_eq!(releases[0].album_artist_id, releases[1].album_artist_id);
        assert_eq!(tracks[0].title, tracks[1].title);
        assert_eq!(tracks[0].track_no, tracks[1].track_no);
        let durations = tracks.map(|track| track.duration_ms.unwrap());
        assert!(durations.iter().all(|duration| *duration > 0));
        assert!(durations[0].abs_diff(durations[1]) <= 3_000);
        let qualities = tracks.map(|track| {
            saved
                .file(track.file_id)
                .unwrap()
                .properties
                .quality_label()
        });
        assert_eq!(qualities[0] == qualities[1], kind == DUPLICATE);
        for (release, other) in [(releases[0], releases[1]), (releases[1], releases[0])] {
            assert_eq!(saved.related_releases(release.id, kind), [other.id]);
            let absent = if kind == DUPLICATE {
                OTHER_EDITION
            } else {
                DUPLICATE
            };
            assert!(saved.related_releases(release.id, absent).is_empty());
        }
    }

    fn related_folder(&self) -> &str {
        &self
            .catalog
            .releases
            .iter()
            .find(|release| Path::new(&release.folder).file_name().unwrap() == STYLE_NAME)
            .unwrap()
            .folder
    }
}

fn unsafe_controls(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
}

fn visible(output: &str) -> String {
    let mut output = output.to_owned();
    for style in ["0", "1", "2", "31", "32", "33", "36"] {
        output = output.replace(&format!("\u{1b}[{style}m"), "");
    }
    assert!(
        output.lines().all(|line| !unsafe_controls(line)),
        "unexpected terminal controls: {output:?}"
    );
    output
}

fn escaped(value: &str) -> String {
    value.replace('\u{1b}', "\\u{1b}")
}

pub fn assert_release_group_folder(terminal: bool) {
    let mut fixture = Library::scanned(false, false);
    fixture.assert_relation(DUPLICATE);
    // The scan supplies both real releases. This explicit valid catalogue group
    // tests loaded-group rendering; it does not claim inferred provider identity.
    let group_id = 0;
    let mbid = "11111111-1111-4111-8111-111111111111";
    let release_ids = fixture
        .catalog
        .releases
        .iter()
        .map(|release| release.id)
        .collect();
    fixture.catalog.release_groups.push(ReleaseGroup {
        id: group_id,
        title: "Kind of Blue".into(),
        key: "kind of blue".into(),
        mbid: mbid.into(),
        release_ids,
    });
    for release in &mut fixture.catalog.releases {
        release.release_group_id = Some(group_id);
        release.release_group_mbid = Some(mbid.into());
    }
    model::rebuild_relations(&mut fixture.catalog);
    store::save(
        &fixture.catalog,
        &store::catalog_path(&fixture.personal.dir),
    )
    .unwrap();
    fixture.catalog = load(&fixture.personal.dir);
    let group = fixture.catalog.release_group(group_id).unwrap();
    assert_eq!(group.release_ids.len(), 2);
    assert_eq!(group.mbid, mbid);
    for release in &fixture.catalog.releases {
        assert_eq!(release.release_group_id, Some(group_id));
        assert_eq!(release.release_group_mbid.as_deref(), Some(mbid));
        assert!(group.release_ids.contains(&release.id));
        assert!(fixture.catalog.relations.iter().any(|relation| {
            relation.kind == model::EDITION_OF
                && relation.source_id == release.id
                && relation.target_id == group_id
        }));
    }
    fixture.freeze();
    let output = fixture.output("album", terminal);
    let rendered = visible(&output);
    assert!(
        rendered.contains("2 local editions"),
        "group branch missing: {output:?}"
    );
    let edition = fixture
        .catalog
        .releases
        .iter()
        .find(|release| release.folder == fixture.related_folder())
        .unwrap();
    let expected = format!(
        "  another edition: {} ({}) · {}",
        edition.title,
        edition.year.unwrap(),
        escaped(&edition.folder)
    );
    assert!(
        rendered.lines().any(|line| line == expected),
        "release-group edition folder is not complete literal text (terminal={terminal}); expected {expected:?}, actual {output:?}"
    );
}

pub fn assert_related_folder(terminal: bool, different_encoding: bool) {
    let fixture = Library::scanned(false, different_encoding);
    // Both relation kinds are inferred naturally from scanned, untouched audio:
    // two FLAC copies, or a FLAC and the genuine MP3 fixture of the same track.
    fixture.assert_relation(if different_encoding {
        OTHER_EDITION
    } else {
        DUPLICATE
    });
    let wording = if different_encoding {
        "also present, encoded differently:"
    } else {
        "also present, same quality:"
    };
    let output = fixture.output("album", terminal);
    let rendered = visible(&output);
    assert_eq!(
        rendered
            .lines()
            .filter(|line| line.trim_start().starts_with(wording))
            .count(),
        2,
        "related-release branch missing: {output:?}"
    );
    let expected = format!("  {wording} {}", escaped(fixture.related_folder()));
    assert!(
        rendered.lines().any(|line| line == expected),
        "related folder is not complete literal text (kind={}, terminal={terminal}); expected {expected:?}, actual {output:?}",
        if different_encoding {
            OTHER_EDITION
        } else {
            DUPLICATE
        }
    );
}

pub fn assert_credit_basename(terminal: bool) {
    let fixture = Library::scanned(true, false);
    let expected = escaped(STYLE_FILE);
    assert!(
        expected.chars().count() <= 36,
        "the complete basename must fit the File column"
    );
    let output = fixture.output("credits", terminal);
    let rendered = visible(&output);
    assert!(
        rendered
            .lines()
            .any(|line| line.contains("Recording") && line.trim_end().ends_with("File")),
        "credits File table missing: {output:?}"
    );
    assert!(
        rendered.lines().any(|line| line
            .trim_start()
            .strip_prefix("So What")
            .is_some_and(|columns| columns.trim_start().starts_with("unidentified"))
            && line.ends_with(&expected)),
        "credits File basename is not complete literal text (terminal={terminal}); expected {expected:?}, actual {output:?}"
    );
}
