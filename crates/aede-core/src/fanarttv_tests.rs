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

/// Two logos at each resolution, the more liked one first in the HD array —
/// the sort must not be mistaken for "as given", and a standard logo with far
/// more likes must still lose to any HD one.
const LOGO_ANSWER: &str = r#"{
  "name": "Ozzy Osbourne",
  "hdmusiclogo": [
    { "id": "1", "url": "https://assets.fanart.tv/fanart/music/ozzy/hd-logo-a.png",
      "likes": "4" },
    { "id": "2", "url": "https://assets.fanart.tv/fanart/music/ozzy/hd-logo-b.png",
      "likes": "9" }
  ],
  "musiclogo": [
    { "id": "3", "url": "https://assets.fanart.tv/fanart/music/ozzy/logo.png",
      "likes": "500" }
  ],
  "artistthumb": [
    { "id": "4", "url": "https://assets.fanart.tv/fanart/music/ozzy/thumb.jpg",
      "likes": "1" }
  ]
}"#;

#[test]
fn an_hd_logo_wins_over_a_standard_one_however_many_more_likes_it_has() {
    // musiclogo's 500 likes is not a competitor to hdmusiclogo's 9 — the two
    // fields are the same submissions at two resolutions, and HD is always
    // preferred when any exists.
    assert_eq!(
        logo_url(&json(LOGO_ANSWER)).as_deref(),
        Some("https://assets.fanart.tv/fanart/music/ozzy/hd-logo-b.png")
    );
}

#[test]
fn the_most_liked_hd_logo_wins_whatever_order_the_answer_gave_them() {
    // Isolated to hdmusiclogo alone, so this locks in the sort itself rather
    // than the HD-over-standard preference the previous test already covers.
    let hd_only = json(
        r#"{"hdmusiclogo":[
            {"id":"1","url":"https://x/hd-a.png","likes":"4"},
            {"id":"2","url":"https://x/hd-b.png","likes":"9"}
        ]}"#,
    );
    assert_eq!(logo_url(&hd_only).as_deref(), Some("https://x/hd-b.png"));
}

#[test]
fn a_standard_logo_is_used_only_when_no_hd_logo_exists() {
    let standard_only =
        json(r#"{"musiclogo":[{"id":"1","url":"https://x/logo.png","likes":"2"}]}"#);
    assert_eq!(
        logo_url(&standard_only).as_deref(),
        Some("https://x/logo.png")
    );
}

#[test]
fn a_thumb_or_background_is_never_answered_as_a_logo() {
    // Symmetric to `a_background_or_a_logo_is_never_answered_as_a_portrait`:
    // an artist with a portrait but no logo has, for this question, nothing.
    assert_eq!(logo_url(&json(ANSWER)), None);
}

#[test]
fn no_logo_at_all_is_none_not_an_error() {
    assert_eq!(logo_url(&json(r#"{"name":"Nobody"}"#)), None);
}

#[test]
fn a_logo_with_no_address_is_skipped_rather_than_answered_as_blank() {
    let blank_url = json(
        r#"{"hdmusiclogo":[
            {"id":"1","url":"","likes":"50"},
            {"id":"2","url":"https://x/ok.png","likes":"1"}
        ]}"#,
    );
    assert_eq!(logo_url(&blank_url).as_deref(), Some("https://x/ok.png"));
}

/// Two banners, the more liked one first in the answer — the same sort as
/// every other kind here.
const BANNER_ANSWER: &str = r#"{
  "name": "Ozzy Osbourne",
  "musicbanner": [
    { "id": "1", "url": "https://assets.fanart.tv/fanart/music/ozzy/banner-a.jpg",
      "likes": "3" },
    { "id": "2", "url": "https://assets.fanart.tv/fanart/music/ozzy/banner-b.jpg",
      "likes": "8" }
  ],
  "hdmusiclogo": [
    { "id": "3", "url": "https://assets.fanart.tv/fanart/music/ozzy/hd-logo.png",
      "likes": "1" }
  ]
}"#;

#[test]
fn the_most_liked_banner_wins_whatever_order_the_answer_gave_them() {
    assert_eq!(
        banner_url(&json(BANNER_ANSWER)).as_deref(),
        Some("https://assets.fanart.tv/fanart/music/ozzy/banner-b.jpg")
    );
}

#[test]
fn a_logo_or_a_thumb_is_never_answered_as_a_banner() {
    assert_eq!(banner_url(&json(ANSWER)), None);
    assert_eq!(banner_url(&json(LOGO_ANSWER)), None);
}

#[test]
fn no_banner_at_all_is_none_not_an_error() {
    assert_eq!(banner_url(&json(r#"{"name":"Nobody"}"#)), None);
}

#[test]
fn a_banner_with_no_address_is_skipped_rather_than_answered_as_blank() {
    let blank_url = json(
        r#"{"musicbanner":[
            {"id":"1","url":"","likes":"50"},
            {"id":"2","url":"https://x/ok.jpg","likes":"1"}
        ]}"#,
    );
    assert_eq!(banner_url(&blank_url).as_deref(), Some("https://x/ok.jpg"));
}

#[test]
fn v32_addresses_artist_and_label_by_their_musicbrainz_identifiers() {
    assert_eq!(
        lookup_url("artist-id", "key"),
        "https://webservice.fanart.tv/v3.2/music/artist-id?api_key=key"
    );
    assert_eq!(
        label_lookup_url("label-id", "key"),
        "https://webservice.fanart.tv/v3.2/music/labels/label-id?api_key=key"
    );
}

#[test]
fn an_unidentified_response_is_not_silently_read_as_no_artwork() {
    assert!(artist_response(&json(r#"{"mbid_id":"artist-id"}"#), "artist-id").is_ok());
    assert!(artist_response(&json(r#"{}"#), "artist-id").is_err());
    assert!(label_response(&json(r#"{"id":"label-id"}"#), "label-id").is_ok());
    assert!(label_response(&json(r#"{}"#), "label-id").is_err());
}

#[test]
fn a_label_logo_comes_only_from_the_musiclabel_field() {
    let response = json(
        r#"{"musiclabel":[
          {"url":"https://x/plain.png","likes":"2"},
          {"url":"https://x/best.png","likes":"9"}
        ],"hdmusiclogo":[{"url":"https://x/artist.png","likes":"99"}]}"#,
    );
    assert_eq!(
        label_logo_url(&response).as_deref(),
        Some("https://x/best.png")
    );
}
