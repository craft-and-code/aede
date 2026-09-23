use super::*;
use crate::args::Args;
use aede_core::model::{CreditAttribute, ScannedFile, build};
use aede_core::sources::{Confidence, CreditLink, WorkLink};
use aede_core::tags::RawTags;

fn expression(words: &[&str]) -> String {
    track_query(&Args::parse(words.iter().map(|w| w.to_string())))
}

#[test]
fn asking_for_a_track_by_an_artist_accepts_either_reading() {
    // A track "by Miles Davis" should be found on a Miles Davis album
    // whether or not he is credited on that particular piece. That is an
    // OR — the one thing a pile of options could never say, and the reason
    // this mapping is a decision rather than a transcription.
    assert_eq!(
        expression(&["track", "So What", "--artist", "Miles Davis"]),
        "(artist:\"Miles Davis\" OR albumartist:\"Miles Davis\")"
    );
}

#[test]
fn each_filter_becomes_one_term_and_a_name_keeps_its_spaces() {
    assert_eq!(
        expression(&["track", "x", "--album", "Kind of Blue"]),
        "album:\"Kind of Blue\""
    );
    assert_eq!(
        expression(&["track", "x", "--comment", "vinyl"]),
        "comment:vinyl"
    );
    assert_eq!(
        expression(&["track", "x"]),
        "",
        "no filter is no expression"
    );
    assert_eq!(
        expression(&["track", "x", "--album", "Legion", "--comment", "rip"]),
        "album:Legion comment:rip"
    );
}

#[test]
fn json_keeps_every_rich_credit_detail_and_its_scope() {
    let mut tags = RawTags::default();
    tags.insert("artist", "Band");
    tags.insert("albumartist", "Band");
    tags.insert("album", "Record");
    tags.insert("title", "Song");
    tags.insert("performer:guitar", "Local Player");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/song.flac".into(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".into()],
        1,
        &[],
    );
    let track = &catalog.tracks[0];
    let sourced = vec![SourcedCreditLink {
        recording_id: track.recording_id,
        work: Some(WorkLink {
            mbid: "work-id".into(),
            title: "Song Work".into(),
            ..Default::default()
        }),
        credit: CreditLink {
            relation_id: Some("relation-id".into()),
            role_id: Some("role-id".into()),
            role: "composer".into(),
            direction: Some("backward".into()),
            artist_mbid: "artist-id".into(),
            artist_name: "Canonical Writer".into(),
            credited_as: Some("The Writer".into()),
            attributes: vec![CreditAttribute {
                id: Some("attribute-id".into()),
                name: "additional".into(),
                value: Some("yes".into()),
                credited_as: Some("additional lyrics".into()),
            }],
            began: Some("1970".into()),
            ended: Some("1971".into()),
            over: Some(true),
            order: Some(2),
        },
        source: "musicbrainz".into(),
        confidence: Confidence::Identified,
        review: None,
        trusted: true,
        fetched_at: 42,
    }];

    let json = as_json(&catalog, track, &sourced);
    let local = json
        .get("credits")
        .and_then(Json::as_arr)
        .expect("local credits")
        .iter()
        .find(|credit| credit.field_str("role").as_deref() == Some("performer"))
        .expect("performer credit");
    let local_attribute = local
        .get("attributes")
        .and_then(Json::as_arr)
        .and_then(|attributes| attributes.first())
        .expect("instrument attribute");
    assert_eq!(local_attribute.field_str("name").as_deref(), Some("guitar"));
    assert_eq!(local.field_str("source").as_deref(), Some("tags"));

    let external = json
        .get("sourced_credits")
        .and_then(Json::as_arr)
        .and_then(|credits| credits.first())
        .expect("sourced credit");
    assert_eq!(external.field_str("scope").as_deref(), Some("work"));
    assert_eq!(external.field_str("work_mbid").as_deref(), Some("work-id"));
    assert_eq!(
        external.field_str("relation_id").as_deref(),
        Some("relation-id")
    );
    assert_eq!(
        external.field_str("credited_as").as_deref(),
        Some("The Writer")
    );
    assert_eq!(external.field_u32("order"), Some(2));
    assert_eq!(
        external.field_str("confidence").as_deref(),
        Some("identified")
    );
}
