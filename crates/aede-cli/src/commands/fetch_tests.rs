//! What `fetch` does, proved without a network.
//!
//! Split out of `fetch.rs`, which was half tests: the module is declared there
//! with `#[path]`, so these are still `fetch`'s own child module and still see
//! its private items through `use super::*`. Nothing about what they can reach
//! changed — only which file they sit in.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

/// A transport that answers from canned text, and remembers what was asked.
struct Canned {
    answers: Vec<Result<String, Refusal>>,
    asked: Vec<String>,
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal> {
        self.asked.push(url.to_string());
        // Running out is a refusal, not a panic. A run asks about artists and
        // then about albums, so a test interested in only one half would
        // otherwise die three frames from anything it was asserting.
        if self.answers.is_empty() {
            return Err(Refusal::Failed("nothing canned for this".to_string()));
        }
        match self.answers.remove(0) {
            Ok(text) => Ok(aede_core::json::parse(&text).expect("valid fixture")),
            Err(why) => Err(why),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        // These passes ask questions and never download anything; a call here
        // is a mistake worth failing on rather than a case worth answering.
        self.asked.push(url.to_string());
        Err(Refusal::Failed("this pass downloads nothing".to_string()))
    }
}

fn library(dir: &std::path::Path) -> std::path::PathBuf {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Miles/Kind of Blue/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    std::fs::create_dir_all(dir).expect("a data folder");
    let path = aede_core::store::catalog_path(dir);
    aede_core::store::save(&catalog, &path).expect("a catalog");
    path
}

/// The test that owns this folder, for a name no other test can produce.
///
/// **Both halves are needed, and each was learnt the hard way.** Naming a
/// folder by the argument alone works only while no two tests pass the same
/// word — a rule nothing enforces and no grep can check, because a helper
/// called from three tests spells the word once: three of them shared a folder,
/// each deleting it as it started, and the race passed on Linux and failed on
/// macOS. Naming it by the test alone then broke the opposite case within a
/// single test, where two sandboxes are two folders on purpose. So the name is
/// the test **and** the argument: unique across tests however they arrive here,
/// unique within one, and the same on the next run, so a re-run still clears
/// what the last one left.
fn owner() -> String {
    std::thread::current()
        .name()
        .map(|name| name.replace("::", "_"))
        .unwrap_or_else(|| "main".to_string())
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_fetch_{}_{name}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    library(&dir);
    dir
}

fn args(dir: &std::path::Path, extra: &[&str]) -> Args {
    let mut raw = vec!["fetch".to_string(), format!("--data={}", dir.display())];
    raw.extend(extra.iter().map(|s| s.to_string()));
    Args::parse(raw)
}

#[test]
fn fanart_exclusions_select_each_image_family_independently() {
    let parsed = args(
        std::path::Path::new("/tmp"),
        &["--fanart", "--no-logo", "--no-background", "--no-cdart"],
    );
    let selected = FanartOptions::from_args(&parsed);

    assert!(selected.all);
    assert!(!selected.logo);
    assert!(selected.label_logo);
    assert!(selected.portrait);
    assert!(!selected.background);
    assert!(selected.banner);
    assert!(selected.album_cover);
    assert!(!selected.cdart);

    for (excluded, expected) in [
        ("--no-logo", [false, true, true, true, true, true, true]),
        (
            "--no-label-logo",
            [true, false, true, true, true, true, true],
        ),
        ("--no-portrait", [true, true, false, true, true, true, true]),
        (
            "--no-background",
            [true, true, true, false, true, true, true],
        ),
        ("--no-banner", [true, true, true, true, false, true, true]),
        (
            "--no-album-cover",
            [true, true, true, true, true, false, true],
        ),
        ("--no-cdart", [true, true, true, true, true, true, false]),
    ] {
        let parsed = args(std::path::Path::new("/tmp"), &["--fanart", excluded]);
        let selected = FanartOptions::from_args(&parsed);
        assert_eq!(
            [
                selected.logo,
                selected.label_logo,
                selected.portrait,
                selected.background,
                selected.banner,
                selected.album_cover,
                selected.cdart,
            ],
            expected,
            "{excluded} must disable only its own family"
        );
    }
}

/// The album in the reference library, as a release-group search answers.
const ONE_ALBUM: &str = r#"{"release-groups":[
    {"id":"c9fdb94c","score":100,"title":"Kind of Blue",
     "primary-type":"Album","first-release-date":"1959-08-17"}]}"#;

/// A search that found nothing — for the tests whose subject is the artist
/// half, where the album still has to be answered because the run asks.
const NO_ALBUM: &str = r#"{"release-groups":[]}"#;

const ONE_ARTIST: &str = r#"{"artists":[
    {"id":"561d854a","score":100,"name":"Miles Davis","type":"Person",
     "area":{"name":"United States"},
     "life-span":{"begin":"1926-05-26","end":"1991-09-28","ended":true}}]}"#;

#[test]
fn an_answer_is_stored_attributed_and_never_as_a_certainty() {
    let dir = sandbox("stored");
    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut transport).expect("a run");

    // One run, both halves: the artist, then the album. They are the same
    // question asked of the same service, and a library gets no report of what
    // its tags say until the albums have been asked about.
    assert_eq!(transport.asked.len(), 2, "asked: {:?}", transport.asked);
    // The name went into the query encoded, not raw.
    assert!(
        transport.asked[0].contains("query=Miles%20Davis"),
        "asked: {}",
        transport.asked[0]
    );
    assert!(
        transport.asked[1].contains("/release-group/"),
        "and the album followed: {}",
        transport.asked[1]
    );

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 2, "one artist, one album");
    let record = held
        .records
        .iter()
        .find(|r| matches!(r.facts, Facts::Artist(_)))
        .expect("the artist row");
    assert_eq!(record.source, sources::MUSICBRAINZ);
    assert_eq!(record.source_id.as_deref(), Some("561d854a"));
    assert!(
        !record.confidence.is_certain(),
        "a search never produces a certainty: {:?}",
        record.confidence
    );
    match &record.facts {
        Facts::Artist(a) => {
            assert_eq!(a.area.as_deref(), Some("United States"));
            assert_eq!(a.ended.as_deref(), Some("1991-09-28"));
        }
        other => panic!("expected artist facts, got {other:?}"),
    }

    // And the album row, which is the one with something to compare: the
    // catalog's date comes from the tags, `first_released` from MusicBrainz,
    // and a reissue is where they part company.
    let album = held
        .records
        .iter()
        .find(|r| matches!(r.facts, Facts::Release(_)))
        .expect("the album row");
    match &album.facts {
        Facts::Release(r) => {
            assert_eq!(r.primary_type.as_deref(), Some("Album"));
            assert_eq!(r.first_released.as_deref(), Some("1959-08-17"));
        }
        other => panic!("expected release facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_identifier_in_the_tags_is_used_instead_of_a_search() {
    // A library tagged with Picard already carries the MBID. Searching by
    // name there swaps a certainty for a guess, and asks a poorer question
    // besides: a search result is abbreviated, a lookup returns the
    // entity — which is why `ended` was missing and the confidence read
    // 95% for an artist nobody was unsure about.
    let dir = sandbox("bymbid");
    let mut tags = RawTags::default();
    tags.insert("artist", "Marilyn Manson");
    tags.insert("albumartist", "Marilyn Manson");
    tags.insert("album", "Antichrist Superstar");
    tags.insert("title", "The Beautiful People");
    tags.insert(
        "musicbrainz_artistid",
        "c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f",
    );
    let catalog = aede_core::model::builder::build(
        vec![ScannedFile {
            path: "/music/Manson/Antichrist/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(r#"{"id":"c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f",
            "name":"Marilyn Manson","type":"Group",
            "disambiguation":"US industrial metal band",
            "area":{"name":"United States"},
            "life-span":{"begin":"1989","ended":false}}"#
            .to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert!(
        transport.asked[0].contains("/artist/c98ff0e1"),
        "it looked the identifier up rather than searching: {}",
        transport.asked[0]
    );
    assert!(!transport.asked[0].contains("query="), "not a search");

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let record = &held.records[0];
    assert!(
        record.confidence.is_certain(),
        "asked by identifier, answered about that identifier: {:?}",
        record.confidence
    );
    match &record.facts {
        Facts::Artist(a) => {
            assert_eq!(a.active, Some(true), "the boolean a search leaves out");
            assert_eq!(
                a.disambiguation.as_deref(),
                Some("US industrial metal band")
            );
        }
        other => panic!("expected artist facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_ambiguous_answer_stores_nothing_at_all() {
    // The refusal has to reach the store, not only the screen: filing one
    // of two equally good answers is the mistake this design exists to
    // prevent, and a test that only read the output would not see it.
    let dir = sandbox("ambiguous");
    let mut transport = Canned {
        answers: vec![
            Ok(r#"{"artists":[
            {"id":"a","score":90,"name":"Someone"},
            {"id":"b","score":90,"name":"Someone Else"}]}"#
                .to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut transport).expect("a run");

    let held = sources::load(&sources::sources_path(&dir)).expect("readable");
    assert!(
        held.is_none_or(|h| h.records.is_empty()),
        "nothing was guessed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// No waiting in tests: the behaviour under test is the retry, not the sleep.
const NO_WAIT: [std::time::Duration; 3] = [std::time::Duration::ZERO; 3];

#[test]
fn a_hiccup_is_waited_out_rather_than_ending_the_run() {
    // What the first real run got wrong: request 5 of 402 came back 503,
    // the whole run stopped, and the message blamed a rate limit that had
    // not been exceeded. A transient refusal lets the next attempt
    // through, which is exactly what tells it apart from a ban.
    let dir = sandbox("hiccup");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::RateLimited),
            Ok(ONE_ARTIST.to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "the refused request, the one that worked, then the album"
    );
    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1, "and the answer was stored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_timeout_is_retried_once_before_being_reported() {
    // The one failure `ask_with_backoff` does not retry on its own: the
    // service was never reached at all. A single one of those is what a
    // MusicBrainz search occasionally does under load, and reporting it
    // immediately would describe a working service as broken.
    let dir = sandbox("timeout-once");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Ok(ONE_ARTIST.to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "the request that timed out, asked again, then the album"
    );
    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1, "the retry is what got stored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_timeout_on_the_same_artist_is_reported() {
    // One retry, not a second: a service that could not be reached twice in
    // a row is not a hiccup any more, and asking a third time would only
    // delay the same report.
    let dir = sandbox("timeout-twice");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "one try, one retry, then the album — the artist gave up after two"
    );
    let held = sources::load(&sources::sources_path(&dir)).expect("readable");
    assert!(
        held.is_none_or(|h| h.records.is_empty()),
        "nothing was stored for an artist never reached"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn being_told_to_slow_down_stops_the_run_and_keeps_what_was_stored() {
    // The one failure that must not be retried: MusicBrainz answers 503 to
    // *everything* once the rate is exceeded, so carrying on would turn a
    // slow run into a broken one for every program on this address.
    let dir = sandbox("ratelimit");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
        ],
        asked: Vec::new(),
    };
    let refused = run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect_err("it stops");
    assert_eq!(transport.asked.len(), 4, "it tried, then gave up");
    assert!(
        refused.to_string().contains("4 times in a row"),
        "and says it is a limit rather than a hiccup: {refused}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_run_asks_again_only_when_told_to() {
    let dir = sandbox("again");
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut first).expect("a run");

    // Nothing to ask: a second pass over a library costs what changed.
    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut second).expect("a run");
    assert!(second.asked.is_empty(), "asked: {:?}", second.asked);

    // Unless asked again explicitly.
    let mut third = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut third).expect("a run");
    assert_eq!(
        third.asked.len(),
        2,
        "both halves again, not just the artist"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_artist_refresh_keeps_the_browsed_discography() {
    // `fetch --discography` adds its result to the MusicBrainz artist row.
    // The ordinary artist endpoint never returns that list, so a later
    // `fetch --full` must merge the fresh facts into the row rather than
    // replacing the separately browsed discography with an empty vector.
    let dir = sandbox("keep_discography");
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("561d854a".to_string()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(sources::ArtistFacts {
            discography: vec![sources::KnownRelease {
                mbid: "missing-group".to_string(),
                title: "The Missing Album".to_string(),
                first_released: Some("1970".to_string()),
                primary_type: Some("Album".to_string()),
                secondary_types: Vec::new(),
            }],
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut transport).expect("a full refresh");

    let after = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let record = after
        .get(&entity, sources::MUSICBRAINZ)
        .expect("the refreshed artist");
    let Facts::Artist(facts) = &record.facts else {
        panic!("expected artist facts")
    };
    assert_eq!(facts.discography.len(), 1);
    assert_eq!(facts.discography[0].title, "The Missing Album");
    assert_eq!(facts.area.as_deref(), Some("United States"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dry_run_asks_nothing() {
    let dir = sandbox("dryrun");
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(&dir, &["--dry-run"]), &mut transport).expect("a run");
    assert!(transport.asked.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_you_corrected_by_hand_survives_a_fetch() {
    // The guarantee that makes hand-correction usable at all: records are
    // keyed on (entity, source), so a value filed under `manual` is not
    // something MusicBrainz can overwrite. Without it, correcting a wrong
    // answer would last until the next run and no further.
    let dir = sandbox("manual");
    let mut held = sources::Sources::default();
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: "manual".to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: aede_core::sources::Confidence::Identified,
        facts: Facts::Artist(aede_core::sources::ArtistFacts {
            area: Some("Alton, Illinois".to_string()),
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(NO_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut transport).expect("a run");

    let after = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(after.records.len(), 2, "two sources, two rows");
    let mine = after
        .get(&entity, "manual")
        .expect("what I wrote is still there");
    match &mine.facts {
        Facts::Artist(a) => assert_eq!(a.area.as_deref(), Some("Alton, Illinois")),
        other => panic!("expected artist facts, got {other:?}"),
    }
    assert!(
        after.get(&entity, sources::MUSICBRAINZ).is_some(),
        "and what was fetched sits beside it rather than on top"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_build_that_cannot_say_who_it_is_refuses_to_ask() {
    // What the first real run cost: an empty contact went out as
    // an empty contact address, MusicBrainz throttled it as anonymous, and the
    // 503 pointed at the rate limit rather than at the manifest.
    assert!(identity("0.2.0", "").is_err());
    assert!(identity("0.2.0", "   ").is_err());
    let refused = identity("0.2.0", "").expect_err("refused");
    assert!(
        refused.to_string().contains("repository.workspace"),
        "and names the fix: {refused}"
    );
    assert_eq!(
        identity("0.2.0", "https://example.org/aede").expect("a header"),
        "aede/0.2.0 ( https://example.org/aede )"
    );
}

#[test]
fn the_estimate_is_in_the_unit_the_formatter_expects() {
    // 403 artists printed "about 0 s": the count was multiplied into
    // seconds and handed to a formatter that takes milliseconds. The kind
    // of mistake that survives review and dies to one assertion.
    let ms = 403_u64 * aede_core::musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    let shown = ui::long_duration(ms);
    assert!(
        shown.contains("min"),
        "403 requests is minutes, not {shown}"
    );
}

#[test]
fn a_name_becomes_a_query_value_and_not_a_second_parameter() {
    // The one that matters: a `&` in a band's name would otherwise end the
    // query and start a parameter of its own, and the search would quietly
    // be about something else.
    assert_eq!(encode("Miles Davis"), "Miles%20Davis");
    assert_eq!(encode("Simon & Garfunkel"), "Simon%20%26%20Garfunkel");
    assert_eq!(encode("AC/DC"), "AC%2FDC");
    assert_eq!(encode("Björk"), "Bj%C3%B6rk", "UTF-8, byte by byte");
    assert_eq!(
        encode("a-b_c.d~e"),
        "a-b_c.d~e",
        "the unreserved set is kept"
    );
}

#[test]
fn two_passes_asked_for_together_both_run() {
    // The defect this pins: the passes were three `return`s in a row, so
    // `--covers --discography` ran the covers and dropped the discography
    // without a word. An option that cannot be honoured is refused everywhere
    // else in this program; this one could be honoured and was not.
    let dir = sandbox("two_passes");

    // First an ordinary fetch, so that both passes have something identified
    // to work from: a discography needs the artist's identifier, a cover needs
    // the album's.
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut first, &NO_WAIT).expect("the ordinary fetch");

    let mut both = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--covers", "--discography"]),
        &mut both,
        &NO_WAIT,
    )
    .expect("both passes run");

    // Each pass asks its own service, so what was asked says which ran. With
    // the old three-`return` dispatch only the second of these appeared.
    assert!(
        both.asked
            .iter()
            .any(|url| url.contains("release-group?artist=")),
        "the discography pass was skipped: {:?}",
        both.asked
    );
    assert!(
        both.asked
            .iter()
            .any(|url| url.contains("coverartarchive.org")),
        "the cover pass was skipped: {:?}",
        both.asked
    );
    // And in the order the passes are declared in, not the order typed.
    let discography = both
        .asked
        .iter()
        .position(|url| url.contains("release-group?artist="))
        .expect("asked");
    let covers = both
        .asked
        .iter()
        .position(|url| url.contains("coverartarchive.org"))
        .expect("asked");
    assert!(discography < covers, "{:?}", both.asked);
}

#[test]
fn the_passes_run_in_their_own_order_and_not_the_typed_one() {
    // Fixed rather than typed, because they go out from the artist: who they
    // are, what they recorded, what the records look like. Asking about the
    // albums first would be asking before the fetch that names them.
    let typed_one_way = Pass::asked_for(&args(
        std::path::Path::new("/tmp"),
        &["--covers", "--summaries", "--discography"],
    ));
    let typed_another = Pass::asked_for(&args(
        std::path::Path::new("/tmp"),
        &["--discography", "--covers", "--summaries"],
    ));
    assert_eq!(
        typed_one_way,
        vec![Pass::Summaries, Pass::Discography, Pass::Covers]
    );
    assert_eq!(
        typed_one_way, typed_another,
        "two orders of the same three options are one run"
    );

    assert_eq!(
        Pass::asked_for(&args(
            std::path::Path::new("/tmp"),
            &["--fanart", "--logos", "--banners"],
        )),
        vec![Pass::Logos],
        "the broad and legacy flags share one Fanart.tv metadata pass"
    );

    assert!(Pass::asked_for(&args(std::path::Path::new("/tmp"), &[])).is_empty());

    // And only the summaries pass can work without a catalog, which is what
    // decides whether one is loaded at all.
    assert!(!Pass::Summaries.needs_the_catalog());
    assert!(Pass::Discography.needs_the_catalog());
    assert!(Pass::Covers.needs_the_catalog());
}

#[test]
fn an_unusable_size_stops_the_run_before_any_pass_asks_anything() {
    // `--size` is read once for the whole run now rather than inside the
    // cover pass, and a value the archive does not generate must still be
    // refused before a summaries pass has spent ten minutes on the network.
    let dir = sandbox("bad_size");
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    let error = run_with(
        &args(&dir, &["--summaries", "--covers", "--size=800"]),
        &mut transport,
        &NO_WAIT,
    )
    .expect_err("800 is not a width the archive generates");
    assert!(error.to_string().contains("--size takes"), "{error}");
    assert!(
        transport.asked.is_empty(),
        "and nothing was asked before it was refused"
    );
}

#[test]
fn a_name_given_to_a_second_pass_narrows_it_instead_of_being_swallowed() {
    // The defect: `aede fetch --discography mika` ran over the whole library
    // and said nothing about the word it had ignored — an argument that could
    // not be honoured, dropped rather than refused, which is the one thing
    // this program is most careful about elsewhere.
    let dir = sandbox("named_pass");
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut first, &NO_WAIT).expect("the ordinary fetch");

    // A name nobody here answers to: nothing is asked at all.
    let mut nobody = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--discography", "mika"]),
        &mut nobody,
        &NO_WAIT,
    )
    .expect("the pass ran");
    assert!(
        nobody.asked.is_empty(),
        "a name reaching nobody must not browse the whole shelf: {:?}",
        nobody.asked
    );

    // And the name of somebody who is here does reach them.
    let mut somebody = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--discography", "miles"]),
        &mut somebody,
        &NO_WAIT,
    )
    .expect("the pass ran");
    assert!(
        somebody
            .asked
            .iter()
            .any(|url| url.contains("release-group?artist=")),
        "the name matched and the pass ran: {:?}",
        somebody.asked
    );
}

#[test]
fn a_name_reaches_an_album_by_its_title_or_by_its_artist() {
    // The rule the ordinary fetch already used, now shared by every pass:
    // `--covers manson` should find the records as well as the person.
    assert!(reaches(&[], &["anything"]), "no name means the whole shelf");
    assert!(reaches(&["miles".to_string()], &["Miles Davis", ""]));
    assert!(reaches(&["blue".to_string()], &["", "Kind of Blue"]));
    assert!(!reaches(
        &["mika".to_string()],
        &["Miles Davis", "Kind of Blue"]
    ));
    // Case and accents go through `normalize`, as everywhere else.
    assert!(reaches(&[text::normalize("MILES")], &["Miles Davis"]));
}

#[test]
fn nothing_matched_says_which_of_the_two_nothings_it_is() {
    // "No such artist here" and "that artist is already done" are different
    // problems with different next steps, and the general "run fetch first"
    // sends the second of them to re-run a pass with nothing to do.
    let names = vec!["mika".to_string()];
    assert!(nothing_named(&names, &EVERYTHING, 0).contains("nothing here matches mika"));
    let done = nothing_named(&names, &EVERYTHING, 3);
    assert!(done.contains("3 artists"), "{done}");
    assert!(done.contains("--full"), "{done}");
}

#[test]
fn a_lookup_asks_for_the_memberships_and_stores_them_dated() {
    // **An include nobody asks for is a field nobody has.** The parser can read
    // a line-up perfectly and still show nothing, for ever, because the request
    // never asked for the relations — which is a failure with no error message
    // anywhere. So the URL is asserted as well as the result.
    //
    // The answer is the one MusicBrainz really gave for Judas Priest, cut to
    // the fields this reads.
    let dir = sandbox("members");
    let mut tags = RawTags::default();
    tags.insert("artist", "Judas Priest");
    tags.insert("albumartist", "Judas Priest");
    tags.insert("album", "British Steel");
    tags.insert("title", "Breaking the Law");
    tags.insert(
        "musicbrainz_artistid",
        "6b335658-22c8-485d-93de-0bc29a1d0349",
    );
    let catalog = aede_core::model::builder::build(
        vec![ScannedFile {
            path: "/music/Priest/British Steel/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(r#"{"id":"6b335658-22c8-485d-93de-0bc29a1d0349",
            "name":"Judas Priest","type":"Group",
            "life-span":{"begin":"1969","ended":false},
            "relations":[
              {"type":"member of band","direction":"backward","target-type":"artist",
               "begin":"1969","end":"1970","ended":true,
               "attributes":["guitar family","original"],
               "artist":{"id":"3651334d-6513-40e1-adbb-423acf0ad3d6",
                         "name":"Ernie Chataway","type":"Person"}},
              {"type":"instrumental supporting musician","direction":"backward",
               "target-type":"artist","begin":"2018","end":null,"ended":false,
               "attributes":["electric guitar"],
               "artist":{"id":"2b9aed4d-769e-4c38-96e4-586ac59ce668",
                         "name":"Andy Sneap","type":"Person"}}]}"#
            .to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--artists"]), &mut transport, &NO_WAIT).expect("a run");

    assert!(
        transport.asked[0].contains("artist-rels"),
        "the request asked for the relations: {}",
        transport.asked[0]
    );

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let Facts::Artist(facts) = &held.records[0].facts else {
        panic!("expected artist facts");
    };
    assert_eq!(facts.line_up().count(), 2, "both relations name a player");
    assert!(facts.bands().next().is_none(), "and neither names a band");

    // The whole reason the dates are worth asking for: the band as it stood in
    // a given year. Ernie Chataway had left by 1980; Andy Sneap had not
    // arrived, so 1980 holds neither.
    let (in_1969, _) = facts.line_up_in(1969);
    assert_eq!(
        in_1969.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["Ernie Chataway"]
    );
    let (in_2020, _) = facts.line_up_in(2020);
    assert_eq!(
        in_2020.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["Andy Sneap"],
        "\"end\": null beside \"ended\": false is a musician still playing"
    );
}

#[test]
fn credits_fetches_recording_and_work_relationships_in_one_request() {
    let dir = sandbox("rich_credits");
    let mut tags = RawTags::default();
    tags.insert("artist", "Jimi Hendrix");
    tags.insert("albumartist", "Jimi Hendrix");
    tags.insert("album", "Electric Ladyland");
    tags.insert("title", "All Along the Watchtower");
    tags.insert("musicbrainz_recordingid", "recording-id");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Hendrix/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");
    let mut transport = Canned {
        answers: vec![Ok(r#"{
          "id":"recording-id","title":"All Along the Watchtower",
          "relations":[
            {"id":"performance","type":"instrument","type-id":"instrument-type",
             "target-type":"artist","target-credit":"Jimi Hendrix",
             "attributes":["guitar"],
             "artist":{"id":"hendrix-id","name":"Jimi Hendrix"}},
            {"id":"work-link","type":"performance","target-type":"work",
             "work":{"id":"work-id","title":"All Along the Watchtower",
               "relations":[
                 {"id":"composer","type":"composer","type-id":"composer-type",
                  "target-type":"artist","target-credit":"Robert Dylan",
                  "attributes":[],
                  "artist":{"id":"dylan-id","name":"Bob Dylan"}}
               ]}}
          ]}"#
        .to_string())],
        asked: Vec::new(),
    };

    run_with(&args(&dir, &["--credits"]), &mut transport, &NO_WAIT).expect("fetch credits");
    assert_eq!(transport.asked.len(), 1);
    assert!(transport.asked[0].contains("artist-rels+work-rels+work-level-rels"));

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("stored");
    let Facts::Track(facts) = &held.records[0].facts else {
        panic!("track facts");
    };
    assert!(facts.relationships_complete);
    assert_eq!(facts.credits[0].role, "instrument");
    assert_eq!(facts.works[0].credits[0].role, "composer");

    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut second, &NO_WAIT).expect("already complete");
    assert!(
        second.asked.is_empty(),
        "a complete relationship lookup is not repeated"
    );
}

/// Two shelves on the disk, one folder each, so that a folder can be given.
///
/// **Real files**, unlike the reference library above: a folder is read off
/// the filesystem before it is looked up in the catalog, and
/// [`super::Scope::of`] refuses a path that is not there — which is what stops
/// a mistyped folder becoming a silent run over everything.
///
/// **And canonical**, which is not a detail: `std::env::temp_dir()` is
/// `/var/folders/…` on macOS and `/var` is a link to `/private/var`, so a
/// catalog built from the path as handed out is filed under a spelling
/// [`super::super::canonical`] never produces. `aede scan` canonicalizes its
/// roots, so a real catalog never holds that spelling — a fixture that did
/// would be testing a library the program cannot build, and it failed on macOS
/// while passing on Linux, where nothing is a link. See `docs/design/paths.md`.
fn two_shelves(what: &str) -> (aede_core::model::Catalog, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("aede_scope_{}_{what}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let dir = super::super::canonical(&dir);
    let mut files = Vec::new();
    for (artist, album) in [("Alastis", "Revenge"), ("Ozzy Osbourne", "Blizzard of Ozz")] {
        let folder = dir.join(artist);
        std::fs::create_dir_all(&folder).expect("a folder");
        let path = folder.join("01.flac");
        std::fs::write(&path, b"not really audio").expect("a file");
        let mut tags = RawTags::default();
        tags.insert("artist", artist);
        tags.insert("albumartist", artist);
        tags.insert("album", album);
        // Titled after the shelf, so that a track can be told from the other.
        tags.insert("title", artist);
        files.push(ScannedFile {
            path: path.to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let catalog = build(files, vec![dir.to_string_lossy().to_string()], 1, &[]);
    (catalog, dir)
}

#[test]
fn a_positional_that_names_a_folder_is_read_as_one_rather_than_as_a_name() {
    // `aede fetch --lyrics ~/Music/Alastis` used to normalise the whole path
    // into words, match none of them against a title or an artist, and ask
    // about the **entire library** — the swallowed argument this program
    // refuses everywhere else, wearing a slash.
    let (_, dir) = two_shelves("split");
    let shelf = dir.join("Alastis");
    let args = Args::parse(vec![
        "fetch".to_string(),
        shelf.to_string_lossy().to_string(),
        "portishead".to_string(),
    ]);

    let (folders, names) = folders_and_names(&args);
    assert_eq!(folders, vec![shelf.to_string_lossy().to_string()]);
    assert_eq!(
        names,
        vec!["portishead".to_string()],
        "a word that is not on the disk is still a name"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "catalog paths are `/`-separated; see docs/design/paths.md"
)]
fn a_folder_reaches_the_tracks_the_album_and_the_artist_under_it() {
    // One walk of the catalog answers for every pass, so what a folder means
    // cannot differ between them.
    let (catalog, dir) = two_shelves("reach");
    let scope = Scope::of(
        &catalog,
        &[dir.join("Alastis").to_string_lossy().to_string()],
    )
    .expect("a folder the catalog holds");

    let artist = |name: &str| {
        catalog
            .artists
            .iter()
            .find(|a| a.name == name)
            .expect("the artist")
    };
    assert!(scope.has_artist(&artist("Alastis").key));
    assert!(!scope.has_artist(&artist("Ozzy Osbourne").key));

    let release = |title: &str| {
        catalog
            .releases
            .iter()
            .find(|r| r.title == title)
            .expect("the record")
    };
    assert!(scope.has_release(release("Revenge").id));
    assert!(!scope.has_release(release("Blizzard of Ozz").id));

    let track = |title: &str| {
        catalog
            .tracks
            .iter()
            .find(|t| t.title == title)
            .expect("the track")
    };
    assert!(scope.has_track(track("Alastis").id));
    assert!(!scope.has_track(track("Ozzy Osbourne").id));
}

#[test]
fn no_folder_at_all_reaches_everything() {
    // The distinction the `Option` inside the scope exists for: "no folder was
    // given" reaches the whole library, "a folder that holds nothing" reaches
    // nothing — and reading the first as the second would turn every ordinary
    // fetch into a run with nothing to do.
    let (catalog, _) = two_shelves("everything");
    let scope = Scope::of(&catalog, &[]).expect("no folder is not a refusal");
    assert!(scope.is_empty());
    for artist in &catalog.artists {
        assert!(scope.has_artist(&artist.key));
    }
    for release in &catalog.releases {
        assert!(scope.has_release(release.id));
    }
}

#[test]
fn a_folder_the_catalog_has_never_seen_is_refused_rather_than_run_over() {
    // The same refusal `check` and `playlist` make, and for the same reason: a
    // folder added since the last scan would otherwise produce a cheerful
    // "nothing to ask about" that reads as *your library is done*.
    let (catalog, dir) = two_shelves("unknown");
    let outside = dir.join("Portishead");
    std::fs::create_dir_all(&outside).expect("a folder");

    let refused = Scope::of(&catalog, &[outside.to_string_lossy().to_string()]);
    assert!(refused.is_err(), "a folder no file of the catalog is under");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "catalog paths are `/`-separated; see docs/design/paths.md"
)]
fn the_album_half_of_an_ordinary_fetch_honours_a_folder() {
    // Both halves of the ordinary run, not only the artists: `aede fetch
    // ~/Music/Alastis` that asked about every album of the library would be
    // answering a question nobody typed.
    let (catalog, dir) = two_shelves("albums");
    let scope = Scope::of(
        &catalog,
        &[dir.join("Alastis").to_string_lossy().to_string()],
    )
    .expect("a folder the catalog holds");
    let held = sources::Sources::default();

    let narrowed = crate::commands::releases::targets(&catalog, &held, &[], &scope, false);
    assert_eq!(
        crate::commands::releases::names(&narrowed).collect::<Vec<_>>(),
        vec!["Revenge"]
    );
    let whole = crate::commands::releases::targets(&catalog, &held, &[], &EVERYTHING, false);
    assert_eq!(whole.len(), 2, "and without a folder, both records");
}
