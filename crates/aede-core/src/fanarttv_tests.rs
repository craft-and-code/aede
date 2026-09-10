//! Tests for [`super`], split out of `fanarttv.rs`.
//!
//! The fixture follows the shape published on
//! <https://fanart.tv/api-docs/api-v3>, read rather than assumed — the same
//! limitation [`crate::acoustid_tests`] states about its own fixture.

use super::*;
use crate::json::parse;

fn json(text: &str) -> Json {
    parse(text).expect("the fixture is valid JSON")
}

/// Two thumbs, the more liked one first in the answer — the sort must not be
/// mistaken for "as given".
const ANSWER: &str = r#"{
  "name": "Ozzy Osbourne",
  "mbid_id": "8aa5b65a-5b3c-4029-92bf-47a544356934",
  "artistthumb": [
    { "id": "1", "url": "https://assets.fanart.tv/fanart/music/ozzy/thumb-a.jpg",
      "likes": "12" },
    { "id": "2", "url": "https://assets.fanart.tv/fanart/music/ozzy/thumb-b.jpg",
      "likes": "31" }
  ],
  "artistbackground": [
    { "id": "3", "url": "https://assets.fanart.tv/fanart/music/ozzy/bg.jpg",
      "likes": "99" }
  ]
}"#;

#[test]
fn the_most_liked_thumb_wins_whatever_order_the_answer_gave_them() {
    assert_eq!(
        portrait_url(&json(ANSWER)).as_deref(),
        Some("https://assets.fanart.tv/fanart/music/ozzy/thumb-b.jpg")
    );
}

#[test]
fn a_background_or_a_logo_is_never_answered_as_a_portrait() {
    // Only `artistthumb` names a portrait. An artist with backgrounds, logos
    // or banners but no thumb has, for this question, nothing — reusing a
    // backdrop as a portrait would answer a question nobody asked.
    let backdrop_only =
        json(r#"{"artistbackground":[{"id":"1","url":"https://x/bg.jpg","likes":"5"}]}"#);
    assert_eq!(portrait_url(&backdrop_only), None);
}

#[test]
fn no_thumb_at_all_is_none_not_an_error() {
    assert_eq!(portrait_url(&json(r#"{"name":"Nobody"}"#)), None);
}

#[test]
fn a_thumb_with_no_address_is_skipped_rather_than_answered_as_blank() {
    let blank_url = json(
        r#"{"artistthumb":[
            {"id":"1","url":"","likes":"50"},
            {"id":"2","url":"https://x/ok.jpg","likes":"1"}
        ]}"#,
    );
    assert_eq!(
        portrait_url(&blank_url).as_deref(),
        Some("https://x/ok.jpg")
    );
}
