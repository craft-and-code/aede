use super::*;
use crate::model::{AudioFile, Recording, Track};
use crate::sources::{CreditLink, Facts, SourceRecord, TrackFacts, WorkLink, WorkParentLink};

fn catalog() -> Catalog {
    Catalog {
        files: (0..2)
            .map(|id| AudioFile {
                id,
                path: format!("/music/{id}.flac"),
                ..Default::default()
            })
            .collect(),
        tracks: (0..2)
            .map(|id| Track {
                id,
                file_id: id,
                recording_id: 0,
                mbid: Some("recording-1".into()),
                ..Default::default()
            })
            .collect(),
        recordings: vec![Recording {
            id: 0,
            mbid: Some("recording-1".into()),
            track_ids: vec![0, 1],
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn source(id: u32, confidence: Confidence, fetched_at: u64, name: &str) -> SourceRecord {
    SourceRecord {
        key: format!("/music/{id}.flac"),
        source: "musicbrainz".into(),
        source_id: Some("recording-1".into()),
        confidence,
        fetched_at,
        facts: Facts::Track(TrackFacts {
            recording: Some("recording-1".into()),
            credits: vec![CreditLink {
                relation_id: Some("credit-1".into()),
                role_id: None,
                role: "producer".into(),
                direction: None,
                artist_mbid: "artist-1".into(),
                artist_name: name.into(),
                credited_as: None,
                attributes: vec![],
                began: None,
                ended: None,
                over: None,
                order: None,
            }],
            works: vec![WorkLink {
                mbid: "work-1".into(),
                title: name.into(),
                parents: vec![WorkParentLink {
                    mbid: "parent-1".into(),
                    title: name.into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            relationships_complete: true,
            ..Default::default()
        }),
    }
}

#[test]
fn repeated_relationships_keep_trusted_evidence_and_count_every_observation() {
    let catalog = catalog();
    let rows = [
        source(0, Confidence::matched(90), 20, "Alpha Untrusted"),
        source(1, Confidence::Identified, 10, "Zed Trusted"),
    ];
    for records in [rows.to_vec(), rows.into_iter().rev().collect()] {
        let sources = Sources {
            records,
            ..Default::default()
        };
        let projected = edges(&catalog, &sources);
        assert_eq!(projected.len(), 3);
        assert!(projected.iter().all(|edge| edge.trusted));
        assert!(projected.iter().all(|edge| edge.weight == 2));
        assert!(projected.iter().all(|edge| edge.fetched_at == Some(10)));
        assert_eq!(sources.records.len(), 2, "the original claims are retained");
    }
}

#[test]
fn repeated_trusted_relationships_display_the_most_recent_snapshot() {
    let catalog = catalog();
    let rows = [
        source(0, Confidence::Identified, 10, "Alpha Old"),
        source(1, Confidence::Identified, 20, "Zed New"),
    ];
    let first = edges(
        &catalog,
        &Sources {
            records: rows.to_vec(),
            ..Default::default()
        },
    );
    let reversed = edges(
        &catalog,
        &Sources {
            records: rows.into_iter().rev().collect(),
            ..Default::default()
        },
    );
    assert_eq!(first, reversed);
    assert!(first.iter().all(|edge| edge.fetched_at == Some(20)));
    assert!(first.iter().all(|edge| edge.weight == 2));
}

#[test]
fn a_credit_exclusion_survives_an_external_artist_becoming_local() {
    let mut catalog = catalog();
    let mut sources = Sources {
        records: vec![source(0, Confidence::Identified, 10, "A Producer")],
        ..Default::default()
    };
    let reference = credit_reference(&catalog, &sources.credit_links(&catalog)[0]).unwrap();
    sources.exclude_credit(reference.clone(), 11);
    catalog.artists.push(crate::model::Artist {
        id: 0,
        key: "a producer".into(),
        name: "A Producer".into(),
        mbid: Some("artist-1".into()),
        ..Default::default()
    });
    let link = &sources.credit_links(&catalog)[0];
    assert!(link.excluded);
    assert!(!link.trusted);
    assert_eq!(
        credit_reference(&catalog, link).unwrap().source.key,
        "a producer"
    );
    assert_eq!(
        EntityRef::new(EntityKind::Artist, "mbid:artist-1").resolve(&catalog),
        Some(0)
    );
    catalog.artists.clear();
    assert!(sources.credit_links(&catalog)[0].excluded);
}

#[test]
fn legacy_named_exclusions_match_only_the_same_external_artist_and_can_be_undone() {
    let mut catalog = catalog();
    catalog.artists.push(crate::model::Artist {
        id: 0,
        key: "a producer".into(),
        name: "A Producer".into(),
        mbid: Some("artist-1".into()),
        ..Default::default()
    });
    let mut sources = Sources {
        records: vec![source(0, Confidence::Identified, 10, "A Producer")],
        ..Default::default()
    };
    let reference = credit_reference(&catalog, &sources.credit_links(&catalog)[0]).unwrap();
    let mut legacy = reference.clone();
    legacy.source = EntityRef::new(EntityKind::Artist, "a producer");
    sources.exclude_credit(legacy, 11);
    assert!(sources.credit_links(&catalog)[0].excluded);
    let mut other_artist = reference.clone();
    other_artist.source = EntityRef::new(EntityKind::Artist, "mbid:artist-2");
    assert!(!same_credit_relation(
        &catalog,
        &sources.credit_exclusions[0].relation,
        &other_artist
    ));
    assert!(sources.restore_credit_in(&catalog, &reference));
    assert!(sources.credit_links(&catalog)[0].trusted);
}

#[test]
fn a_new_exclusion_keeps_the_artist_identity_after_local_tags_disappear() {
    let mut catalog = catalog();
    catalog.artists.push(crate::model::Artist {
        id: 0,
        key: "a producer".into(),
        name: "A Producer".into(),
        mbid: Some("artist-1".into()),
        ..Default::default()
    });
    let mut sources = Sources {
        records: vec![source(0, Confidence::Identified, 10, "A Producer")],
        ..Default::default()
    };
    let reference = credit_reference(&catalog, &sources.credit_links(&catalog)[0]).unwrap();
    assert_eq!(reference.source.key, "a producer");
    sources.exclude_credit_in(&catalog, reference, 11);
    assert_eq!(
        sources.credit_exclusions[0].relation.source.key,
        "mbid:artist-1"
    );
    catalog.artists.clear();
    let link = &sources.credit_links(&catalog)[0];
    assert!(link.excluded);
    let external = credit_reference(&catalog, link).unwrap();
    assert!(sources.restore_credit_in(&catalog, &external));
    assert!(sources.credit_links(&catalog)[0].trusted);
}
