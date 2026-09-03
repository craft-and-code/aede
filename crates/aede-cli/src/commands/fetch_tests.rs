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
        }],
        vec!["/music".to_string()],
        1,
    );
    std::fs::create_dir_all(dir).expect("a data folder");
    let path = aede_core::store::catalog_path(dir);
    aede_core::store::save(&catalog, &path).expect("a catalog");
    path
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_fetch_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    library(&dir);
    dir
}

fn args(dir: &std::path::Path, extra: &[&str]) -> Args {
    let mut raw = vec!["fetch".to_string(), format!("--data={}", dir.display())];
    raw.extend(extra.iter().map(|s| s.to_string()));
    Args::parse(raw)
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
        }],
        vec!["/music".to_string()],
        1,
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
    // `aede/0.1.0 (  )`, MusicBrainz throttled it as anonymous, and the
    // 503 pointed at the rate limit rather than at the manifest.
    assert!(identity("0.1.0", "").is_err());
    assert!(identity("0.1.0", "   ").is_err());
    let refused = identity("0.1.0", "").expect_err("refused");
    assert!(
        refused.to_string().contains("repository.workspace"),
        "and names the fix: {refused}"
    );
    assert_eq!(
        identity("0.1.0", "https://example.org/aede").expect("a header"),
        "aede/0.1.0 ( https://example.org/aede )"
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
    assert!(nothing_named(&names, 0).contains("nothing here matches mika"));
    let done = nothing_named(&names, 3);
    assert!(done.contains("3 artists"), "{done}");
    assert!(done.contains("--full"), "{done}");
}
