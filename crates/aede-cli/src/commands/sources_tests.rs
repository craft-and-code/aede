//! What the `sources` command shows and stores.
//!
//! In its own file from the start, under the rule `fetch` established: the
//! module is declared in `sources.rs` with `#[path]`, so these are its child
//! module and reach its private functions through `use super::*`.
//!
//! The display is what most of this covers, because the display is where this
//! command's mistakes have actually been: a column that claimed the tags were
//! silent when there was no tag to be silent, and a section that rendered
//! "(no results)" for a source that had answered and held nothing.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{ArtistFacts, ReleaseFacts};
use aede_core::tags::RawTags;

/// A one-album, one-artist catalog whose tags say what is given.
fn catalog_with(genre: &str, label: &str) -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("date", "1959");
    tags.insert("genre", genre);
    tags.insert("label", label);
    build(
        vec![ScannedFile {
            path: "/music/Miles/Kind of Blue/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }],
        vec!["/music".to_string()],
        1,
    )
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_sources_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
}

fn saved(name: &str, catalog: &Catalog) -> std::path::PathBuf {
    let dir = sandbox(name);
    aede_core::store::save(catalog, &aede_core::store::catalog_path(&dir)).expect("a catalog");
    dir
}

fn args(dir: &std::path::Path, extra: &[&str]) -> Args {
    let mut raw = vec!["sources".to_string(), format!("--data={}", dir.display())];
    raw.extend(extra.iter().map(|s| s.to_string()));
    Args::parse(raw)
}

fn artist_of(catalog: &Catalog) -> EntityRef {
    EntityRef::of(catalog, EntityKind::Artist, 0).expect("an artist")
}

fn release_of(catalog: &Catalog) -> EntityRef {
    EntityRef::of(catalog, EntityKind::Release, 0).expect("a release")
}

#[test]
fn a_field_the_tags_cannot_hold_is_not_a_field_they_are_missing() {
    // The bug this pins: artist fields were shown against a "Your tags" column
    // reading "nothing in your tags", which sends the reader looking for a tag
    // to fill that has never existed. There is no widely used tag for where an
    // artist is from, so the right answer is no verdict at all.
    let catalog = catalog_with("Jazz", "Columbia");
    let rows = compared(
        &catalog,
        &artist_of(&catalog),
        &Facts::Artist(ArtistFacts {
            area: Some("United States".to_string()),
            kind: Some("Person".to_string()),
            ..Default::default()
        }),
    );
    assert!(
        rows.iter().all(|(_, _, verdict)| verdict.is_none()),
        "no artist field is judged against a tag: {rows:?}"
    );
    assert!(rows.iter().any(|(field, _, _)| *field == "from"));
    assert!(
        !rows.iter().any(|(field, _, _)| *field == "country"),
        "an area is a country, a city or a region — \"country\" was the wrong word"
    );
}

#[test]
fn a_release_field_is_judged_against_the_tag_that_answers_it() {
    // The other half: a release does have tag counterparts, which is what
    // makes agreement and disagreement observable at all.
    let catalog = catalog_with("Jazz", "Columbia");
    let rows = compared(
        &catalog,
        &release_of(&catalog),
        &Facts::Release(ReleaseFacts {
            primary_type: Some("Album".to_string()),
            first_released: Some("1959-08-17".to_string()),
            label: Some("Blue Note".to_string()),
            secondary_types: vec![],
        }),
    );

    let verdict_for = |name: &str| {
        rows.iter()
            .find(|(field, _, _)| *field == name)
            .and_then(|(_, _, v)| v.clone())
    };

    // No RELEASETYPE tag here: the source adds rather than contradicts.
    assert_eq!(verdict_for("release type"), Some(Verdict::NothingToCompare));
    // A full date against a bare year is agreement, not a disagreement — the
    // false alarm that would otherwise land on nearly every album.
    assert_eq!(verdict_for("first released"), Some(Verdict::Agrees));
    // And a real difference names both sides.
    assert_eq!(
        verdict_for("label"),
        Some(Verdict::Differs {
            theirs: "Blue Note".to_string(),
            yours: "Columbia".to_string()
        })
    );
}

#[test]
fn a_genre_is_compared_with_the_one_your_files_carry() {
    // A genre lives on the tracks, not on the artist, so "what do my files
    // call this artist" is answered by the files they appear on.
    let catalog = catalog_with("Jazz", "Columbia");
    let entity = artist_of(&catalog);
    assert_eq!(tags_of_artist(&catalog, &entity, "genre"), vec!["Jazz"]);

    let genres_of = |theirs: &[&str]| {
        compared(
            &catalog,
            &entity,
            &Facts::Artist(ArtistFacts {
                genres: theirs.iter().map(|g| g.to_string()).collect(),
                ..Default::default()
            }),
        )
        .into_iter()
        .find(|(field, _, _)| *field == "genres")
        .expect("a genres row")
    };

    let agreeing = genres_of(&["jazz", "cool jazz"]);
    assert_eq!(agreeing.1, "jazz, cool jazz", "all of them are shown");
    assert_eq!(
        agreeing.2,
        Some(Verdict::Agrees),
        "case does not matter, and a shared name is agreement"
    );

    // The false alarm this replaced: the top genre was compared, as a string,
    // against the whole tag. A tag naming two genres, one of which the source
    // also names, is not a contradiction.
    let overlapping = genres_of(&["cool jazz", "jazz"]);
    assert_eq!(
        overlapping.2,
        Some(Verdict::Agrees),
        "the shared name need not be first on either side"
    );

    let disjoint = genres_of(&["techno", "house"]);
    assert!(
        matches!(disjoint.2, Some(Verdict::Differs { .. })),
        "and two sets with nothing in common still differ: {:?}",
        disjoint.2
    );
}

#[test]
fn what_a_record_says_reads_as_a_sentence_or_says_it_holds_nothing() {
    let full = says(&Facts::Artist(ArtistFacts {
        kind: Some("Group".to_string()),
        area: Some("United States".to_string()),
        began: Some("1989".to_string()),
        active: Some(true),
        ..Default::default()
    }));
    assert!(full.contains("Group"), "{full}");
    assert!(full.contains("1989–"), "an open period stays open: {full}");
    assert!(full.contains("still active"), "{full}");

    let ended = says(&Facts::Artist(ArtistFacts {
        began: Some("1926".to_string()),
        ended: Some("1991".to_string()),
        active: Some(false),
        ..Default::default()
    }));
    assert!(ended.contains("1926–1991"), "{ended}");

    // An answer holding nothing is a real answer: printing an empty cell would
    // make it look like a missing row instead.
    let empty = says(&Facts::Artist(ArtistFacts::default()));
    assert!(empty.contains("nothing"), "{empty}");
}

#[test]
fn a_search_and_a_lookup_do_not_read_the_same() {
    // The distinction the whole layer turns on, in the one place a reader
    // sees it.
    assert_eq!(confidence_label(Confidence::Identified), "identified");
    assert_eq!(confidence_label(Confidence::matched(88)), "matched 88%");
}

#[test]
fn a_template_carries_the_keys_and_an_import_takes_them_back() {
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("roundtrip", &catalog);
    let written = dir.join("template.json");

    template(&args(
        &dir,
        &[
            "--template",
            "--source=manual",
            "--output",
            written.to_str().unwrap(),
            "Kind of Blue",
        ],
    ))
    .expect("a template");

    let text = std::fs::read_to_string(&written).expect("a document");
    assert!(text.contains("\"source\": \"manual\""), "{text}");
    assert!(text.contains("\"entity\": \"release:"), "{text}");
    assert!(text.contains("\"primary_type\": null"), "{text}");

    // Filled in and taken back, it lands under the source it names.
    std::fs::write(
        &written,
        text.replace("\"primary_type\": null", "\"primary_type\": \"Album\""),
    )
    .unwrap();
    import(
        &args(&dir, &[&format!("--import={}", written.display())]),
        &sources::sources_path(&dir),
    )
    .expect("an import");

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1);
    assert_eq!(held.records[0].source, "manual");
    match &held.records[0].facts {
        Facts::Release(r) => assert_eq!(r.primary_type.as_deref(), Some("Album")),
        other => panic!("expected release facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn naming_something_the_catalog_does_not_hold_is_refused() {
    // Writing an empty document and reporting success would look like an
    // answer. It is not one.
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("nomatch", &catalog);
    let target = dir.join("template.json");
    let refused = template(&args(
        &dir,
        &[
            "--template",
            "--output",
            target.to_str().unwrap(),
            "an album nobody owns",
        ],
    ))
    .expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("nothing in this catalog matches")
    );
    assert!(!target.exists(), "and no file was written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn forgetting_a_source_nobody_stored_is_an_error_and_removes_nothing() {
    // An instruction that cannot be honoured is refused, not treated as a
    // no-op — and above all it must not take the other sources with it.
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("forget", &catalog);
    let path = sources::sources_path(&dir);

    let mut held = Sources::default();
    held.set(SourceRecord {
        key: artist_of(&catalog).key,
        source: sources::MUSICBRAINZ.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts::default()),
    });
    sources::save(&held, &path).expect("saved");

    let refused = forget(&args(&dir, &["--forget", "--source=discogs"]), &path)
        .expect_err("an unknown source is refused");
    assert!(refused.to_string().contains("no source named"), "{refused}");

    let after = sources::load(&path).expect("readable").expect("a layer");
    assert_eq!(after.records.len(), 1, "nothing was removed on the way out");

    // And forgetting the one that is there empties it.
    forget(&args(&dir, &["--forget"]), &path).expect("forgotten");
    let empty = sources::load(&path).expect("readable").expect("a layer");
    assert!(empty.records.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn there_is_nothing_to_export_before_anything_is_fetched() {
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("export", &catalog);
    let path = sources::sources_path(&dir);
    let refused = export(&args(&dir, &["--export"]), &path).expect_err("refused");
    assert!(refused.to_string().contains("nothing has been fetched"));
    assert!(
        refused.to_string().contains("--template"),
        "and says what to do instead: {refused}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
