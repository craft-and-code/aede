use super::*;
use aede_core::model::{self, ScannedFile};
use aede_core::sources::{
    Confidence, Facts, ReviewDecision, SourceRecord, Sources, TrackFacts, WorkLink,
};
use aede_core::tags::RawTags;

#[test]
fn canonical_work_unites_only_trusted_same_identity_recordings() {
    let mut scanned = Vec::new();
    for (index, recording, work) in [
        (0, "first-recording", Some("shared-work")),
        (1, "second-recording", Some("shared-work")),
        (2, "third-recording", None),
        (3, "third-recording", None),
        (4, "unrelated-recording", None),
    ] {
        let mut tags = RawTags::default();
        tags.insert("title", format!("Recording {index}"));
        tags.insert("artist", "Performer");
        tags.insert("album", format!("Edition {index}"));
        tags.insert("musicbrainz_recordingid", recording);
        if let Some(work) = work {
            tags.insert("musicbrainz_workid", work);
            tags.insert("grouping", "Shared Composition");
        }
        scanned.push(ScannedFile {
            path: format!("/music/{index}.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let catalog = model::build(scanned, vec!["/music".into()], 1, &[]);
    assert_eq!(catalog.recordings.len(), 4);
    let work = &catalog.works[0];
    assert_eq!(work.recording_ids, [0, 1]);
    assert_eq!(
        recordings_of_work(work, &[])
            .into_iter()
            .collect::<Vec<_>>(),
        [0, 1]
    );
    for (mode, expected) in [
        ("identified", vec![0, 1, 2]),
        ("pending", vec![0, 1]),
        ("accepted", vec![0, 1, 2]),
        ("rejected", vec![0, 1]),
        ("undone", vec![0, 1]),
        ("conflicting", vec![0, 1]),
        ("accepted-conflict", vec![0, 1, 2]),
    ] {
        let mut held = Sources::default();
        for (index, recording, mbid) in [
            (0, "first-recording", "shared-work"),
            (2, "third-recording", "shared-work"),
            (3, "third-recording", "shared-work"),
            (4, "unrelated-recording", "another-work"),
        ] {
            let additional = index == 2 || index == 3;
            let conflicting = additional && mode.contains("conflict");
            let source_id = if conflicting {
                "different-recording"
            } else {
                recording
            };
            held.set(SourceRecord {
                key: format!("/music/{index}.flac"),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(source_id.into()),
                fetched_at: 42,
                confidence: if additional
                    && ["pending", "accepted", "rejected", "undone"].contains(&mode)
                {
                    Confidence::Matched(90)
                } else {
                    Confidence::Identified
                },
                facts: Facts::Track(TrackFacts {
                    recording: Some(source_id.into()),
                    works: vec![WorkLink {
                        mbid: mbid.into(),
                        // Local/source spelling differences cannot split an exact identity;
                        // an identical title under another MBID cannot join it.
                        title: if additional {
                            "Source spelling"
                        } else {
                            "Shared Composition"
                        }
                        .into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            });
        }
        if ["accepted", "rejected", "undone", "accepted-conflict"].contains(&mode) {
            for review in held.review_items(&catalog) {
                held.decide(&catalog, &review.id, ReviewDecision::Accepted, 50)
                    .unwrap();
                if mode == "rejected" {
                    held.decide(&catalog, &review.id, ReviewDecision::Rejected, 51)
                        .unwrap();
                } else if mode == "undone" {
                    held.clear_review(&catalog, &review.id).unwrap();
                }
            }
        }
        let links = held.work_links(&catalog);
        assert_eq!(
            recordings_of_work(work, &links)
                .into_iter()
                .collect::<Vec<_>>(),
            expected,
            "{mode}"
        );
        assert_eq!(catalog.works.len(), 1);
        assert_eq!(
            work.recording_ids,
            [0, 1],
            "source links remain beside tags"
        );
    }
}
