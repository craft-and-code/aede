//! Tests for [`super`], split out of `acoustid.rs`.
//!
//! The fixture below follows the shape published on
//! <https://acoustid.org/webservice>, read rather than assumed — the service
//! is unreachable from where this was written, and a fixture agreeing with
//! whoever wrote it proves nothing about a live answer. That limitation is
//! stated here rather than discovered later.

use super::*;
use crate::json::parse;

fn json(text: &str) -> Json {
    parse(text).expect("the fixture is valid JSON")
}

/// Two results, the better one second, each with a recording.
const ANSWER: &str = r#"{
  "status": "ok",
  "results": [
    { "id": "9ff43b6a", "score": 0.71,
      "recordings": [
        { "id": "b1a9c0de", "title": "So What (live)", "duration": 545,
          "artists": [{ "id": "561d854a", "name": "Miles Davis" }] } ] },
    { "id": "5c1b3f77", "score": 0.98,
      "recordings": [
        { "id": "a3e4f5c6", "title": "So What", "duration": 545,
          "artists": [{ "id": "561d854a", "name": "Miles Davis" },
                      { "id": "0f1e2d3c", "name": "John Coltrane" }],
          "releasegroups": [{ "id": "c9fdb94c", "title": "Kind of Blue",
                              "type": "Album" }] } ] }
  ]
}"#;

#[test]
fn the_best_score_wins_whatever_order_the_answer_came_in() {
    let heard = best(&json(ANSWER)).expect("a match");
    assert_eq!(heard.recording, "a3e4f5c6");
    assert_eq!(heard.score, 0.98);
    assert_eq!(heard.title.as_deref(), Some("So What"));
    assert_eq!(heard.artists, vec!["Miles Davis", "John Coltrane"]);
    assert_eq!(heard.album.as_deref(), Some("Kind of Blue"));
}

#[test]
fn a_result_with_no_recording_is_not_an_answer() {
    // The service replies with its own track identifiers even when it knows
    // nothing else about them. A caller taking the first result would file a
    // record naming a track nobody can look up — true, useless, and
    // indistinguishable from a real identification at a glance.
    let bare = json(r#"{"status":"ok","results":[{"id":"9ff43b6a","score":1.0}]}"#);
    assert_eq!(best(&bare), None);

    // Nor is a recording with no identifier.
    let nameless = json(
        r#"{"status":"ok","results":[{"id":"x","score":1.0,
             "recordings":[{"title":"So What"}]}]}"#,
    );
    assert_eq!(best(&nameless), None);

    // And a service that found nothing answers with an empty list, which is
    // an answer — "asked, and it does not know" — not a failure.
    let nothing = json(r#"{"status":"ok","results":[]}"#);
    assert_eq!(best(&nothing), None);
    assert_eq!(refused(&nothing), None);
}

#[test]
fn two_runs_over_one_unchanged_file_answer_the_same_thing() {
    // Ties broken by identifier: a match that changes between runs cannot be
    // compared with the one stored last time, which is the whole point of
    // storing it.
    let tied = json(
        r#"{"status":"ok","results":[
             {"id":"x","score":0.9,"recordings":[{"id":"ffff","title":"B"}]},
             {"id":"y","score":0.9,"recordings":[{"id":"aaaa","title":"A"}]}]}"#,
    );
    assert_eq!(best(&tied).expect("a match").recording, "aaaa");
    assert_eq!(best(&tied), best(&tied));
}

#[test]
fn an_error_answered_with_two_hundred_is_still_an_error() {
    // A bad key answers `200 OK` with `"status": "error"`. A client reading
    // only the HTTP code would file "this file is unknown" for every file in
    // the library, and the reader would conclude their music is unidentifiable.
    let bad = json(r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#);
    assert_eq!(refused(&bad).as_deref(), Some("invalid API key"));
    assert_eq!(best(&bad), None);

    // An answer with no status at all is not trusted either.
    assert!(refused(&json(r#"{"results":[]}"#)).is_some());
    assert_eq!(refused(&json(r#"{"status":"ok","results":[]}"#)), None);
}

#[test]
fn a_fingerprint_survives_the_query_string_it_is_put_into() {
    // Base64 carries `+` and `/`, and both mean something else in a query
    // string: a fingerprint sent raw comes back "not found" for a file the
    // service knows perfectly well.
    let url = lookup_url("KEY123", "AQAA+bc/de=", 183);
    assert!(url.starts_with(WEB_SERVICE));
    assert!(url.contains("client=KEY123"));
    assert!(url.contains("duration=183"));
    assert!(
        url.contains("fingerprint=AQAA%2Bbc%2Fde%3D"),
        "the fingerprint must be escaped: {url}"
    );
    assert!(!url.contains("AQAA+bc/de="), "and not sent raw: {url}");

    // Without `meta` the answer carries the service's own identifiers and
    // nothing else — true, and useless for naming a file.
    // The `+` between the two values is left as the service's documentation
    // writes it: escaping it would be a guess made against something that
    // cannot be tried from here.
    assert!(url.contains("meta=recordings+releasegroups"), "{url}");
}

#[test]
fn the_message_for_a_missing_key_explains_rather_than_orders() {
    let said = no_key();
    assert!(said.contains(KEY_VARIABLE));
    assert!(said.contains("acoustid.org/new-application"), "{said}");
    // Why there is no key in the program, because a reader told only to set a
    // variable has been handed a chore with no reason behind it.
    assert!(said.contains("every\ncopy shares"), "{said}");
    assert!(said.contains("Nothing else in Aède needs it"));
}
