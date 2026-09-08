//! Tests for [`super`], split out of `lrclib.rs`.

use super::*;
use crate::json::parse;

#[test]
fn the_address_carries_every_criterion_encoded() {
    // This is the address that was actually sent, and answered.
    let url = get_url("Ozzy Osbourne", "Crazy Train", "Blizzard of Ozz", 296);
    assert!(url.starts_with("https://lrclib.net/api/get?"), "{url}");
    assert!(url.contains("artist_name=Ozzy%20Osbourne"), "{url}");
    assert!(url.contains("track_name=Crazy%20Train"), "{url}");
    assert!(url.contains("album_name=Blizzard%20of%20Ozz"), "{url}");
    assert!(url.contains("duration=296"), "{url}");
}

#[test]
fn an_album_nobody_named_is_left_out_rather_than_sent_empty() {
    // Two different questions. An omitted parameter asks with one criterion
    // fewer — which a live answer showed works — where `album_name=` states
    // that the album is called nothing, which nobody has checked.
    let url = get_url("Ozzy Osbourne", "Crazy Train", "   ", 296);
    assert!(!url.contains("album_name"), "{url}");
    assert!(url.contains("track_name=Crazy%20Train"), "{url}");
    assert!(url.contains("duration=296"), "{url}");
}

#[test]
fn a_name_that_would_break_the_query_is_encoded_and_not_dropped() {
    // Real titles carry `&`, `?` and `#`, and a value pasted in raw would end
    // the parameter early — which does not fail, it asks a different question
    // and gets a confident wrong answer.
    let url = get_url("AC/DC", "Rock & Roll #1", "Q?", 1);
    assert!(url.contains("artist_name=AC%2FDC"), "{url}");
    assert!(url.contains("track_name=Rock%20%26%20Roll%20%231"), "{url}");
    assert!(url.contains("album_name=Q%3F"), "{url}");
    // Accented names survive as UTF-8 bytes rather than being folded away:
    // this is a lookup key, not a matching key.
    assert!(get_url("Björk", "x", "y", 1).contains("Bj%C3%B6rk"));
}

#[test]
fn the_timed_form_wins_where_there_is_one() {
    // The richer of the two, and the difference cannot be recovered: the
    // parser reads timings into lines that carry them, and a reader who only
    // wants the words gets them from the same lines with the timings dropped.
    let answer = parse(r#"{"plainLyrics":"All aboard","syncedLyrics":"[00:12.00]All aboard"}"#)
        .expect("valid JSON");
    assert_eq!(
        read(&answer),
        Found::Words(Words {
            text: "[00:12.00]All aboard".to_string(),
            synced: true,
        })
    );
}

#[test]
fn plain_words_are_words() {
    let answer = parse(r#"{"plainLyrics":"All aboard","syncedLyrics":null}"#).expect("valid JSON");
    assert_eq!(
        read(&answer),
        Found::Words(Words {
            text: "All aboard".to_string(),
            synced: false,
        })
    );
}

#[test]
fn an_instrumental_is_an_answer_and_not_an_absence() {
    // The distinction is the reason this is a variant of its own. An
    // instrumental is a finished question; telling a reader "no lyrics found"
    // about one would send them looking for a fault that is not there.
    let answer =
        parse(r#"{"instrumental":true,"plainLyrics":"","syncedLyrics":null}"#).expect("valid JSON");
    assert_eq!(read(&answer), Found::Instrumental);
}

#[test]
fn a_record_with_no_words_in_it_is_nothing() {
    // Both fields present and empty is not an answer about the words, and
    // writing an empty `.lrc` for it would put a file on somebody's disk that
    // says a song has no words when nobody said that.
    for empty in [
        r#"{"plainLyrics":"","syncedLyrics":""}"#,
        r#"{"plainLyrics":null,"syncedLyrics":null}"#,
        r#"{"plainLyrics":"   ","syncedLyrics":"\n"}"#,
        r#"{}"#,
    ] {
        assert_eq!(
            read(&parse(empty).expect("valid JSON")),
            Found::Nothing,
            "{empty}"
        );
    }
}

/// The answer LRCLIB really gave for `Crazy Train`, cut to the fields this
/// reads and to the first lines of each.
///
/// Kept whole in shape rather than invented: the timestamps carry a space after
/// the bracket, the blank lines are timed too, and `instrumental` is present and
/// `false` rather than absent. Every one of those is something a parser written
/// from the documentation would have guessed at.
const CRAZY_TRAIN: &str = r#"{
  "id": 3792909,
  "name": "Crazy Train",
  "trackName": "Crazy Train",
  "artistName": "Ozzy Osbourne",
  "albumName": "Blizzard of Ozz",
  "duration": 296.0,
  "instrumental": false,
  "plainLyrics": "All aboard! Ha ha ha ha ha ha haaaa!\nAy, ay, ay, ay, ay, ay, ay...\n\nCrazy, but that's how it goes",
  "syncedLyrics": "[00:00.15] All aboard! Ha ha ha ha ha ha haaaa!\n[00:07.73] Ay, ay, ay, ay, ay, ay, ay...\n[00:12.42] \n[00:38.87] Crazy, but that's how it goes"
}"#;

/// And the answer for a track it has never heard of: `404`, with a body.
///
/// The body is why the status is what decides. A reader looking only at the
/// JSON would see a perfectly well-formed document with no lyrics field in it
/// and could not tell that from a service having a bad day.
const NOT_FOUND: &str =
    r#"{"message":"Failed to find specified track","name":"TrackNotFound","statusCode":404}"#;

#[test]
fn the_answer_that_came_back_reads_as_timed_words() {
    let found = read(&parse(CRAZY_TRAIN).expect("valid JSON"));
    let Found::Words(words) = found else {
        panic!("expected words, got {found:?}");
    };
    assert!(words.synced, "the timed form is the one to keep");
    assert!(
        words.text.starts_with("[00:00.15] All aboard!"),
        "{}",
        words.text
    );

    // And it goes straight through this program's own reader, which is the
    // whole point of writing it to disk in that form.
    let lines = crate::lyrics::parse(&words.text);
    assert_eq!(lines[0].at_ms, Some(150));
    assert_eq!(lines[0].text, "All aboard! Ha ha ha ha ha ha haaaa!");
    // A timed line with nothing in it is a section break, and the service
    // writes them: kept, because a player following the timings needs to know
    // when the singing stops.
    assert_eq!(lines[2].at_ms, Some(12_420));
    assert_eq!(lines[2].text, "");
}

#[test]
fn instrumental_false_is_not_instrumental() {
    // Present and `false` on every ordinary answer, which is why the check is
    // for `Some(true)` and not for the field being there.
    assert!(matches!(
        read(&parse(CRAZY_TRAIN).expect("valid JSON")),
        Found::Words(_)
    ));
}

#[test]
fn a_track_the_service_never_heard_of_answers_a_document_with_no_words_in_it() {
    // The status is what says "not found" — this body is well-formed JSON and
    // reads, on its own, exactly like a service having a bad day. Reading it
    // as an answer about the words gives `Nothing`, which is right, and the
    // transport's `404` is what makes the run count it as a miss rather than
    // as a failure.
    assert_eq!(read(&parse(NOT_FOUND).expect("valid JSON")), Found::Nothing);
}
