use aede_core::{json, model, sources, store, tags::RawTags};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const WORK: &str = "11111111-1111-1111-1111-111111111111";

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        static NEXT_LIBRARY: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Clock precision can give concurrent tests the same timestamp.
        let sequence = NEXT_LIBRARY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aede_manual_work_{}_{nonce}_{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("data")).unwrap();
        Self(root)
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn snapshot(&self) -> [Option<Vec<u8>>; 4] {
        [
            "catalog.json",
            "sources.json",
            "user.json",
            "conclusions.json",
        ]
        .map(|name| std::fs::read(self.data().join(name)).ok())
    }

    fn run(&self, args: &[&str], writes: bool) -> Output {
        let before = self.snapshot();
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .arg("--data")
            .arg(self.data())
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        if !writes {
            assert_eq!(self.snapshot(), before, "{args:?} changed stores");
        }
        output
    }

    fn text(&self, args: &[&str], writes: bool) -> String {
        let output = self.run(args, writes);
        assert!(output.status.success(), "{args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    fn titles(&self, expression: &str) -> Vec<String> {
        json::parse(&self.text(&["query", expression, "--json", "--all"], false))
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .map(|row| row.field_str("title").unwrap())
            .collect()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn manual_work_credits_reach_exact_realizations_and_keep_one_attributed_claim() {
    let library = Library::new();
    let rows = [
        ("Album A", "First Song", "recording-one", Some(WORK)),
        ("Album B", "Second Song", "recording-two", Some(WORK)),
        ("Album C", "First Song", "recording-one", Some(WORK)),
        (
            "Album D",
            "Different Work Song",
            "recording-three",
            Some("22222222-2222-2222-2222-222222222222"),
        ),
        ("Album E", "Sourced Work Song", "recording-four", None),
        ("Album F", "Pending Work Song", "recording-five", None),
    ];
    let files = rows
        .iter()
        .enumerate()
        .map(|(index, (album, title, recording, work))| {
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Local Player"),
                ("albumartist", "Local Player"),
                ("album", *album),
                ("title", *title),
                ("musicbrainz_recordingid", *recording),
            ] {
                tags.insert(key, value);
            }
            tags.insert("musicbrainz_albumid", format!("edition-{index}"));
            if let Some(work) = work {
                tags.insert("musicbrainz_workid", *work);
                tags.insert("work", "Shared Work");
            }
            let path = library.0.join("music").join(album).join("01.flac");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                include_bytes!("../../aede-core/tests/fixtures/track.flac"),
            )
            .unwrap();
            model::ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: 20132,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = model::build(
        files,
        vec![library.0.join("music").to_string_lossy().into_owned()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&library.data())).unwrap();
    let catalog_before = std::fs::read(store::catalog_path(&library.data())).unwrap();
    let host = catalog
        .recordings
        .iter()
        .find(|row| row.mbid.as_deref() == Some("recording-one"))
        .unwrap()
        .id;
    let mut membership = sources::Sources::default();
    for (album, recording, confidence) in [
        ("Album E", "recording-four", sources::Confidence::Identified),
        (
            "Album F",
            "recording-five",
            sources::Confidence::matched(42),
        ),
    ] {
        membership.set(sources::SourceRecord {
            key: library
                .0
                .join("music")
                .join(album)
                .join("01.flac")
                .to_string_lossy()
                .into_owned(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(recording.into()),
            fetched_at: 1,
            confidence,
            facts: sources::Facts::Track(sources::TrackFacts {
                works: vec![sources::WorkLink {
                    mbid: WORK.into(),
                    title: "Remote Work Spelling".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        });
    }
    let document = library.0.join("membership.json");
    std::fs::write(&document, sources::to_json(&membership).to_string_pretty()).unwrap();
    library.text(&["sources", "--import", document.to_str().unwrap()], true);
    let scope = format!("work:{WORK}");
    let notice = library.text(
        &[
            "credit",
            "--add",
            &scope,
            "--artist=Manual Writer",
            "--artist-id=writer-id",
            "--role=composer",
        ],
        true,
    );
    let credit_id = notice.split_whitespace().last().unwrap().to_string();
    let expected = vec![
        "First Song",
        "Second Song",
        "First Song",
        "Sourced Work Song",
    ];
    assert_eq!(library.titles("composer:writer-id"), expected);
    assert_eq!(library.titles(&format!("work:{WORK}")), expected);

    for args in [vec!["recording", "recording-two"], vec!["album", "Album B"]] {
        assert!(library.text(&args, false).contains("Manual Writer"));
    }
    let detail = json::parse(&library.text(&["track", "Second Song", "--json"], false)).unwrap();
    let credit = detail.as_arr().unwrap()[0]
        .get("sourced_credits")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .find(|row| row.field_str("artist_mbid").as_deref() == Some("writer-id"))
        .unwrap();
    assert_eq!(credit.field_str("scope").as_deref(), Some("work"));
    assert_eq!(
        credit.field_u32("recording_id"),
        Some(host),
        "the evidence host is retained"
    );
    assert!(
        library
            .text(&["artist", "writer-id"], false)
            .contains("3 credited local recordings")
    );
    let selected =
        json::parse(&library.text(&["artist", "writer-id", "--json", "--all"], false)).unwrap();
    assert_eq!(selected.as_arr().unwrap().len(), 4);

    // Recording and edition corrections retain their narrower scopes.
    library.text(
        &[
            "credit",
            "--add",
            "recording:recording-one",
            "--artist=Manual Producer",
            "--artist-id=producer-id",
            "--role=producer",
        ],
        true,
    );
    library.text(
        &[
            "credit",
            "--add",
            "release:edition-1",
            "--artist=Manual Engineer",
            "--artist-id=engineer-id",
            "--role=engineer",
        ],
        true,
    );
    assert_eq!(
        library.titles("producer:producer-id"),
        ["First Song", "First Song"]
    );
    assert_eq!(library.titles("engineer:engineer-id"), ["Second Song"]);
    let before_duplicate = library.snapshot();
    assert!(
        !library
            .run(
                &[
                    "credit",
                    "--add",
                    &scope,
                    "--artist=Manual Writer",
                    "--artist-id=writer-id",
                    "--role=composer"
                ],
                true
            )
            .status
            .success()
    );
    assert_eq!(library.snapshot(), before_duplicate);
    library.text(
        &[
            "relation",
            &credit_id,
            "--text=Retained work note",
            "--tag=checked",
        ],
        true,
    );

    let raw = sources::load_all(&sources::sources_path(&library.data()))
        .unwrap()
        .unwrap();
    let manual = raw
        .credit_links(&catalog)
        .into_iter()
        .filter(|link| link.source == "manual" && link.work.is_some())
        .collect::<Vec<_>>();
    assert_eq!(manual.len(), 1);
    assert_eq!(manual[0].recording_id, host);
    let graph = json::parse(&library.text(&["export", "--graph"], false)).unwrap();
    let edge = graph
        .get("relations")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .find(|row| row.field_str("id").as_deref() == Some(&credit_id))
        .unwrap();
    assert_eq!(edge.field_u32("weight"), Some(1));
    assert!(edge.to_string_pretty().contains("Retained work note"));

    let pending = raw
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.source_id.as_deref() == Some("recording-five"))
        .unwrap()
        .id;
    library.text(&["review", "--accept", &pending], true);
    assert_eq!(library.titles("composer:writer-id").len(), 5);
    library.text(&["review", "--reject", &pending], true);
    assert_eq!(library.titles("composer:writer-id"), expected);
    library.text(&["credit", "--exclude", &credit_id], true);
    assert!(
        !library
            .run(&["query", "composer:writer-id", "--json"], false)
            .status
            .success()
    );
    let excluded = library.text(&["relation", &credit_id], false);
    assert!(excluded.contains("excluded by you") && excluded.contains("Retained work note"));
    let rules_path = library.0.join("rules.json");
    library.text(
        &[
            "rules",
            "--export",
            "--output",
            rules_path.to_str().unwrap(),
        ],
        false,
    );
    let sources_path = library.0.join("sources-export.json");
    library.text(
        &[
            "sources",
            "--export",
            "--output",
            sources_path.to_str().unwrap(),
        ],
        false,
    );
    library.text(&["credit", "--undo", &credit_id], true);
    assert_eq!(library.titles("composer:writer-id"), expected);
    for (command, path) in [
        ("sources", sources_path.as_path()),
        ("rules", rules_path.as_path()),
    ] {
        library.text(&[command, "--import", path.to_str().unwrap()], true);
        assert!(
            !library
                .run(&["query", "composer:writer-id", "--json"], false)
                .status
                .success()
        );
        library.text(&["credit", "--undo", &credit_id], true);
        assert_eq!(library.titles("composer:writer-id"), expected);
    }
    let final_graph = json::parse(&library.text(&["export", "--graph"], false)).unwrap();
    let final_edge = final_graph
        .get("relations")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .find(|row| row.field_str("id").as_deref() == Some(&credit_id))
        .unwrap();
    assert_eq!(final_edge.field_u32("weight"), Some(1));
    assert!(final_edge.to_string_pretty().contains("Retained work note"));
    assert_eq!(
        std::fs::read(store::catalog_path(&library.data())).unwrap(),
        catalog_before
    );
    for (album, _, _, _) in rows {
        assert_eq!(
            std::fs::read(library.0.join("music").join(album).join("01.flac")).unwrap(),
            include_bytes!("../../aede-core/tests/fixtures/track.flac")
        );
    }
    assert!(!library.data().join("conclusions.json").exists());
}

#[test]
fn local_artist_role_unites_exact_trusted_scopes_and_preserves_tag_credits() {
    const WRITER: &str = "33333333-3333-4333-8333-333333333333";
    const NAMESAKE: &str = "44444444-4444-4444-8444-444444444444";
    let library = Library::new();
    // More than fifty composition performances exercise the existing default window.
    let mut rows: Vec<_> = (0..52)
        .map(|index| {
            (
                format!("Work Album {index:02}"),
                format!("Work Song {index:02}"),
            )
        })
        .collect();
    rows.extend([
        ("Direct Album".into(), "Direct Song".into()),
        ("Edition Album".into(), "Edition Song".into()),
        ("Writer Album".into(), "Writer Performance".into()),
        ("Other Album".into(), "Other Song".into()),
    ]);
    let files = rows
        .iter()
        .enumerate()
        .map(|(index, (album, title))| {
            let mut tags = RawTags::default();
            let artist = if index == 54 {
                "Manual Writer"
            } else {
                "Local Player"
            };
            for (key, value) in [
                ("artist", artist),
                ("albumartist", artist),
                ("album", album.as_str()),
                ("title", title.as_str()),
            ] {
                tags.insert(key, value);
            }
            tags.insert("musicbrainz_recordingid", format!("recording-{index}"));
            tags.insert("musicbrainz_albumid", format!("edition-{index}"));
            if index < 52 {
                tags.insert("musicbrainz_workid", WORK);
                tags.insert("work", "Shared Work");
            }
            if index == 0 {
                tags.insert("composer", "Manual Writer");
            }
            if index == 54 {
                tags.insert("musicbrainz_artistid", WRITER);
                tags.insert("musicbrainz_albumartistid", WRITER);
            }
            let path = library.0.join("music").join(album).join("01.flac");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                include_bytes!("../../aede-core/tests/fixtures/track.flac"),
            )
            .unwrap();
            model::ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: 20132,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = model::build(
        files,
        vec![library.0.join("music").to_string_lossy().into_owned()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&library.data())).unwrap();
    let catalog_before = std::fs::read(store::catalog_path(&library.data())).unwrap();
    let local_writer = catalog
        .artists
        .iter()
        .find(|artist| artist.mbid.as_deref() == Some(WRITER))
        .unwrap();
    assert_eq!(
        catalog
            .tracks_of_artist_in_role(local_writer.id, "composer")
            .len(),
        1
    );
    let add = |scope: &str, artist_id: &str, role: &str| {
        library
            .text(
                &[
                    "credit",
                    "--add",
                    scope,
                    "--artist=Manual Writer",
                    &format!("--artist-id={artist_id}"),
                    &format!("--role={role}"),
                ],
                true,
            )
            .split_whitespace()
            .last()
            .unwrap()
            .to_string()
    };
    let work_credit = add(&format!("work:{WORK}"), WRITER, "composer");
    let selected = |role: &str, options: &[&str]| -> Vec<String> {
        let mut args = vec!["artist", WRITER, "--role", role, "--json"];
        args.extend(options);
        json::parse(&library.text(&args, false))
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .map(|row| row.field_str("title").unwrap())
            .collect()
    };
    let work_titles: Vec<_> = (0..52)
        .map(|index| format!("Work Song {index:02}"))
        .collect();
    assert_eq!(
        selected("composer", &["--all"]),
        work_titles,
        "the local role page includes the exact-MBID sourced work credit once per placement"
    );
    assert_eq!(selected("main artist", &["--all"]), ["Writer Performance"]);
    assert_eq!(selected("composer", &[]).len(), 50);
    assert_eq!(
        selected("COMPOSER", &["--limit=2", "--offset=1"]),
        work_titles[1..3]
    );
    let direct_credit = add("recording:recording-52", WRITER, "composer");
    let edition_credit = add("release:edition-53", WRITER, "composer");
    add("recording:recording-55", NAMESAKE, "composer");
    add("recording:recording-52", WRITER, "art director");
    assert_eq!(selected("ART DIRECTOR", &["--all"]), ["Direct Song"]);
    let mut expected = vec!["Direct Song".to_string(), "Edition Song".to_string()];
    expected.extend(work_titles.clone());
    assert_eq!(selected("composer", &["--all"]), expected);
    assert_eq!(
        selected("composer", &["--all", "--offset=1"]),
        expected[1..]
    );
    let original =
        json::parse(&library.text(&["artist", WRITER, "--json", "--all"], false)).unwrap();
    assert_eq!(
        original.as_arr().unwrap().len(),
        1,
        "the ordinary artist selection stays its performed track"
    );

    let mut held = sources::load_all(&sources::sources_path(&library.data()))
        .unwrap()
        .unwrap();
    held.set(sources::SourceRecord {
        key: library
            .0
            .join("music/Other Album/01.flac")
            .to_string_lossy()
            .into_owned(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("recording-55".into()),
        fetched_at: 1,
        confidence: sources::Confidence::matched(42),
        facts: sources::Facts::Track(sources::TrackFacts {
            credits: vec![sources::CreditLink {
                artist_mbid: WRITER.into(),
                artist_name: "Manual Writer".into(),
                role: "composer".into(),
                relation_id: None,
                role_id: None,
                direction: None,
                credited_as: None,
                attributes: vec![],
                began: None,
                ended: None,
                over: None,
                order: None,
            }],
            ..Default::default()
        }),
    });
    let pending = held
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.source == sources::MUSICBRAINZ)
        .unwrap()
        .id;
    let pending_path = library.0.join("pending.json");
    std::fs::write(&pending_path, sources::to_json(&held).to_string_pretty()).unwrap();
    library.text(
        &["sources", "--import", pending_path.to_str().unwrap()],
        true,
    );
    assert_eq!(
        selected("composer", &["--all"]),
        expected,
        "a pending exact recording answer contributes no credit"
    );
    library.text(&["review", "--accept", &pending], true);
    let accepted = selected("composer", &["--all"]);
    assert_eq!(accepted.len(), expected.len() + 1);
    assert!(accepted.iter().any(|title| title == "Other Song"));
    library.text(&["review", "--reject", &pending], true);
    assert_eq!(selected("composer", &["--all"]), expected);
    library.text(&["credit", "--exclude", &work_credit], true);
    assert_eq!(
        selected("composer", &["--all"]),
        ["Direct Song", "Edition Song", "Work Song 00"]
    );
    library.text(&["credit", "--exclude", &direct_credit], true);
    library.text(&["credit", "--exclude", &edition_credit], true);
    assert_eq!(
        selected("composer", &["--all"]),
        ["Work Song 00"],
        "an exclusion cannot remove the independent tag credit"
    );
    for credit in [&work_credit, &direct_credit, &edition_credit] {
        library.text(&["credit", "--undo", credit], true);
    }
    assert_eq!(selected("composer", &["--all"]), expected);
    for args in [
        vec!["artist", WRITER, "--role=composer", "--members"],
        vec!["artist", WRITER, "--role=composer", "--with=Local Player"],
        vec!["artist", WRITER, "--role=not-a-role"],
    ] {
        assert!(!library.run(&args, false).status.success());
    }
    assert_eq!(
        std::fs::read(store::catalog_path(&library.data())).unwrap(),
        catalog_before
    );
    for (album, _) in rows {
        assert_eq!(
            std::fs::read(library.0.join("music").join(album).join("01.flac")).unwrap(),
            include_bytes!("../../aede-core/tests/fixtures/track.flac")
        );
    }
    assert!(!library.data().join("conclusions.json").exists());
}

#[test]
fn parent_work_navigation_reaches_exact_child_performances_without_new_observations() {
    const CHILD: &str = "11111111-1111-4111-8111-111111111111";
    const PARENT: &str = "22222222-2222-4222-8222-222222222222";
    const OTHER: &str = "33333333-3333-4333-8333-333333333333";
    let library = Library::new();
    let rows = [
        ("First Album", "First Performance", "recording-one", CHILD),
        ("Second Album", "Second Performance", "recording-two", CHILD),
        ("Other Album", "Other Performance", "recording-three", OTHER),
    ];
    let files = rows
        .iter()
        .map(|(album, title, recording, work)| {
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Local Orchestra"),
                ("albumartist", "Local Orchestra"),
                ("album", *album),
                ("title", *title),
                ("musicbrainz_recordingid", *recording),
                ("musicbrainz_workid", *work),
                ("work", "One Movement"),
            ] {
                tags.insert(key, value);
            }
            let path = library.0.join("music").join(album).join("01.flac");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                include_bytes!("../../aede-core/tests/fixtures/track.flac"),
            )
            .unwrap();
            model::ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: 20132,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = model::build(
        files,
        vec![library.0.join("music").to_string_lossy().into_owned()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&library.data())).unwrap();
    let catalog_before = std::fs::read(store::catalog_path(&library.data())).unwrap();
    let host = catalog
        .recordings
        .iter()
        .find(|recording| recording.mbid.as_deref() == Some("recording-one"))
        .unwrap()
        .id;
    let mut held = sources::Sources::default();
    held.set(sources::SourceRecord {
        key: library
            .0
            .join("music/First Album/01.flac")
            .to_string_lossy()
            .into_owned(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("recording-one".into()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: sources::Facts::Track(sources::TrackFacts {
            works: vec![sources::WorkLink {
                mbid: CHILD.into(),
                title: "One Movement".into(),
                parents: vec![sources::WorkParentLink {
                    mbid: PARENT.into(),
                    title: "Whole Symphony".into(),
                    order: Some(1),
                    relation_id: Some("movement-parent".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let document = library.0.join("parent-membership.json");
    std::fs::write(&document, sources::to_json(&held).to_string_pretty()).unwrap();
    library.text(&["sources", "--import", document.to_str().unwrap()], true);
    let source_before = std::fs::read(sources::sources_path(&library.data())).unwrap();
    let graph_before = library.text(&["export", "--graph"], false);
    let parent_titles = library.titles(&format!("work:{PARENT}"));
    let second =
        json::parse(&library.text(&["track", "Second Performance", "--json"], false)).unwrap();
    let second_parents = second.as_arr().unwrap()[0]
        .get("parent_works")
        .unwrap()
        .as_arr()
        .unwrap();
    let second_human = library.text(&["track", "Second Performance"], false);
    let second_album = library.text(&["album", "Second Album"], false);
    let observed = (
        parent_titles,
        second_parents
            .iter()
            .filter_map(|row| row.field_str("parent_mbid"))
            .collect::<Vec<_>>(),
        second_human.contains("Whole Symphony") && second_human.contains(PARENT),
        second_album.contains("Whole Symphony") && second_album.contains(PARENT),
    );

    let other =
        json::parse(&library.text(&["track", "Other Performance", "--json"], false)).unwrap();
    assert!(
        other.as_arr().unwrap()[0]
            .get("parent_works")
            .unwrap()
            .as_arr()
            .unwrap()
            .is_empty(),
        "a matching child title cannot establish the parent relationship"
    );
    assert!(
        !library
            .text(&["album", "Other Album"], false)
            .contains("Whole Symphony")
    );
    let graph = json::parse(&graph_before).unwrap();
    let edges: Vec<_> = graph
        .get("relations")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .filter(|row| row.field_str("relation").as_deref() == Some("part_of_work"))
        .collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].field_u32("weight"), Some(1));
    assert_eq!(
        edges[0].field_str("source").as_deref(),
        Some(format!("work:{CHILD}").as_str())
    );
    assert_eq!(
        edges[0].field_str("target").as_deref(),
        Some(format!("work:{PARENT}").as_str())
    );
    let retained = sources::load_all(&sources::sources_path(&library.data()))
        .unwrap()
        .unwrap();
    let parents = retained.work_parent_links(&catalog);
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].recording_id, host);
    let mut graph_after = json::parse(&library.text(&["export", "--graph"], false)).unwrap();
    // Each response has its own timestamp; the retained graph must stay equal.
    assert!(graph.field_u64("exported_at").is_some());
    assert!(graph_after.field_u64("exported_at").is_some());
    graph_after.set("exported_at", graph.get("exported_at").unwrap().clone());
    assert_eq!(graph_after, graph);
    assert_eq!(
        std::fs::read(sources::sources_path(&library.data())).unwrap(),
        source_before
    );
    assert_eq!(
        std::fs::read(store::catalog_path(&library.data())).unwrap(),
        catalog_before
    );
    for (album, _, _, _) in rows {
        assert_eq!(
            std::fs::read(library.0.join("music").join(album).join("01.flac")).unwrap(),
            include_bytes!("../../aede-core/tests/fixtures/track.flac")
        );
    }
    assert!(!library.data().join("conclusions.json").exists());
    assert_eq!(
        observed,
        (
            vec![
                "First Performance".to_string(),
                "Second Performance".to_string()
            ],
            vec![PARENT.to_string()],
            true,
            true,
        ),
        "one trusted child-parent assertion applies to every exact child performance"
    );
}

#[test]
fn an_attached_manual_credit_cannot_be_duplicated_after_its_representative_changes() {
    for kind in ["work", "recording"] {
        let library = Library::new();
        let file = |album: &str| {
            let path = library.0.join("music").join(album).join("01.flac");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let bytes = include_bytes!("../../aede-core/tests/fixtures/track.flac");
            std::fs::write(&path, bytes).unwrap();
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Local Player"),
                ("albumartist", "Local Player"),
                ("album", album),
                ("title", album),
                ("musicbrainz_workid", WORK),
                ("work", "One Composition"),
                ("musicbrainz_albumid", album),
            ] {
                tags.insert(key, value);
            }
            tags.insert(
                "musicbrainz_recordingid",
                match (kind, album) {
                    ("recording", _) => "shared-recording",
                    (_, "A") => "recording-a",
                    _ => "recording-b",
                },
            );
            model::ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: bytes.len() as u64,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        };
        let catalog = |albums: &[&str]| {
            model::build(
                albums.iter().map(|album| file(album)).collect(),
                vec![library.0.join("music").to_string_lossy().into_owned()],
                1,
                &[],
            )
        };
        store::save(&catalog(&["B"]), &store::catalog_path(&library.data())).unwrap();
        let scope = if kind == "work" {
            format!("work:{WORK}")
        } else {
            "recording:shared-recording".into()
        };
        let add = [
            "credit",
            "--add",
            &scope,
            "--artist=A Writer",
            "--artist-id=writer-id",
            "--role=composer",
        ];
        let notice = library.text(&add, true);
        let id = notice.split_whitespace().last().unwrap();
        let original_sources = std::fs::read(sources::sources_path(&library.data())).unwrap();
        store::save(&catalog(&["A", "B"]), &store::catalog_path(&library.data())).unwrap();
        assert_eq!(
            std::fs::read(sources::sources_path(&library.data())).unwrap(),
            original_sources,
            "a new representative does not move the retained observation"
        );
        assert_eq!(library.titles("composer:writer-id").len(), 2, "{kind}");
        let before = library.snapshot();
        let duplicate = library.run(&add, true);
        assert!(
            !duplicate.status.success(),
            "{kind}: the already attached manual credit was added again: {duplicate:?}"
        );
        assert_eq!(library.snapshot(), before, "{kind}: refused duplicate");
        assert!(
            String::from_utf8(duplicate.stderr)
                .unwrap()
                .contains("already present")
        );
        library.text(&["credit", "--exclude", id], true);
        let excluded = library.snapshot();
        assert!(!library.run(&add, true).status.success(), "excluded {kind}");
        assert_eq!(library.snapshot(), excluded);
        let edges = json::parse(&library.text(
            &["relations", "--source", "manual", "--json", "--all"],
            false,
        ))
        .unwrap();
        let excluded_credit = edges
            .as_arr()
            .unwrap()
            .iter()
            .find(|edge| edge.field_str("id").as_deref() == Some(id))
            .unwrap();
        assert_eq!(excluded_credit.field_optional_bool("excluded"), Some(true));
        assert_eq!(excluded_credit.field_optional_bool("trusted"), Some(false));
        library.text(&["credit", "--undo", id], true);
        assert_eq!(library.titles("composer:writer-id").len(), 2);

        for extra in [
            "--role=producer",
            "--instrument=piano",
            "--artist-id=another-writer",
        ] {
            let mut different = add.to_vec();
            if extra.starts_with("--role=") {
                different.pop();
            } else if extra.starts_with("--artist-id=") {
                different.retain(|argument| !argument.starts_with("--artist-id="));
            }
            different.push(extra);
            library.text(&different, true);
        }
        let edges = json::parse(&library.text(
            &["relations", "--source", "manual", "--json", "--all"],
            false,
        ))
        .unwrap();
        let original = edges
            .as_arr()
            .unwrap()
            .iter()
            .find(|edge| edge.field_str("id").as_deref() == Some(id))
            .unwrap();
        assert_eq!(original.field_u32("weight"), Some(1));

        // An unresolved old host remains waiting evidence. It cannot block
        // an explicit correction on a performance that is still available.
        store::save(&catalog(&["A"]), &store::catalog_path(&library.data())).unwrap();
        assert_eq!(library.titles("composer:writer-id").len(), 1);
        library.text(&add, true);
        let held = sources::load_all(&sources::sources_path(&library.data()))
            .unwrap()
            .unwrap();
        assert!(
            held.records
                .iter()
                .any(|record| record.key == library.0.join("music/B/01.flac").to_string_lossy())
        );
        for album in ["A", "B"] {
            assert_eq!(
                std::fs::read(library.0.join("music").join(album).join("01.flac")).unwrap(),
                include_bytes!("../../aede-core/tests/fixtures/track.flac")
            );
        }
    }
}

#[test]
fn adding_an_edition_credit_cannot_retarget_retained_credits_to_another_edition() {
    for (original, current, allowed) in [
        (Some("original-edition"), Some("other-edition"), false),
        (Some("original-edition"), None, false),
        (Some("original-edition"), Some("original-edition"), true),
        (None, Some("new-edition"), true),
        (None, None, true),
    ] {
        let library = Library::new();
        let audio = library.0.join("music/One Album/01.flac");
        std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
        let bytes = include_bytes!("../../aede-core/tests/fixtures/track.flac");
        std::fs::write(&audio, bytes).unwrap();
        let catalog = |edition: Option<&str>| {
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Local Player"),
                ("albumartist", "Local Player"),
                ("album", "One Album"),
                ("title", "A Song"),
                ("musicbrainz_recordingid", "one-recording"),
            ] {
                tags.insert(key, value);
            }
            if let Some(edition) = edition {
                tags.insert("musicbrainz_albumid", edition);
            }
            model::build(
                vec![model::ScannedFile {
                    path: audio.to_string_lossy().into_owned(),
                    size: bytes.len() as u64,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }],
                vec![library.0.join("music").to_string_lossy().into_owned()],
                1,
                &[],
            )
        };
        let original_catalog = catalog(original);
        let scope = aede_core::user::EntityRef::of(
            &original_catalog,
            model::EntityKind::Release,
            original_catalog.releases[0].id,
        )
        .unwrap()
        .to_token();
        store::save(&original_catalog, &store::catalog_path(&library.data())).unwrap();
        aede_core::user::save(&Default::default(), &library.data().join("user.json")).unwrap();
        aede_core::conclusions::save(
            &Default::default(),
            &library.data().join("conclusions.json"),
        )
        .unwrap();
        let add = |role: &str| {
            library.run(
                &[
                    "credit",
                    "--add",
                    &scope,
                    "--artist=A Contributor",
                    "--artist-id=contributor-id",
                    &format!("--role={role}"),
                ],
                true,
            )
        };
        let first = add("producer");
        assert!(first.status.success(), "{first:?}");
        assert_eq!(library.titles("producer:contributor-id"), ["A Song"]);
        let retained = std::fs::read(sources::sources_path(&library.data())).unwrap();
        store::save(&catalog(current), &store::catalog_path(&library.data())).unwrap();
        assert_eq!(
            std::fs::read(sources::sources_path(&library.data())).unwrap(),
            retained,
            "changing a catalog identity does not rewrite its retained claims"
        );
        let visible =
            json::parse(&library.text(&["relations", "--source=manual", "--json", "--all"], false))
                .unwrap();
        assert_eq!(visible.as_arr().unwrap().len(), usize::from(allowed));
        let before = library.snapshot();
        assert!(before.iter().all(Option::is_some));
        let second = add("engineer");
        assert_eq!(
            second.status.success(),
            allowed,
            "{original:?} -> {current:?}: {second:?}"
        );
        if allowed {
            assert_eq!(library.titles("producer:contributor-id"), ["A Song"]);
            assert_eq!(library.titles("engineer:contributor-id"), ["A Song"]);
        } else {
            assert_eq!(
                library.snapshot(),
                before,
                "refused retargeting preserves every store"
            );
            assert!(
                String::from_utf8(second.stderr)
                    .unwrap()
                    .contains("edition")
            );
            let visible = json::parse(
                &library.text(&["relations", "--source=manual", "--json", "--all"], false),
            )
            .unwrap();
            assert!(visible.as_arr().unwrap().is_empty());
        }
        assert_eq!(std::fs::read(&audio).unwrap(), bytes);
    }
}

#[test]
fn an_attached_manual_credit_cannot_be_duplicated_when_a_recording_gains_an_identity() {
    const RECORDING: &str = "33333333-3333-3333-3333-333333333333";
    const OTHER_RECORDING: &str = "44444444-4444-4444-4444-444444444444";
    const WRITER: &str = "22222222-2222-2222-2222-222222222222";
    for identified_artist in [false, true] {
        for excluded in [false, true] {
            let library = Library::new();
            let files = [
                library.0.join("music/One Album/01.flac"),
                library.0.join("music/One Album/02.flac"),
            ];
            let bytes = include_bytes!("../../aede-core/tests/fixtures/track.flac");
            std::fs::create_dir_all(files[0].parent().unwrap()).unwrap();
            for path in &files {
                std::fs::write(path, bytes).unwrap();
            }
            let catalog = |identified: bool| {
                model::build(
                    files
                        .iter()
                        .enumerate()
                        .map(|(index, path)| {
                            let mut tags = RawTags::default();
                            for (key, value) in [
                                ("artist", "Local Player"),
                                ("albumartist", "Local Player"),
                                ("album", "One Album"),
                                ("title", if index == 0 { "One Song" } else { "Other Song" }),
                                ("musicbrainz_albumid", "one-edition"),
                                ("musicbrainz_workid", WORK),
                                ("work", "One Work"),
                            ] {
                                tags.insert(key, value);
                            }
                            if index == 1 || identified {
                                tags.insert(
                                    "musicbrainz_recordingid",
                                    if index == 0 {
                                        RECORDING
                                    } else {
                                        OTHER_RECORDING
                                    },
                                );
                            }
                            model::ScannedFile {
                                path: path.to_string_lossy().into_owned(),
                                size: bytes.len() as u64,
                                mtime: 1,
                                tags,
                                folder_cover: None,
                                sidecar: None,
                                integrity: None,
                                fingerprint: None,
                            }
                        })
                        .collect(),
                    vec![library.0.join("music").to_string_lossy().into_owned()],
                    1,
                    &[],
                )
            };
            let initial = catalog(false);
            let local_scope = format!("recording:local:{}", files[0].to_string_lossy());
            let identified_scope = format!("recording:{RECORDING}");
            store::save(&initial, &store::catalog_path(&library.data())).unwrap();
            aede_core::user::save(&Default::default(), &library.data().join("user.json")).unwrap();
            aede_core::conclusions::save(
                &Default::default(),
                &library.data().join("conclusions.json"),
            )
            .unwrap();
            let mut add = vec![
                "credit",
                "--add",
                &local_scope,
                "--artist=A Writer",
                "--role=composer",
            ];
            let writer_id = format!("--artist-id={WRITER}");
            if identified_artist {
                add.push(&writer_id);
            }
            library.text(&add, true);
            let read_edges = || {
                let rows = json::parse(
                    &library.text(&["relations", "--source=manual", "--json", "--all"], false),
                )
                .unwrap();
                json::Json::Arr(
                    rows.as_arr()
                        .unwrap()
                        .iter()
                        .filter(|edge| {
                            edge.field_str("relation")
                                .is_some_and(|kind| kind.starts_with("credit:"))
                        })
                        .cloned()
                        .collect(),
                )
            };
            let original = read_edges();
            let original = &original.as_arr().unwrap()[0];
            let source_id = original.field_str("source_id").unwrap();
            if excluded {
                library.text(
                    &["credit", "--exclude", &original.field_str("id").unwrap()],
                    true,
                );
            }
            let retained = std::fs::read(sources::sources_path(&library.data())).unwrap();
            store::save(&catalog(true), &store::catalog_path(&library.data())).unwrap();
            assert_eq!(
                std::fs::read(sources::sources_path(&library.data())).unwrap(),
                retained,
                "identity enrichment preserves the original source assertion"
            );
            let attached = read_edges();
            let attached = attached.as_arr().unwrap();
            assert_eq!(attached.len(), 1);
            assert_eq!(attached[0].field_str("target").unwrap(), identified_scope);
            assert_eq!(attached[0].field_str("source_id").unwrap(), source_id);
            assert_eq!(attached[0].field_optional_bool("excluded"), Some(excluded));
            assert_eq!(attached[0].field_optional_bool("trusted"), Some(!excluded));

            for scope in [&local_scope, &identified_scope] {
                let mut repeated = add.clone();
                repeated[2] = scope;
                repeated[3] = if identified_artist {
                    "--artist=Writer Alias"
                } else {
                    "--artist=a writer"
                };
                let before = library.snapshot();
                assert!(before.iter().all(Option::is_some));
                let duplicate = library.run(&repeated, true);
                assert!(
                    !duplicate.status.success(),
                    "identity enrichment admitted an already attached manual credit: {duplicate:?}"
                );
                assert_eq!(
                    library.snapshot(),
                    before,
                    "refused duplicate preserves every store"
                );
                assert!(
                    String::from_utf8(duplicate.stderr)
                        .unwrap()
                        .contains("already present")
                );
            }
            if excluded {
                library.text(
                    &["credit", "--undo", &attached[0].field_str("id").unwrap()],
                    true,
                );
                let restored = read_edges();
                let restored = restored.as_arr().unwrap();
                assert_eq!(restored.len(), 1);
                assert_eq!(restored[0].field_str("source_id").unwrap(), source_id);
                assert_eq!(restored[0].field_optional_bool("excluded"), Some(false));
                assert_eq!(restored[0].field_optional_bool("trusted"), Some(true));
            }

            for (scope, role, artist_id, instrument) in [
                (
                    identified_scope.as_str(),
                    "producer",
                    identified_artist.then_some(WRITER),
                    None,
                ),
                (
                    identified_scope.as_str(),
                    "composer",
                    identified_artist.then_some(WRITER),
                    Some("piano"),
                ),
                (
                    identified_scope.as_str(),
                    "composer",
                    Some("another-writer"),
                    None,
                ),
                (
                    &format!("recording:{OTHER_RECORDING}"),
                    "composer",
                    identified_artist.then_some(WRITER),
                    None,
                ),
                (
                    &format!("work:{WORK}"),
                    "composer",
                    identified_artist.then_some(WRITER),
                    None,
                ),
                (
                    "release:one-edition",
                    "composer",
                    identified_artist.then_some(WRITER),
                    None,
                ),
            ] {
                let role_arg = format!("--role={role}");
                let artist_arg = artist_id.map(|id| format!("--artist-id={id}"));
                let instrument_arg = instrument.map(|name| format!("--instrument={name}"));
                let mut distinct = vec!["credit", "--add", scope, "--artist=A Writer", &role_arg];
                if let Some(argument) = &artist_arg {
                    distinct.push(argument);
                }
                if let Some(argument) = &instrument_arg {
                    distinct.push(argument);
                }
                library.text(&distinct, true);
            }
            let edges = read_edges();
            let edges = edges.as_arr().unwrap();
            assert_eq!(
                edges.len(),
                7,
                "different roles, instruments, artists and scopes remain distinct"
            );
            assert_eq!(
                edges
                    .iter()
                    .filter(|edge| edge.field_str("source_id").as_deref() == Some(&source_id))
                    .count(),
                1,
                "the original assertion keeps its relationship ID"
            );
            let mut held = sources::load_all(&sources::sources_path(&library.data()))
                .unwrap()
                .unwrap();
            let record = held
                .records
                .iter_mut()
                .find(|record| record.key == files[0].to_string_lossy())
                .unwrap();
            let sources::Facts::Track(facts) = &mut record.facts else {
                panic!("the manual recording claim has track facts");
            };
            let mut imported = facts
                .credits
                .iter()
                .find(|credit| credit.relation_id.as_deref() == Some(&source_id))
                .unwrap()
                .clone();
            imported.relation_id = Some("custom-imported-credit".into());
            imported.role = "arranger".into();
            facts.credits.push(imported);
            sources::save(&held, &sources::sources_path(&library.data())).unwrap();
            let mut independent = add.clone();
            independent[2] = &identified_scope;
            independent[4] = "--role=arranger";
            library.text(&independent, true);
            let edges = read_edges();
            let edges = edges.as_arr().unwrap();
            assert_eq!(
                edges.len(),
                9,
                "a custom imported assertion ID remains independent"
            );
            assert_eq!(
                edges
                    .iter()
                    .filter(|edge| edge.field_str("source_id").as_deref()
                        == Some("custom-imported-credit"))
                    .count(),
                1
            );
            for path in files {
                assert_eq!(std::fs::read(path).unwrap(), bytes);
            }
        }
    }
}

#[test]
fn manual_recording_credits_keep_their_known_performance_when_file_identity_changes() {
    for legacy in [false, true] {
        for (original, current) in [
            (Some("original-recording"), Some("other-recording")),
            (Some("original-recording"), None),
            (Some("original-recording"), Some("original-recording")),
            (None, Some("new-recording")),
            (None, None),
        ] {
            let library = Library::new();
            let audio = library.0.join("music/One Album/01.flac");
            std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
            let bytes = include_bytes!("../../aede-core/tests/fixtures/track.flac");
            std::fs::write(&audio, bytes).unwrap();
            let catalog = |recording: Option<&str>| {
                let mut tags = RawTags::default();
                for (key, value) in [
                    ("artist", "Local Player"),
                    ("albumartist", "Local Player"),
                    ("album", "One Album"),
                    ("title", "One Song"),
                    ("musicbrainz_artistid", "player-id"),
                    ("musicbrainz_workid", WORK),
                    ("work", "One Work"),
                ] {
                    tags.insert(key, value);
                }
                if let Some(recording) = recording {
                    tags.insert("musicbrainz_recordingid", recording);
                }
                model::build(
                    vec![model::ScannedFile {
                        path: audio.to_string_lossy().into_owned(),
                        size: bytes.len() as u64,
                        mtime: 1,
                        tags,
                        folder_cover: None,
                        sidecar: None,
                        integrity: None,
                        fingerprint: None,
                    }],
                    vec![library.0.join("music").to_string_lossy().into_owned()],
                    1,
                    &[],
                )
            };
            let initial = catalog(original);
            let scope = |catalog: &model::Catalog| {
                aede_core::user::EntityRef::of(
                    catalog,
                    model::EntityKind::Recording,
                    catalog.recordings[0].id,
                )
                .unwrap()
                .to_token()
            };
            store::save(&initial, &store::catalog_path(&library.data())).unwrap();
            aede_core::user::save(&Default::default(), &library.data().join("user.json")).unwrap();
            aede_core::conclusions::save(
                &Default::default(),
                &library.data().join("conclusions.json"),
            )
            .unwrap();
            library.text(
                &[
                    "credit",
                    "--add",
                    &format!("work:{WORK}"),
                    "--artist=Work Writer",
                    "--artist-id=writer-id",
                    "--role=composer",
                ],
                true,
            );
            library.text(
                &[
                    "credit",
                    "--add",
                    &scope(&initial),
                    "--artist=Local Player",
                    "--artist-id=player-id",
                    "--role=producer",
                ],
                true,
            );
            if legacy {
                let path = sources::sources_path(&library.data());
                let mut held = sources::load_all(&path).unwrap().unwrap();
                let record = held
                    .records
                    .iter_mut()
                    .find(|row| row.source == "manual")
                    .unwrap();
                let sources::Facts::Track(facts) = &mut record.facts else {
                    panic!("manual recording observation must retain track facts");
                };
                // Older versions did not capture the selected recording identity.
                facts.recording = None;
                sources::save(&held, &path).unwrap();
            }
            assert_eq!(library.titles("producer:Local Player"), ["One Song"]);
            assert_eq!(library.titles("composer:writer-id"), ["One Song"]);
            let retained = std::fs::read(sources::sources_path(&library.data())).unwrap();
            let changed = catalog(current);
            store::save(&changed, &store::catalog_path(&library.data())).unwrap();
            let allowed = legacy || original.is_none() || original == current;
            let projected = json::parse(
                &library.text(&["relations", "--source=manual", "--json", "--all"], false),
            )
            .unwrap();
            let producer_edges: Vec<_> = projected
                .as_arr()
                .unwrap()
                .iter()
                .filter(|row| row.field_str("relation").as_deref() == Some("credit:producer"))
                .collect();
            assert_eq!(
                producer_edges.len(),
                usize::from(allowed),
                "legacy={legacy}, {original:?} -> {current:?}: precise recording credit moved"
            );
            assert_eq!(
                library.titles("producer:Local Player"),
                if allowed { vec!["One Song"] } else { vec![] }
            );
            assert_eq!(library.titles("composer:writer-id"), ["One Song"]);
            assert_eq!(
                std::fs::read(sources::sources_path(&library.data())).unwrap(),
                retained,
                "reads must not migrate or overwrite the original source evidence"
            );
            let detail =
                json::parse(&library.text(&["track", "One Song", "--json"], false)).unwrap();
            let direct_count = detail.as_arr().unwrap()[0]
                .get("sourced_credits")
                .unwrap()
                .as_arr()
                .unwrap()
                .iter()
                .filter(|row| row.field_str("role").as_deref() == Some("producer"))
                .count();
            assert_eq!(direct_count, usize::from(allowed));
            let before = library.snapshot();
            assert!(before.iter().all(Option::is_some));
            let second = library.run(
                &[
                    "credit",
                    "--add",
                    &scope(&changed),
                    "--artist=Local Player",
                    "--artist-id=player-id",
                    "--role=engineer",
                ],
                true,
            );
            assert_eq!(second.status.success(), allowed, "{second:?}");
            if !allowed {
                assert_eq!(library.snapshot(), before, "refusal preserves all stores");
                assert!(
                    String::from_utf8(second.stderr)
                        .unwrap()
                        .contains("recording")
                );
            } else {
                assert_eq!(library.titles("producer:Local Player"), ["One Song"]);
                assert_eq!(library.titles("engineer:Local Player"), ["One Song"]);
            }
            assert_eq!(std::fs::read(&audio).unwrap(), bytes);
        }
    }
}
