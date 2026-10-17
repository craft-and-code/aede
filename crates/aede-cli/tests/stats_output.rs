//! Statistics page their subsidiary lists and include every persistent store.

use aede_core::model::{self, ScannedFile};
use aede_core::{conclusions, json, store, tags, text};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede-stats-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let files = ["Alpha", "Beta", "Gamma"]
            .into_iter()
            .enumerate()
            .map(|(i, artist)| {
                let mut tags = tags::RawTags::default();
                tags.insert("artist", artist);
                tags.insert("albumartist", artist);
                tags.insert("album", artist);
                tags.insert("title", artist);
                tags.insert("producer", artist);
                if i == 0 {
                    tags.insert("composer", artist);
                }
                tags.properties.codec = ["flac", "mp3", "wav"][i].into();
                ScannedFile {
                    path: format!("/music/{artist}/track.flac"),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect();
        let catalog = model::build(files, vec!["/music".into()], 1, &[]);
        store::save(&catalog, &store::catalog_path(&dir)).unwrap();
        Self(dir)
    }

    fn run(&self, extra: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .arg("stats")
            .arg("--data")
            .arg(&self.0)
            .args(extra)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn pagination_applies_to_human_and_json_lists_without_changing_totals() {
    let library = Library::new();
    let out = library.run(&["--limit=1", "--offset=1", "--json"]);
    let report = json::parse(&out).unwrap();
    assert_eq!(report.get("tracks").and_then(|v| v.as_u64()), Some(3));
    let artists = report
        .get("top")
        .unwrap()
        .get("artists")
        .unwrap()
        .as_arr()
        .unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(
        artists[0].get("name").and_then(|v| v.as_str()),
        Some("Beta")
    );
    let formats = report.get("by_codec").unwrap().as_arr().unwrap();
    assert_eq!(formats.len(), 1);
    assert_eq!(
        formats[0].get("label").and_then(|v| v.as_str()),
        Some("MP3")
    );
    let roles = report.get("roles").unwrap().as_arr().unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(
        roles[0].get("role").and_then(|v| v.as_str()),
        Some("composer")
    );

    let human = library.run(&["--limit=1", "--offset=1"]);
    let ranking = human.split("Most present performers").nth(1).unwrap();
    assert!(ranking.contains("Beta"), "{human}");
    assert!(
        !ranking.contains("Alpha") && !ranking.contains("Gamma"),
        "{human}"
    );

    let all = json::parse(&library.run(&["--all", "--json"])).unwrap();
    assert_eq!(
        all.get("top")
            .unwrap()
            .get("artists")
            .unwrap()
            .as_arr()
            .unwrap()
            .len(),
        3
    );
    let beyond = json::parse(&library.run(&["--offset=100", "--json"])).unwrap();
    assert!(
        beyond
            .get("top")
            .unwrap()
            .get("artists")
            .unwrap()
            .as_arr()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn data_weight_includes_the_conclusions_store() {
    let library = Library::new();
    let mut saved = conclusions::to_json(&conclusions::Conclusions::default());
    // Unknown optional fields are valid; make this file visibly outweigh the catalog.
    saved.set("future_evidence", "x".repeat(20_000).into());
    let path = conclusions::conclusions_path(&library.0);
    std::fs::write(&path, saved.to_string_pretty()).unwrap();
    let bytes = std::fs::metadata(store::catalog_path(&library.0))
        .unwrap()
        .len()
        + std::fs::metadata(path).unwrap().len();
    let output = library.run(&[]);
    assert!(output.contains(&text::format_size(bytes)), "{output}");
}

#[test]
fn album_summaries_saturate_extreme_measures_and_preserve_ordinary_totals() {
    let library = Library::new();
    let files = [
        ("Extreme", 1u64 << 63, 1u64 << 63),
        ("Extreme", 1u64 << 63, 1u64 << 63),
        ("Ordinary", 500, 11),
        ("Ordinary", 700, 13),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (album, duration, size))| {
        let mut tags = tags::RawTags::default();
        tags.insert("artist", "Artist");
        tags.insert("albumartist", "Artist");
        tags.insert("album", album);
        tags.insert("title", format!("Track {index}"));
        tags.properties.duration_ms = Some(duration);
        ScannedFile {
            path: format!("/music/{album}/{index}.flac"),
            size,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    })
    .collect();
    let catalog = model::build(files, vec!["/music".into()], 1, &[]);
    store::save(&catalog, &store::catalog_path(&library.0)).unwrap();
    let paths = [
        store::catalog_path(&library.0),
        conclusions::conclusions_path(&library.0),
        aede_core::user::user_path(&library.0),
        aede_core::sources::sources_path(&library.0),
    ];
    let snapshot = || {
        paths
            .iter()
            .map(|path| path.exists().then(|| std::fs::read(path).unwrap()))
            .collect::<Vec<_>>()
    };
    let before = snapshot();
    for (album, duration, size) in [("Extreme", u64::MAX, u64::MAX), ("Ordinary", 1200, 24)] {
        let result = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(["album", album, "--data"])
            .arg(&library.0)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        let expected = format!(
            "2 tracks · {} · {}",
            text::format_duration(duration),
            text::format_size(size)
        );
        assert!(output.contains(&expected), "{output}");
        assert_eq!(snapshot(), before, "album read changed persistent stores");
    }
}

#[test]
fn labels_sort_track_and_album_counts_independently_with_stable_pages() {
    let library = Library::new();
    let mut files = Vec::new();
    for (album, label, count) in [
        ("Alpha Many", "Alpha Label", 3),
        ("Beta First", "Beta Label", 1),
        ("Beta Second", "Beta Label", 1),
        ("Gamma Pair", "Gamma Label", 2),
    ] {
        for index in 0..count {
            let mut tags = tags::RawTags::default();
            tags.insert("artist", "Artist");
            tags.insert("albumartist", "Artist");
            tags.insert("album", album);
            tags.insert("label", label);
            tags.insert("title", format!("Track {index}"));
            files.push(ScannedFile {
                path: format!("/music/{album}/{index}.flac"),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            });
        }
    }
    let catalog = model::build(files, vec!["/music".into()], 1, &[]);
    store::save(&catalog, &store::catalog_path(&library.0)).unwrap();
    let paths = [
        store::catalog_path(&library.0),
        conclusions::conclusions_path(&library.0),
        aede_core::user::user_path(&library.0),
        aede_core::sources::sources_path(&library.0),
    ];
    let snapshot = || {
        paths
            .iter()
            .map(|path| path.exists().then(|| std::fs::read(path).unwrap()))
            .collect::<Vec<_>>()
    };
    let before = snapshot();
    let run = |options: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(["labels", "--data"])
            .arg(&library.0)
            .args(options)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert_eq!(
            snapshot(),
            before,
            "label listing changed persistent stores"
        );
        String::from_utf8(result.stdout).unwrap()
    };
    for (sort, expected) in [
        ("tracks", ["Beta Label", "Gamma Label", "Alpha Label"]),
        ("tracks-", ["Alpha Label", "Beta Label", "Gamma Label"]),
        ("albums", ["Alpha Label", "Gamma Label", "Beta Label"]),
        ("albums-", ["Beta Label", "Alpha Label", "Gamma Label"]),
    ] {
        let output = run(&["--json", &format!("--sort={sort}")]);
        let report = json::parse(&output).unwrap();
        let rows = report.as_arr().unwrap();
        let names: Vec<_> = rows
            .iter()
            .map(|row| row.get("label").and_then(|label| label.as_str()).unwrap())
            .collect();
        assert_eq!(names, expected, "--sort={sort}");
        for row in rows {
            let label = row.get("label").and_then(|label| label.as_str()).unwrap();
            let (tracks, albums) = match label {
                "Alpha Label" => (3, 1),
                "Beta Label" => (2, 2),
                "Gamma Label" => (2, 1),
                _ => panic!("unexpected label {label}"),
            };
            assert_eq!(
                row.get("tracks").and_then(|value| value.as_u64()),
                Some(tracks)
            );
            assert_eq!(
                row.get("albums").and_then(|value| value.as_u64()),
                Some(albums)
            );
        }
    }
    let page = run(&["--sort=tracks", "--offset=1", "--limit=1", "--json"]);
    let report = json::parse(&page).unwrap();
    let rows = report.as_arr().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("label").and_then(|label| label.as_str()),
        Some("Gamma Label")
    );
    let human = run(&["--sort=tracks", "--offset=1", "--limit=1"]);
    assert!(human.contains("Gamma Label"), "{human}");
    assert!(
        !human.contains("Alpha Label") && !human.contains("Beta Label"),
        "{human}"
    );
}

#[test]
fn sourced_country_exports_keep_sorted_windows_and_numeric_totals() {
    use aede_core::model::EntityKind;
    use aede_core::sources::{ArtistFacts, Confidence, Facts, SourceRecord, Sources};
    use aede_core::user::EntityRef;

    let library = Library::new();
    let catalog_path = store::catalog_path(&library.0);
    let mut catalog = store::load(&catalog_path).unwrap().unwrap();
    for (file, size) in catalog.files.iter_mut().zip([20, 30, 10]) {
        file.size = size;
    }
    for (track, duration) in catalog.tracks.iter_mut().zip([1000, 2000, 10000]) {
        track.duration_ms = Some(duration);
    }
    store::save(&catalog, &catalog_path).unwrap();
    let mut held = Sources::default();
    for artist in &catalog.artists {
        let (area, code) = if artist.name == "Gamma" {
            ("United Kingdom", "GB")
        } else {
            ("France", "FR")
        };
        held.set(SourceRecord {
            key: EntityRef::of(&catalog, EntityKind::Artist, artist.id)
                .unwrap()
                .key,
            source: aede_core::sources::MUSICBRAINZ.into(),
            source_id: None,
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                area: Some(area.into()),
                country_code: Some(code.into()),
                ..Default::default()
            }),
        });
    }
    aede_core::sources::save(&held, &aede_core::sources::sources_path(&library.0)).unwrap();
    let snapshots = || {
        [
            "catalog.json",
            "conclusions.json",
            "sources.json",
            "user.json",
        ]
        .map(|name| std::fs::read(library.0.join(name)).ok())
    };
    let before = snapshots();
    let run = |extra: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(["countries", "--data"])
            .arg(&library.0)
            .args(extra)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert_eq!(snapshots(), before, "changed stores for {extra:?}");
        output
    };
    let text = |extra: &[&str]| {
        let output = run(extra);
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let names = |report: &json::Json| {
        report
            .as_arr()
            .unwrap()
            .iter()
            .map(|row| row.field_str("country").unwrap())
            .collect::<Vec<_>>()
    };
    let page = json::parse(&text(&["--json", "--sort=name", "--offset=1", "--limit=1"])).unwrap();
    assert_eq!(names(&page), ["United Kingdom"]);
    let row = &page.as_arr().unwrap()[0];
    for (key, value) in [
        ("artists", 1),
        ("tracks", 1),
        ("duration_ms", 10000),
        ("size_bytes", 10),
    ] {
        assert_eq!(row.field_u64(key), Some(value));
    }
    assert_eq!(row.field_str("iso_code").as_deref(), Some("gb"));
    assert_eq!(row.field_str("initials").as_deref(), Some("uk"));
    let header = "country,iso_code,initials,artists,tracks,duration_ms,size_bytes\r\n";
    assert_eq!(
        text(&["--csv", "--sort=name", "--offset=1", "--limit=1"]),
        format!("{header}United Kingdom,gb,uk,1,1,10000,10\r\n")
    );
    let human = text(&["--sort=name", "--offset=1", "--limit=1"]);
    assert!(human.contains("United Kingdom"));
    assert!(!human.contains("France"));
    for (sort, expected) in [
        ("name", ["France", "United Kingdom"]),
        ("name-", ["United Kingdom", "France"]),
        ("albums", ["France", "United Kingdom"]),
        ("albums-", ["United Kingdom", "France"]),
        ("tracks", ["France", "United Kingdom"]),
        ("tracks-", ["United Kingdom", "France"]),
        ("duration", ["United Kingdom", "France"]),
        ("duration-", ["France", "United Kingdom"]),
        ("size", ["France", "United Kingdom"]),
        ("size-", ["United Kingdom", "France"]),
    ] {
        let option = format!("--sort={sort}");
        let all = json::parse(&text(&["--json", &option, "--all"])).unwrap();
        assert_eq!(names(&all), expected, "{sort}");
        let tail = json::parse(&text(&["--json", &option, "--all", "--offset=1"])).unwrap();
        assert_eq!(names(&tail), expected[1..], "{sort}");
    }
    let empty = json::parse(&text(&["--json", "--offset=99"])).unwrap();
    assert!(empty.as_arr().unwrap().is_empty());
    assert_eq!(text(&["--csv", "--offset=99"]), header);
    for format in ["--json", "--csv"] {
        let path = library.0.join(format!("country-page-{format}.txt"));
        let notice = text(&[
            format,
            "--sort=name",
            "--offset=1",
            "--limit=1",
            "--output",
            path.to_str().unwrap(),
        ]);
        assert!(notice.contains("written to"), "{notice}");
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("United Kingdom") && !saved.contains("France"));
        for window in ["--limit=0", "--offset=-1"] {
            let output = run(&[format, window, "--output", path.to_str().unwrap()]);
            assert!(!output.status.success());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
        }
    }
}

#[test]
fn genre_and_label_exports_honour_and_validate_track_windows_before_output() {
    let library = Library::new();
    let files = ["Alpha", "Beta", "Gamma"]
        .into_iter()
        .map(|name| {
            let mut tags = tags::RawTags::default();
            for field in ["artist", "albumartist", "album", "title"] {
                tags.insert(field, name);
            }
            tags.insert("genre", "Facet Genre");
            tags.insert("label", "Facet Label");
            ScannedFile {
                path: format!("/music/{name}/track.flac"),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = model::build(files, vec!["/music".into()], 1, &[]);
    store::save(&catalog, &store::catalog_path(&library.0)).unwrap();
    let paths = [
        store::catalog_path(&library.0),
        conclusions::conclusions_path(&library.0),
        aede_core::user::user_path(&library.0),
        aede_core::sources::sources_path(&library.0),
    ];
    let snapshot = || {
        paths
            .iter()
            .map(|path| path.exists().then(|| std::fs::read(path).unwrap()))
            .collect::<Vec<_>>()
    };
    let before = snapshot();
    let output = library.0.join("existing-report");
    std::fs::write(&output, "preserve this report").unwrap();
    for command in [
        vec!["genre", "Facet Genre"],
        vec!["label", "Facet Label", "--offline"],
    ] {
        let run = |options: &[&str]| {
            let result = Command::new(env!("CARGO_BIN_EXE_aede"))
                .args(&command)
                .arg("--data")
                .arg(&library.0)
                .args(options)
                .env("NO_COLOR", "1")
                .output()
                .unwrap();
            assert_eq!(
                snapshot(),
                before,
                "facet listing changed persistent stores"
            );
            result
        };
        for format in ["--json", "--csv", "--m3u"] {
            let result = run(&[format, "--limit=1", "--offset=1"]);
            assert!(result.status.success(), "{command:?}: {result:?}");
            let text = String::from_utf8(result.stdout).unwrap();
            match format {
                "--json" => {
                    let report = json::parse(&text).unwrap();
                    let rows = report.as_arr().unwrap();
                    assert_eq!(rows.len(), 1, "{command:?} ignored the track window");
                    assert_eq!(
                        rows[0].get("title").and_then(|value| value.as_str()),
                        Some("Beta")
                    );
                }
                "--csv" => {
                    assert_eq!(text.lines().count(), 2, "{text}");
                    assert!(text.contains("/music/Beta/track.flac"), "{text}");
                }
                "--m3u" => {
                    let entries: Vec<_> =
                        text.lines().filter(|line| !line.starts_with('#')).collect();
                    assert_eq!(entries, ["/music/Beta/track.flac"]);
                }
                _ => unreachable!(),
            }
            for invalid in ["--limit=0", "--offset=invalid"] {
                let result = run(&[format, invalid, "--output", output.to_str().unwrap()]);
                assert!(!result.status.success(), "{command:?} accepted {invalid}");
                assert!(result.stdout.is_empty(), "{result:?}");
                assert_eq!(
                    std::fs::read_to_string(&output).unwrap(),
                    "preserve this report"
                );
            }
        }
        let result = run(&["--all", "--offset=1", "--json"]);
        assert!(result.status.success(), "{result:?}");
        let text = String::from_utf8(result.stdout).unwrap();
        let report = json::parse(&text).unwrap();
        let titles: Vec<_> = report
            .as_arr()
            .unwrap()
            .iter()
            .map(|row| row.get("title").and_then(|value| value.as_str()).unwrap())
            .collect();
        assert_eq!(titles, ["Beta", "Gamma"]);
        let result = run(&["--offset=99", "--json"]);
        assert!(result.status.success(), "{result:?}");
        let report = json::parse(&String::from_utf8(result.stdout).unwrap()).unwrap();
        assert!(report.as_arr().unwrap().is_empty());
        let result = run(&["--offset=99", "--m3u"]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("nothing to put"));
        let result = run(&["--all", "--limit=1", "--json"]);
        assert!(
            !result.status.success(),
            "{command:?} accepted conflicting window options"
        );
    }
}
