//! Work pages preserve source confidence and explicit review decisions.

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

use aede_core::{conclusions, json, model, sources, store, tags::RawTags, user};
use sources::{Confidence, Facts, SourceRecord, Sources, TrackFacts, WorkLink};
use support::Library;

#[test]
fn work_pages_preserve_accepted_match_scores_and_each_sources_provenance() {
    const WORK: &str = "11111111-1111-1111-1111-111111111111";
    const RECORDING: &str = "73eac6f8-522a-4eab-8cf5-1e708f666e31";

    for canonical in [false, true] {
        let library = Library::new("work-evidence", false);
        let mut catalog = store::load(&store::catalog_path(&library.data))
            .unwrap()
            .unwrap();
        if canonical {
            let mut tags = RawTags {
                fields: catalog.files[0].tags.clone(),
                ..Default::default()
            };
            tags.insert("musicbrainz_workid", WORK);
            tags.insert("work", "A Reviewed Work");
            catalog = model::build(
                vec![model::ScannedFile {
                    path: library.audio.to_string_lossy().into_owned(),
                    size: 24,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }],
                catalog.roots,
                1,
                &[],
            );
            store::save(&catalog, &store::catalog_path(&library.data)).unwrap();
        }
        user::save(&user::UserData::default(), &user::user_path(&library.data)).unwrap();
        conclusions::save(
            &conclusions::Conclusions::default(),
            &conclusions::conclusions_path(&library.data),
        )
        .unwrap();
        let music_before = filesystem::snapshot(&library.music);
        let read = |arguments: &[&str], success: bool| {
            let before = filesystem::snapshot(&library.data);
            let output = library.run(arguments);
            assert_eq!(
                output.status.success(),
                success,
                "{arguments:?}: {output:?}"
            );
            assert_eq!(filesystem::snapshot(&library.data), before, "{arguments:?}");
            assert_eq!(filesystem::snapshot(&library.music), music_before);
            format!(
                "{}{}",
                String::from_utf8(output.stdout).unwrap(),
                String::from_utf8(output.stderr).unwrap()
            )
        };
        let write = |arguments: &[&str]| {
            let output = library.run(arguments);
            assert!(output.status.success(), "{arguments:?}: {output:?}");
            assert_eq!(filesystem::snapshot(&library.music), music_before);
        };
        let record = SourceRecord {
            key: library.audio.to_string_lossy().into_owned(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(RECORDING.into()),
            fetched_at: 1,
            confidence: Confidence::matched(93),
            facts: Facts::Track(TrackFacts {
                recording: Some(RECORDING.into()),
                relationships_complete: true,
                works: vec![WorkLink {
                    mbid: WORK.into(),
                    title: "A Reviewed Work".into(),
                    attributes: vec![model::CreditAttribute {
                        id: None,
                        name: "live".into(),
                        value: None,
                        credited_as: None,
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }),
        };
        let document = library.directory.join("work-evidence.json");
        let import = |record: SourceRecord| {
            let incoming = Sources {
                records: vec![record],
                ..Default::default()
            };
            std::fs::write(&document, sources::to_json(&incoming).to_string_pretty()).unwrap();
            write(&["sources", "--import", document.to_str().unwrap()]);
        };
        import(record.clone());
        let pending = read(&["work", WORK], canonical);
        if canonical {
            assert!(pending.contains("musicbrainz · matched 93% · pending review"));
        } else {
            assert!(pending.contains("no work matches"));
            assert_eq!(
                read(&["query", &format!("work:{WORK}"), "--json"], true).trim(),
                "[]"
            );
        }
        let held = sources::load_all(&sources::sources_path(&library.data))
            .unwrap()
            .unwrap();
        let review = held.review_items(&catalog)[0].id.clone();
        write(&["review", &format!("--accept={review}")]);
        let accepted = read(&["work", WORK], true);
        assert!(
            accepted.contains("musicbrainz · matched 93% · accepted"),
            "{accepted}"
        );
        assert!(!accepted.contains("musicbrainz · identified"), "{accepted}");
        if !canonical {
            assert!(accepted.contains(" · live"), "{accepted}");
        }
        write(&[
            "credit",
            "--add",
            &format!("work:{WORK}"),
            "--artist=Manual Composer",
            "--artist-id=manual-composer-id",
            "--role=composer",
        ]);
        let mixed = read(&["work", WORK], true);
        assert!(
            mixed.contains("musicbrainz · matched 93% · accepted"),
            "{mixed}"
        );
        assert!(mixed.contains("manual · identified"), "{mixed}");
        let exported = read(&["sources", "--export"], true);
        let exported = sources::from_json(&json::parse(&exported).unwrap()).unwrap();
        assert_eq!(
            exported.get(&record.entity(), sources::MUSICBRAINZ),
            Some(&record)
        );
        assert_eq!(
            exported.review_for(&record).unwrap().decision,
            sources::ReviewDecision::Accepted
        );

        write(&["sources", "--forget", "--source=manual"]);
        write(&["review", &format!("--reject={review}")]);
        let rejected = read(&["work", WORK], canonical);
        if canonical {
            assert!(rejected.contains("musicbrainz · matched 93% · rejected"));
        } else {
            assert!(rejected.contains("no work matches"));
            assert_eq!(
                read(&["query", &format!("work:{WORK}"), "--json"], true).trim(),
                "[]"
            );
        }
        write(&["review", &format!("--undo={review}")]);
        read(&["work", WORK], canonical);
        import(SourceRecord {
            confidence: Confidence::Identified,
            ..record
        });
        let identified = read(&["work", WORK], true);
        assert!(
            identified.contains("musicbrainz · identified"),
            "{identified}"
        );
        assert!(!identified.contains("matched") && !identified.contains("accepted"));
    }
}
