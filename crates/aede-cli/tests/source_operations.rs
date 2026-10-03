//! Public regressions for M1 source decisions and portable rules.

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

use aede_core::{json, sources, store, user};
use sources::{ReviewDecision, SourceReview, Sources};
use support::Library;

#[test]
fn reading_m1_data_creates_no_writer_lock() {
    let library = Library::new("reads", false);
    let before = filesystem::snapshot(&library.data);
    for arguments in [
        vec!["sources"],
        vec!["sources", "--list"],
        vec!["sources", "--export"],
        vec!["sources", "--template"],
        vec!["review"],
        vec!["rules"],
        vec!["fingerprint", "--list"],
    ] {
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            filesystem::snapshot(&library.data) == before,
            "{arguments:?} must leave data files and locks untouched"
        );
    }
}

fn decisions_only(library: &Library) -> Sources {
    let mut held = sources::load_all(&sources::sources_path(&library.data))
        .unwrap()
        .unwrap();
    let record = held.records.pop().unwrap();
    held.set_review(SourceReview {
        entity: record.entity(),
        source: record.source,
        source_id: record.source_id,
        decision: ReviewDecision::Rejected,
        reviewed_at: 1,
    });
    held.exclude_credit(
        aede_core::graph::RelationRef {
            source: user::EntityRef::new(aede_core::model::EntityKind::Artist, "miles davis"),
            kind: "credit:performer".into(),
            target: user::EntityRef::new(
                aede_core::model::EntityKind::Recording,
                "73eac6f8-522a-4eab-8cf5-1e708f666e31",
            ),
            provenance: sources::MUSICBRAINZ.into(),
            source_id: None,
        },
        1,
    );
    sources::save(&held, &sources::sources_path(&library.data)).unwrap();
    held
}

#[test]
fn decisions_without_cached_records_can_be_exported_and_forgotten() {
    for selected in [false, true] {
        let library = Library::new("decisions", false);
        let held = decisions_only(&library);
        let output = library.run(&["sources", "--export"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let exported =
            sources::from_json(&json::parse(&String::from_utf8(output.stdout).unwrap()).unwrap())
                .unwrap();
        assert_eq!(exported.reviews, held.reviews);
        assert_eq!(exported.credit_exclusions, held.credit_exclusions);
        let arguments = if selected {
            vec!["sources", "--forget", "--source=musicbrainz"]
        } else {
            vec!["sources", "--forget"]
        };
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("decision"));
        let after = sources::load_all(&sources::sources_path(&library.data))
            .unwrap()
            .unwrap();
        assert!(
            after.records.is_empty()
                && after.reviews.is_empty()
                && after.credit_exclusions.is_empty()
        );
    }
}

#[test]
fn conflicting_source_operations_and_unused_filters_are_refused() {
    let library = Library::new("options", false);
    let path = sources::sources_path(&library.data);
    let before = std::fs::read(&path).unwrap();
    let before_music = filesystem::snapshot(&library.music);
    for arguments in [
        vec!["sources", "--export", "--forget"],
        vec!["sources", "--list", "--forget"],
        vec!["sources", "--source=musicbrainz"],
        vec!["sources", "--export", "--source=musicbrainz"],
    ] {
        let output = library.run(&arguments);
        assert!(!output.status.success(), "accepted {arguments:?}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    assert_eq!(filesystem::snapshot(&library.music), before_music);
}

#[test]
fn an_empty_review_still_checks_its_window_options() {
    let library = Library::new("review", false);
    for arguments in [
        vec!["review", "--limit=bad"],
        vec!["review", "--offset=bad"],
    ] {
        let output = library.run(&arguments);
        assert!(!output.status.success(), "accepted {arguments:?}");
    }
}

#[test]
fn fingerprint_listing_escapes_controls_without_changing_the_catalog() {
    let library = Library::new("fingerprint", false);
    let path = store::catalog_path(&library.data);
    let mut catalog = store::load(&path).unwrap().unwrap();
    catalog.files[0].path.push_str("\u{1b}[2J\rhidden");
    catalog.files[0]
        .fingerprint
        .as_mut()
        .unwrap()
        .data
        .push_str("\u{1b}]0;title\u{7}");
    store::save(&catalog, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let before_music = filesystem::snapshot(&library.music);
    let before_audio = std::fs::read(&library.audio).unwrap();
    let output = library.run(&["fingerprint", "--list"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b)
            && !output.stdout.contains(&b'\r')
            && !output.stdout.contains(&7)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("\\u{1b}"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(filesystem::snapshot(&library.music), before_music);
    assert_eq!(std::fs::read(&library.audio).unwrap(), before_audio);
}

#[cfg(unix)]
#[test]
fn rules_import_checks_both_destinations_before_writing_either() {
    let library = Library::new("rules", false);
    let user_path = user::user_path(&library.data);
    let mut incoming = user::UserData::default();
    incoming.same_artist.push(user::SameArtist {
        owner: user::LOCAL_USER.into(),
        spelling: "miles dewey davis".into(),
        filed_as: "miles davis".into(),
        created_at: 1,
    });
    user::save(&incoming, &user_path).unwrap();
    let rule_path = library.directory.join("rules.json");
    let export = library.run(&[
        "rules",
        "--export",
        &format!("--output={}", rule_path.display()),
    ]);
    assert!(export.status.success());
    user::save(&user::UserData::default(), &user_path).unwrap();
    let before_user = std::fs::read(&user_path).unwrap();
    let source_path = sources::sources_path(&library.data);
    let real = library.directory.join("real-sources.json");
    std::fs::rename(&source_path, &real).unwrap();
    std::os::unix::fs::symlink(&real, &source_path).unwrap();
    let before_sources = std::fs::read(&real).unwrap();
    let output = library.run(&["rules", &format!("--import={}", rule_path.display())]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&user_path).unwrap(), before_user);
    assert_eq!(std::fs::read(&real).unwrap(), before_sources);

    let fresh = library.directory.join("new-data");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("rules")
        .arg(format!("--import={}", rule_path.display()))
        .arg(format!("--data={}", fresh.display()))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        user::load(&user::user_path(&fresh))
            .unwrap()
            .unwrap()
            .same_artist
            .len(),
        1
    );
}

#[test]
fn a_rules_bundle_cannot_import_nonportable_data() {
    let library = Library::new("rules-scope", false);
    let rule_path = library.directory.join("rules.json");
    let export = library.run(&[
        "rules",
        "--export",
        &format!("--output={}", rule_path.display()),
    ]);
    assert!(export.status.success());
    let source_path = sources::sources_path(&library.data);
    let before_sources = std::fs::read(&source_path).unwrap();
    let original = json::parse(&std::fs::read_to_string(&rule_path).unwrap()).unwrap();
    let mut unrelated = user::UserData::default();
    unrelated
        .entry(
            user::LOCAL_USER,
            &user::EntityRef::new(aede_core::model::EntityKind::Artist, "miles davis"),
            1,
        )
        .loved = true;
    for (table, value) in [
        (
            "sources",
            json::parse(&String::from_utf8(before_sources.clone()).unwrap()).unwrap(),
        ),
        ("user", user::to_json(&unrelated)),
    ] {
        let mut document = original.clone();
        document.set(table, value);
        std::fs::write(&rule_path, document.to_string_pretty()).unwrap();
        let output = library.run(&["rules", &format!("--import={}", rule_path.display())]);
        assert!(
            !output.status.success(),
            "{table} data outside personal rules was accepted"
        );
        assert!(!user::user_path(&library.data).exists());
        assert_eq!(std::fs::read(&source_path).unwrap(), before_sources);
    }
}

#[test]
fn source_paragraphs_and_interactive_review_show_imported_text_literally() {
    let library = Library::new("paragraphs", false);
    let path = sources::sources_path(&library.data);
    let mut held = sources::load_all(&path).unwrap().unwrap();
    let record = &mut held.records[0];
    record.source = "manual".into();
    record.confidence = sources::Confidence::Matched(80);
    if let sources::Facts::Artist(facts) = &mut record.facts {
        facts.kind = Some("test\u{1b}[2J\rhidden".into());
        facts.summary = Some(sources::Prose {
            text: "Imported test paragraph\u{1b}[2J\rhidden".into(),
            url: "https://example.org/imported\u{1b}]0;title\u{7}".into(),
            lang: "en".into(),
            licence: "test data".into(),
        });
    }
    sources::save(&held, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    for arguments in [
        vec!["artist", "Miles Davis"],
        vec!["review", "--interactive"],
    ] {
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.contains(&0x1b)
                && !output.stdout.contains(&b'\r')
                && !output.stdout.contains(&7),
            "unsafe {arguments:?}"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("\\u{1b}"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
