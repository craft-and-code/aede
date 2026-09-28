use super::*;
use crate::model::{Artist, AudioFile, Recording, Track};
use crate::sources::{
    Confidence, CreditLink, Facts, ReviewDecision, SourceRecord, TrackFacts, WorkLink,
};

fn credit(mbid: &str, name: &str) -> CreditLink {
    CreditLink {
        relation_id: Some("relation-1".into()),
        role_id: None,
        role: "producer".into(),
        direction: None,
        artist_mbid: mbid.into(),
        artist_name: name.into(),
        credited_as: Some("Watt".into()),
        attributes: vec![],
        began: None,
        ended: None,
        over: None,
        order: None,
    }
}

fn source(path: &str, artist_mbid: &str, confidence: Confidence) -> SourceRecord {
    SourceRecord {
        key: path.into(),
        source: "musicbrainz".into(),
        source_id: Some("recording-1".into()),
        fetched_at: 2,
        confidence,
        facts: Facts::Track(TrackFacts {
            recording: Some("recording-1".into()),
            credits: vec![credit(artist_mbid, "Andrew Watt")],
            works: vec![WorkLink {
                mbid: "work-1".into(),
                title: "A Song".into(),
                credits: vec![credit(artist_mbid, "Andrew Watt")],
                ..Default::default()
            }],
            relationships_complete: true,
            ..Default::default()
        }),
    }
}

fn catalog() -> Catalog {
    Catalog {
        files: vec![
            AudioFile {
                id: 0,
                path: "/music/one.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 1,
                path: "/music/reissue.flac".into(),
                ..Default::default()
            },
        ],
        tracks: vec![
            Track {
                id: 0,
                file_id: 0,
                recording_id: 0,
                mbid: Some("recording-1".into()),
                ..Default::default()
            },
            Track {
                id: 1,
                file_id: 1,
                recording_id: 0,
                mbid: Some("recording-1".into()),
                ..Default::default()
            },
        ],
        recordings: vec![Recording {
            id: 0,
            mbid: Some("recording-1".into()),
            track_ids: vec![0, 1],
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[test]
fn source_only_artist_is_grouped_by_identity_across_placements_and_scopes() {
    let catalog = catalog();
    let mut sources = Sources::default();
    sources.set(source(
        "/music/one.flac",
        "artist-1",
        Confidence::Identified,
    ));
    sources.set(source(
        "/music/reissue.flac",
        "artist-1",
        Confidence::Identified,
    ));
    let people = sourced(&catalog, &sources);
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].mbid, "artist-1");
    assert_eq!(people[0].recording_ids, vec![0]);
    assert_eq!(people[0].work_mbids, vec!["work-1"]);
    assert!(matches_name(&people[0], "Watt", true));
}

#[test]
fn local_or_untrusted_identity_is_not_promoted_as_source_only() {
    let mut catalog = catalog();
    let mut sources = Sources::default();
    sources.set(source(
        "/music/one.flac",
        "artist-1",
        Confidence::Matched(90),
    ));
    assert!(sourced(&catalog, &sources).is_empty());

    sources.set(source(
        "/music/one.flac",
        "artist-1",
        Confidence::Identified,
    ));
    catalog.artists.push(Artist {
        id: 0,
        mbid: Some("artist-1".into()),
        ..Default::default()
    });
    assert!(sourced(&catalog, &sources).is_empty());
}

#[test]
fn accepting_then_rejecting_a_source_claim_controls_contributor_navigation() {
    let catalog = catalog();
    let mut sources = Sources::default();
    sources.set(source(
        "/music/one.flac",
        "artist-1",
        Confidence::Matched(90),
    ));
    let id = sources.review_items(&catalog)[0].id.clone();
    assert!(sourced(&catalog, &sources).is_empty());
    sources
        .decide(&catalog, &id, ReviewDecision::Accepted, 3)
        .unwrap();
    assert_eq!(sourced(&catalog, &sources).len(), 1);
    sources
        .decide(&catalog, &id, ReviewDecision::Rejected, 4)
        .unwrap();
    assert!(sourced(&catalog, &sources).is_empty());
}

#[test]
fn equal_names_with_different_ids_remain_distinct() {
    let catalog = catalog();
    let mut sources = Sources::default();
    sources.set(source(
        "/music/one.flac",
        "artist-1",
        Confidence::Identified,
    ));
    sources.set(source(
        "/music/reissue.flac",
        "artist-2",
        Confidence::Identified,
    ));
    let people = sourced(&catalog, &sources);
    assert_eq!(people.len(), 2);
    assert_eq!(people[0].name, people[1].name);
    assert_ne!(people[0].mbid, people[1].mbid);
}
