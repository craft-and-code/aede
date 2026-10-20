//! Tests for [`super`], split out of `sources.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;

#[test]
fn edition_aliases_survive_reload_without_widening_other_source_scopes() {
    let alias = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let group = "33333333-3333-4333-8333-333333333333";
    let unrelated = "44444444-4444-4444-8444-444444444444";
    for source in [MUSICBRAINZ, "manual", "other"] {
        for source_id in [canonical, group, "not-an-mbid"] {
            for (complete, captured) in [
                (true, Some(alias)),
                (false, Some(alias)),
                (true, None),
                (true, Some("not-an-mbid")),
            ] {
                let mut held = Sources::default();
                held.set(SourceRecord {
                    key: "example band|example album|/music/example".into(),
                    source: source.into(),
                    source_id: Some(source_id.into()),
                    fetched_at: 1,
                    confidence: Confidence::Identified,
                    facts: Facts::Release(ReleaseFacts {
                        edition_mbid: Some(canonical.into()),
                        edition_lookup_mbid: captured.map(str::to_string),
                        lookup_mbid: Some(unrelated.into()),
                        relationships_complete: complete,
                        ..Default::default()
                    }),
                });
                let loaded = from_json(&to_json(&held)).unwrap();
                assert_eq!(loaded, held);
                let record = &loaded.records[0];
                let proven = source == MUSICBRAINZ
                    && source_id != "not-an-mbid"
                    && complete
                    && captured == Some(alias);
                assert!(record.matches_edition(canonical));
                assert_eq!(record.matches_edition(alias), proven);
                assert!(
                    !record.matches_edition(unrelated),
                    "a group alias cannot identify an edition"
                );
                assert!(!record.matches_edition(""));
                if source_id == canonical {
                    assert_eq!(record.musicbrainz_lookup_alias(), proven.then_some(alias));
                }
            }
        }
    }
}

#[test]
fn label_lookup_aliases_round_trip_without_upgrading_a_name_match() {
    let alias = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let different = "33333333-3333-4333-8333-333333333333";
    for (local, captured, confidence, automatic) in [
        (Some(alias), Some(alias), Confidence::Identified, true),
        (Some(canonical), Some(alias), Confidence::Identified, true),
        (None, Some(alias), Confidence::Identified, true),
        (Some(different), Some(alias), Confidence::Identified, false),
        (Some(alias), Some(alias), Confidence::Matched(95), false),
        (Some(alias), None, Confidence::Identified, false),
        (
            Some(alias),
            Some("not-an-mbid"),
            Confidence::Identified,
            false,
        ),
    ] {
        for decision in [
            None,
            Some(ReviewDecision::Accepted),
            Some(ReviewDecision::Rejected),
        ] {
            let catalog = crate::model::Catalog {
                labels: vec![crate::model::Label {
                    id: 0,
                    key: "example records".into(),
                    name: "Example Records".into(),
                    mbid: local.map(str::to_string),
                }],
                ..Default::default()
            };
            let entity = EntityRef::of(&catalog, EntityKind::Label, 0).unwrap();
            let mut held = Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: MUSICBRAINZ.into(),
                source_id: Some(canonical.into()),
                fetched_at: 1,
                confidence,
                facts: Facts::Label(LabelFacts {
                    lookup_mbid: captured.map(str::to_string),
                    ..Default::default()
                }),
            });
            if let Some(decision) = decision {
                held.set_review(SourceReview {
                    entity: entity.clone(),
                    source: MUSICBRAINZ.into(),
                    source_id: Some(canonical.into()),
                    decision,
                    reviewed_at: 17,
                });
            }
            let loaded = from_json(&to_json(&held)).unwrap();
            assert_eq!(loaded, held);
            let record = loaded.get(&entity, MUSICBRAINZ).unwrap();
            let expected = match decision {
                Some(ReviewDecision::Accepted) => true,
                Some(ReviewDecision::Rejected) => false,
                None => automatic,
            };
            assert_eq!(loaded.is_trusted(&catalog, record), expected);
            let resolved = loaded.label_identities(&catalog);
            assert_eq!(resolved.len(), usize::from(expected));
            if expected {
                assert_eq!(resolved[0].mbid, canonical);
            }
        }
    }
}

#[test]
fn release_group_aliases_round_trip_without_widening_edition_or_review_scope() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let different = "33333333-3333-4333-8333-333333333333";
    for (edition, captured, confidence, expected_alias, automatic) in [
        (
            None,
            Some(queried),
            Confidence::Identified,
            Some(queried),
            true,
        ),
        (
            Some(different),
            Some(queried),
            Confidence::Identified,
            Some(queried),
            true,
        ),
        (
            Some(canonical),
            Some(queried),
            Confidence::Identified,
            None,
            false,
        ),
        (None, None, Confidence::Identified, None, false),
        (
            None,
            Some("not-an-mbid"),
            Confidence::Identified,
            None,
            false,
        ),
        (
            None,
            Some(queried),
            Confidence::Matched(95),
            Some(queried),
            false,
        ),
    ] {
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: "example band|example album|/music/example".into(),
            source: MUSICBRAINZ.into(),
            source_id: Some(canonical.into()),
            fetched_at: 1,
            confidence,
            facts: Facts::Release(ReleaseFacts {
                lookup_mbid: captured.map(str::to_string),
                edition_mbid: edition.map(str::to_string),
                ..Default::default()
            }),
        });
        let loaded = from_json(&to_json(&held)).unwrap();
        assert_eq!(loaded, held);
        let record = &loaded.records[0];
        assert_eq!(record.musicbrainz_lookup_alias(), expected_alias);
        assert_eq!(
            trusted_with_identity(record, Some(queried), None),
            automatic
        );
        assert!(!trusted_with_identity(record, Some(different), None));
        assert!(trusted_with_identity(
            record,
            Some(queried),
            Some(ReviewDecision::Accepted)
        ));
        assert!(!trusted_with_identity(
            record,
            Some(queried),
            Some(ReviewDecision::Rejected)
        ));
    }
}

#[test]
fn recording_cache_identity_rejects_contradictions_but_retains_legacy_single_ids() {
    let canonical = "11111111-1111-4111-8111-111111111111";
    let other = "22222222-2222-4222-8222-222222222222";
    for source in [MUSICBRAINZ, "other"] {
        for (source_id, recording, expected) in [
            (Some(canonical), Some(canonical), Some(canonical)),
            (Some(canonical), None, Some(canonical)),
            (None, Some(canonical), Some(canonical)),
            (Some(canonical), Some(other), None),
            (None, None, None),
            (Some(""), Some(canonical), None),
            (
                Some("legacy-recording"),
                Some("legacy-recording"),
                Some("legacy-recording"),
            ),
        ] {
            let held = Sources {
                records: vec![SourceRecord {
                    key: "/music/example.flac".into(),
                    source: source.into(),
                    source_id: source_id.map(str::to_string),
                    fetched_at: 1,
                    confidence: Confidence::Identified,
                    facts: Facts::Track(TrackFacts {
                        recording: recording.map(str::to_string),
                        ..Default::default()
                    }),
                }],
                ..Default::default()
            };
            let loaded = from_json(&to_json(&held)).unwrap();
            assert_eq!(loaded, held);
            assert_eq!(
                loaded.records[0].musicbrainz_recording_identity(),
                if source == MUSICBRAINZ {
                    expected
                } else {
                    None
                }
            );
        }
    }
    let release = SourceRecord {
        key: "example album".into(),
        source: MUSICBRAINZ.into(),
        source_id: Some(canonical.into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    };
    assert_eq!(release.musicbrainz_recording_identity(), None);
}

#[test]
fn recording_lookup_aliases_need_completed_consistent_evidence() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let different = "33333333-3333-4333-8333-333333333333";
    for (case, complete, recording, captured, expected) in [
        ("verified alias", true, Some(canonical), Some(queried), true),
        (
            "incomplete lookup",
            false,
            Some(canonical),
            Some(queried),
            false,
        ),
        (
            "inconsistent canonical ID",
            true,
            Some(different),
            Some(queried),
            false,
        ),
        ("missing canonical ID", true, None, Some(queried), false),
        ("legacy receipt", true, Some(canonical), None, false),
        (
            "invalid query",
            true,
            Some(canonical),
            Some("not-an-mbid"),
            false,
        ),
    ] {
        let record = SourceRecord {
            key: "/music/example.flac".into(),
            source: MUSICBRAINZ.into(),
            source_id: Some(canonical.into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Track(TrackFacts {
                recording: recording.map(str::to_owned),
                relationships_complete: complete,
                relationships_for: captured.map(str::to_owned),
                ..Default::default()
            }),
        };
        assert_eq!(
            trusted_with_identity(&record, Some(queried), None),
            expected,
            "{case}"
        );
        assert!(
            !trusted_with_identity(&record, Some(different), None),
            "{case}"
        );
        assert!(
            !trusted_with_identity(&record, Some(queried), Some(ReviewDecision::Rejected)),
            "{case}"
        );
    }
}

#[test]
fn verified_artist_lookup_aliases_survive_storage_without_trusting_other_identities() {
    let alias = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let different = "33333333-3333-4333-8333-333333333333";
    for (case, local, captured, confidence, automatic) in [
        ("canonical", canonical, None, Confidence::Identified, true),
        (
            "verified alias",
            alias,
            Some(alias),
            Confidence::Identified,
            true,
        ),
        (
            "different artist",
            different,
            Some(alias),
            Confidence::Identified,
            false,
        ),
        (
            "pending name match",
            alias,
            Some(alias),
            Confidence::Matched(95),
            false,
        ),
        (
            "legacy unknown alias",
            alias,
            None,
            Confidence::Identified,
            false,
        ),
        (
            "invalid alias",
            alias,
            Some("not-an-mbid"),
            Confidence::Identified,
            false,
        ),
    ] {
        for decision in [
            None,
            Some(ReviewDecision::Accepted),
            Some(ReviewDecision::Rejected),
        ] {
            let catalog = crate::model::Catalog {
                artists: vec![crate::model::Artist {
                    id: 0,
                    key: "example band".into(),
                    name: "Example Band".into(),
                    mbid: Some(local.into()),
                    ..Default::default()
                }],
                ..Default::default()
            };
            let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap();
            let mut held = Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: MUSICBRAINZ.into(),
                source_id: Some(canonical.into()),
                fetched_at: 1,
                confidence,
                facts: Facts::Artist(ArtistFacts {
                    lookup_mbid: captured.map(str::to_owned),
                    ..Default::default()
                }),
            });
            if let Some(decision) = decision {
                held.set_review(SourceReview {
                    entity: entity.clone(),
                    source: MUSICBRAINZ.into(),
                    source_id: Some(canonical.into()),
                    decision,
                    reviewed_at: 17,
                });
            }
            let reloaded = from_json(&to_json(&held)).unwrap();
            assert_eq!(held, reloaded, "{case}");
            let record = reloaded.get(&entity, MUSICBRAINZ).unwrap();
            assert_eq!(
                reloaded.is_trusted(&catalog, record),
                match decision {
                    Some(ReviewDecision::Accepted) => true,
                    Some(ReviewDecision::Rejected) => false,
                    None => automatic,
                },
                "{case}, {decision:?}"
            );
        }
    }
}

#[test]
fn displaying_a_discography_year_preserves_unreadable_dates_without_panicking() {
    for (date, expected) in [
        (None, None),
        (Some("2000"), Some("2000")),
        (Some("2000-01-02"), Some("2000")),
        (Some("200"), Some("200")),
        (Some(""), Some("")),
        (Some("éé"), Some("éé")),
        (Some("二〇〇〇"), None),
        (Some("20€"), None),
        (Some("20𐀀"), None),
    ] {
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: "example artist".into(),
            source: MUSICBRAINZ.into(),
            source_id: Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                discography: vec![KnownRelease {
                    mbid: "11111111-1111-4111-8111-111111111111".into(),
                    title: "An album".into(),
                    first_released: date.map(str::to_owned),
                    primary_type: Some("Album".into()),
                    secondary_types: vec![],
                }],
                ..Default::default()
            }),
        });
        let before = to_json(&held);
        let reloaded = from_json(&before).unwrap();
        let Facts::Artist(artist) = &reloaded.records[0].facts else {
            panic!("the imported artist remains available");
        };
        assert_eq!(artist.discography[0].year(), expected, "{date:?}");
        assert_eq!(to_json(&reloaded), before, "the source date is retained");
    }
}

#[cfg(unix)]
#[test]
fn source_save_does_not_follow_a_predictable_temporary_symlink() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("aede_sources_link_{}_{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = sources_path(&dir);
    let victim = dir.join("keep.txt");
    std::fs::write(&victim, "keep me").unwrap();
    std::os::unix::fs::symlink(&victim, path.with_extension("json.tmp")).unwrap();
    save(&Sources::default(), &path).unwrap();
    let held = std::fs::read_to_string(&victim).unwrap();
    let published_is_regular = std::fs::symlink_metadata(&path).unwrap().is_file();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        held, "keep me",
        "a stale temporary link must not redirect a save"
    );
    assert!(published_is_regular);
}

fn release(primary: &str, label: &str) -> Facts {
    Facts::Release(ReleaseFacts {
        primary_type: Some(primary.to_string()),
        secondary_types: vec!["Live".to_string()],
        first_released: Some("1973".to_string()),
        label: Some(label.to_string()),
        label_mbid: None,
        cover_art: None,
        ..Default::default()
    })
}

fn record(key: &str, source: &str, facts: Facts) -> SourceRecord {
    SourceRecord {
        key: key.to_string(),
        source: source.to_string(),
        source_id: Some("6a1b…".to_string()),
        fetched_at: 1_700_000_000,
        confidence: Confidence::Identified,
        facts,
    }
}

#[test]
fn a_second_fetch_updates_and_does_not_duplicate() {
    // The point of keying on (entity, source): asking MusicBrainz twice
    // must leave one answer, not two that a reader has to arbitrate.
    let mut sources = Sources::default();
    let entity = EntityRef {
        kind: EntityKind::Release,
        key: "pink floyd|dark side|/music".to_string(),
    };

    assert!(!sources.set(record(
        &entity.key,
        MUSICBRAINZ,
        release("Album", "Harvest")
    )));
    assert!(sources.set(record(&entity.key, MUSICBRAINZ, release("Album", "EMI"))));
    assert_eq!(sources.records.len(), 1, "one source, one answer");
    assert_eq!(
        sources.get(&entity, MUSICBRAINZ).map(|r| &r.facts),
        Some(&release("Album", "EMI")),
        "the newer answer replaced the older one"
    );

    // A second source is a second row: two of them disagreeing is
    // information, and merging them would destroy it.
    sources.set(record(&entity.key, "discogs", release("Album", "Capitol")));
    assert_eq!(sources.records.len(), 2);
    assert_eq!(sources.about(&entity).count(), 2);
}

#[test]
fn a_source_can_be_forgotten_without_touching_the_others() {
    let mut sources = Sources::default();
    sources.set(record("a", MUSICBRAINZ, release("Album", "Harvest")));
    sources.set(record("b", MUSICBRAINZ, release("EP", "Harvest")));
    sources.set(record("a", "discogs", release("Album", "Capitol")));

    assert_eq!(sources.forget(MUSICBRAINZ), 2);
    assert_eq!(sources.records.len(), 1);
    assert_eq!(sources.records[0].source, "discogs");
}

#[test]
fn two_kinds_sharing_a_key_are_two_entities() {
    // Keys are only unique within a kind: an artist and a release may
    // perfectly well be spelled the same, and reading one as the other
    // would attach a country to an album.
    let mut sources = Sources::default();
    sources.set(record("nirvana", MUSICBRAINZ, release("Album", "Sub Pop")));
    sources.set(record(
        "nirvana",
        MUSICBRAINZ,
        Facts::Artist(ArtistFacts {
            area: Some("United States".to_string()),
            ..Default::default()
        }),
    ));
    assert_eq!(sources.records.len(), 2, "two kinds, two rows");

    let artist = EntityRef {
        kind: EntityKind::Artist,
        key: "nirvana".to_string(),
    };
    assert_eq!(sources.about(&artist).count(), 1);
    assert!(matches!(
        sources.get(&artist, MUSICBRAINZ).map(|r| &r.facts),
        Some(Facts::Artist(_))
    ));
}

#[test]
fn the_verdict_is_about_meaning_and_not_about_spelling() {
    // Reporting `Album` against `album` as a disagreement would teach the
    // reader to skip the report, which costs more than the report is worth.
    assert_eq!(verdict("Album", Some("album")), Verdict::Agrees);
    assert_eq!(verdict("Björk", Some("Bjork")), Verdict::Agrees);
    assert_eq!(
        verdict("The Beatles", Some("Beatles, The")),
        Verdict::Agrees
    );

    // And the limit of that, pinned rather than left to be discovered: an
    // ampersand is not the word "and" to `normalize`, so this reports a
    // difference. Widening `normalize` to quiet it would loosen the rule
    // that decides two artists are one, which costs far more.
    assert!(
        matches!(
            verdict("Rock & Roll", Some("Rock and Roll")),
            Verdict::Differs { .. }
        ),
        "known limit: punctuation is dropped, `&` is not read as a word"
    );

    assert_eq!(
        verdict("Album", Some("EP")),
        Verdict::Differs {
            theirs: "Album".to_string(),
            yours: "EP".to_string()
        },
        "a real difference names both sides so the reader can judge"
    );

    // No tag is not a disagreement: the source is adding, not contradicting.
    assert_eq!(verdict("Album", None), Verdict::NothingToCompare);
    assert_eq!(verdict("Album", Some("   ")), Verdict::NothingToCompare);
}

#[test]
fn a_year_and_a_full_date_are_not_a_disagreement() {
    // The false alarm this would otherwise produce on nearly every album:
    // MusicBrainz answers a full date, a tag almost always holds a year.
    assert_eq!(verdict_date("1973-03-01", Some("1973")), Verdict::Agrees);
    assert_eq!(verdict_date("1973", Some("1973-03-01")), Verdict::Agrees);
    assert_eq!(
        verdict_date("1973-03-01", Some("1973-03-01")),
        Verdict::Agrees
    );

    // A real difference of year is still one, at either precision.
    assert!(matches!(
        verdict_date("1973-03-01", Some("1974")),
        Verdict::Differs { .. }
    ));
    // And two precise dates that differ are a disagreement, which is the
    // case a year-only comparison would have hidden.
    assert!(matches!(
        verdict_date("1973-03-01", Some("1973-03-24")),
        Verdict::Differs { .. }
    ));

    assert_eq!(verdict_date("1973", None), Verdict::NothingToCompare);
    // Not a date at all: falls back to the ordinary comparison rather than
    // inventing a year out of the first four characters.
    assert_eq!(verdict_date("unknown", Some("Unknown")), Verdict::Agrees);
}

#[test]
fn genres_are_compared_as_sets_and_not_as_sentences() {
    // The first false alarm this layer produced, kept as a test: MusicBrainz
    // answered `pop, dance-pop, electropop, europop`, the files said
    // `Rock, Pop`, and the report called it a disagreement. The tags say the
    // record is pop *and* rock; MusicBrainz says pop and three finer words for
    // it. Nobody is contradicting anybody.
    let theirs: Vec<String> = ["pop", "dance-pop", "electropop", "europop"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        verdict_set(&theirs, &["Rock, Pop".to_string()]),
        Verdict::Agrees,
        "one value holding a list is a list"
    );
    assert_eq!(
        verdict_set(&theirs, &["Rock".to_string(), "Pop".to_string()]),
        Verdict::Agrees,
        "and so are several values"
    );
    for written in ["Rock; Pop", "Rock / Pop", "rock,pop"] {
        assert_eq!(
            verdict_set(&theirs, &[written.to_string()]),
            Verdict::Agrees,
            "a genre tag is written every way there is: {written}"
        );
    }

    // Nothing in common is a real difference, and both sides are named.
    assert_eq!(
        verdict_set(&theirs, &["Jazz, Blues".to_string()]),
        Verdict::Differs {
            theirs: "pop, dance-pop, electropop, europop".to_string(),
            yours: "Jazz, Blues".to_string(),
        }
    );

    // No tag is not a disagreement — the source is adding, not contradicting.
    assert_eq!(verdict_set(&theirs, &[]), Verdict::NothingToCompare);
    assert_eq!(
        verdict_set(&theirs, &["  ".to_string()]),
        Verdict::NothingToCompare
    );
}

#[test]
fn an_answer_holding_nothing_is_not_the_absence_of_an_answer() {
    // "Asked, and MusicBrainz holds nothing about this artist" and "never
    // asked" are different states, and the layer exists to keep them apart.
    let empty = Facts::Artist(ArtistFacts::default());
    assert!(empty.is_empty());

    let mut sources = Sources::default();
    sources.set(record("someone", MUSICBRAINZ, empty));
    let entity = EntityRef {
        kind: EntityKind::Artist,
        key: "someone".to_string(),
    };
    assert!(
        sources.get(&entity, MUSICBRAINZ).is_some(),
        "the record exists, and says the source had nothing"
    );
}

#[test]
fn a_completed_empty_discography_survives_storage() {
    let json = crate::json::parse(&format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
            {{"entity":"artist:miles davis","source":"musicbrainz","source_id":"561d854a",
             "confidence":"identified","fetched_at":1,
             "facts":{{"discography":[],"discography_fetched_at":17}}}}
        ]}}"#
    ))
    .unwrap();
    let held = from_json(&json).unwrap();
    assert!(
        !held.records[0].facts.is_empty(),
        "an empty completed answer is knowledge, not an unasked source"
    );
    let encoded = to_json(&held);
    let written = encoded
        .get("records")
        .and_then(crate::json::Json::as_arr)
        .unwrap()[0]
        .get("facts")
        .unwrap();
    assert_eq!(
        written
            .get("discography_fetched_at")
            .and_then(crate::json::Json::as_u64),
        Some(17)
    );
    assert_eq!(from_json(&encoded).unwrap(), held);
}

#[test]
fn malformed_discography_markers_do_not_create_an_empty_completed_answer() {
    for facts in [
        r#"{"discography":[],"discography_fetched_at":false}"#,
        r#"{"discography":[],"discography_fetched_at":-1}"#,
        r#"{"discography":[],"discography_fetched_at":1.5}"#,
        r#"{"discography":[],"discography_fetched_at":18446744073709551615}"#,
        r#"{"discography":[],"discography_fetched_at":"17"}"#,
        r#"{"discography_fetched_at":17}"#,
        r#"{"discography":null,"discography_fetched_at":17}"#,
        r#"{"discography":[null],"discography_fetched_at":17}"#,
        r#"{"discography":[{}],"discography_fetched_at":17}"#,
    ] {
        let json = crate::json::parse(&format!(
            r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
                {{"entity":"artist:miles davis","source":"musicbrainz",
                 "confidence":"identified","fetched_at":1,"facts":{facts}}}
            ]}}"#
        ))
        .unwrap();
        let held = from_json(&json).unwrap();
        assert!(
            held.records[0].facts.is_empty(),
            "malformed facts must not become a completed empty answer: {facts}"
        );
    }
}

#[test]
fn an_unreadable_discography_does_not_cache_its_readable_subset() {
    for marker in ["", r#", "discography_fetched_at":17"#] {
        for unreadable in [r#"{}"#, r#"{"mbid":""}"#, r#"{"mbid":" "}"#] {
            let json = crate::json::parse(&format!(
                r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
                    {{"entity":"artist:miles davis","source":"musicbrainz","source_id":"561d854a",
                     "confidence":"identified","fetched_at":1,
                     "facts":{{"area":"United States","discography":[
                        {{"mbid":"group-1","title":"Valid"}},{unreadable}
                     ]{marker}}}}}
                ]}}"#
            ))
            .unwrap();
            let held = from_json(&json).unwrap();
            let Facts::Artist(artist) = &held.records[0].facts else {
                panic!("artist facts");
            };
            assert!(
                !artist.discography_is_cached(),
                "an invalid row must not leave a partial list cached through legacy fallback: {unreadable} {marker}"
            );
            assert!(artist.discography.is_empty());
            assert_eq!(
                artist.area.as_deref(),
                Some("United States"),
                "unrelated facts remain available"
            );
        }
    }
}

#[test]
fn a_round_trip_keeps_every_field() {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "pink floyd|dark side|/music".to_string(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("f5093c06".to_string()),
        fetched_at: 1_700_000_123,
        confidence: Confidence::matched(72),
        facts: release("Album", "Harvest"),
    });
    sources.set(SourceRecord {
        key: "miles davis".to_string(),
        source: "discogs".to_string(),
        source_id: None,
        fetched_at: 1_700_000_456,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            lookup_mbid: None,
            relationships_complete: true,
            area: Some("United States".to_string()),
            country_code: Some("US".to_string()),
            began: Some("1926-05-26".to_string()),
            ended: Some("1991-09-28".to_string()),
            active: Some(false),
            kind: Some("person".to_string()),
            disambiguation: Some("the trumpeter".to_string()),
            genres: vec!["jazz".to_string(), "cool jazz".to_string()],
            aliases: vec!["Miles Dewey Davis III".to_string()],
            wikidata: Some("https://www.wikidata.org/wiki/Q93341".to_string()),
            discogs: None,
            homepage: None,
            discography_fetched_at: Some(1_700_000_455),
            discography: vec![KnownRelease {
                mbid: "c9fdb94c".to_string(),
                title: "Kind of Blue".to_string(),
                first_released: Some("1959-08-17".to_string()),
                primary_type: Some("Album".to_string()),
                secondary_types: vec![],
            }],
            members: vec![Membership {
                relation_id: Some("membership-row".to_string()),
                role_id: Some("membership-type".to_string()),
                direction: Some("forward".to_string()),
                mbid: "b1a9c0e9".to_string(),
                name: "The Miles Davis Quintet".to_string(),
                credited_as: Some("Miles Davis Quintet".to_string()),
                side: Side::Group,
                kind: "member of band".to_string(),
                attributes: vec!["trumpet".to_string()],
                began: Some("1955".to_string()),
                ended: Some("1968".to_string()),
                over: Some(true),
            }],
            summary: Some(Prose {
                text: "An American trumpeter and bandleader.".to_string(),
                url: "https://en.wikipedia.org/wiki/Miles_Davis".to_string(),
                lang: "en".to_string(),
                licence: "CC BY-SA 4.0".to_string(),
            }),
            portrait: Some(Picture {
                url: "https://commons.wikimedia.org/wiki/Special:FilePath/Miles_Davis.jpg"
                    .to_string(),
            }),
            logo: Some(Picture {
                url: "https://assets.fanart.tv/fanart/music/miles-davis/hd-logo.png".to_string(),
            }),
        }),
    });

    let text = to_json(&sources).to_string_pretty();
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
    assert_eq!(back, sources, "written and read back are the same layer");
}

#[test]
fn recording_work_evidence_survives_the_round_trip() {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "/music/01.flac".to_string(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("recording-id".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            recording: Some("recording-id".to_string()),
            relationships_complete: true,
            credits: vec![CreditLink {
                relation_id: Some("producer-rel".to_string()),
                role_id: Some("producer-type".to_string()),
                role: "producer".to_string(),
                direction: Some("backward".to_string()),
                artist_mbid: "producer-id".to_string(),
                artist_name: "A Producer".to_string(),
                credited_as: Some("The Producer".to_string()),
                attributes: vec![CreditAttribute {
                    id: Some("additional-type".to_string()),
                    name: "additional".to_string(),
                    value: None,
                    credited_as: None,
                }],
                began: Some("2020".to_string()),
                ended: None,
                over: Some(false),
                order: Some(1),
            }],
            works: vec![WorkLink {
                mbid: "work-id".to_string(),
                title: "The Work".to_string(),
                relation_id: Some("performance-rel".to_string()),
                relation_type: Some("performance".to_string()),
                relation_type_id: Some("performance-type".to_string()),
                direction: Some("forward".to_string()),
                credits: vec![CreditLink {
                    relation_id: Some("composer-rel".to_string()),
                    role_id: Some("composer-type".to_string()),
                    role: "composer".to_string(),
                    direction: Some("backward".to_string()),
                    artist_mbid: "composer-id".to_string(),
                    artist_name: "A Composer".to_string(),
                    credited_as: None,
                    attributes: Vec::new(),
                    began: None,
                    ended: None,
                    over: None,
                    order: Some(2),
                }],
                ..Default::default()
            }],
            ..Default::default()
        }),
    });

    let encoded = to_json(&sources);
    let decoded = from_json(&encoded).expect("read back");
    let Facts::Track(facts) = &decoded.records[0].facts else {
        panic!("track facts");
    };
    assert_eq!(facts.works[0].mbid, "work-id");
    assert_eq!(facts.works[0].title, "The Work");
    assert_eq!(
        facts.works[0].relation_type_id.as_deref(),
        Some("performance-type")
    );
    assert_eq!(facts.credits[0].direction.as_deref(), Some("backward"));
    assert_eq!(
        facts.credits[0].attributes[0].id.as_deref(),
        Some("additional-type")
    );
    assert_eq!(
        facts.credits[0].credited_as.as_deref(),
        Some("The Producer")
    );
    assert_eq!(facts.works[0].credits[0].role, "composer");
    assert!(facts.relationships_complete);
}

#[test]
fn sourced_work_links_attach_to_the_recording_without_mutating_it() {
    let catalog = crate::model::tests::example_catalog();
    let track = &catalog.tracks[0];
    let path = catalog.file(track.file_id).expect("file").path.clone();
    let original_links = catalog.recordings[track.recording_id as usize]
        .work_ids
        .clone();
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: path,
        source: MUSICBRAINZ.to_string(),
        source_id: Some("recording-id".to_string()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            works: vec![WorkLink {
                mbid: "work-id".to_string(),
                title: "The Work".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });

    let links = sources.work_links(&catalog);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].recording_id, track.recording_id);
    assert_eq!(links[0].work.mbid, "work-id");
    assert_eq!(links[0].source, MUSICBRAINZ);
    assert_eq!(
        catalog.recordings[track.recording_id as usize].work_ids,
        original_links
    );
}

#[test]
fn movement_parent_survives_storage_and_only_trusted_evidence_is_navigable() {
    let catalog = crate::model::tests::example_catalog();
    let track = &catalog.tracks[0];
    let path = catalog.file(track.file_id).unwrap().path.clone();
    let parent = WorkParentLink {
        mbid: "symphony-id".into(),
        title: "Symphony No. 5".into(),
        relation_id: Some("part-rel".into()),
        relation_type_id: Some("parts-type".into()),
        direction: Some("backward".into()),
        attributes: vec![CreditAttribute {
            id: None,
            name: "movement".into(),
            value: None,
            credited_as: None,
        }],
        order: Some(1),
    };
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: path,
        source: MUSICBRAINZ.into(),
        source_id: Some("recording-id".into()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            recording: Some("recording-id".into()),
            works: vec![WorkLink {
                mbid: "movement-id".into(),
                title: "I. Allegro".into(),
                parents: vec![parent.clone()],
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let restored = from_json(&to_json(&sources)).unwrap();
    assert_eq!(restored, sources);
    let links = restored.work_parent_links(&catalog);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].recording_id, track.recording_id);
    assert_eq!(links[0].child_mbid, "movement-id");
    assert_eq!(links[0].parent, parent);
    assert!(links[0].trusted);
    let edges = crate::graph::edges(&catalog, &restored);
    let part = edges
        .iter()
        .find(|edge| edge.reference.kind == "part_of_work")
        .unwrap();
    assert!(part.trusted);
    assert_eq!(part.reference.source.key, "movement-id");
    assert_eq!(part.reference.target.key, "symphony-id");
    assert_eq!(part.order, Some(1));
    assert!(catalog.works.is_empty(), "source facts do not mutate tags");

    let mut uncertain = restored;
    uncertain.records[0].confidence = Confidence::matched(99);
    let uncertain_links = uncertain.work_parent_links(&catalog);
    assert!(!uncertain_links[0].trusted);
    let uncertain_edges = crate::graph::edges(&catalog, &uncertain);
    assert!(
        !uncertain_edges
            .iter()
            .find(|edge| edge.reference.kind == "part_of_work")
            .unwrap()
            .trusted
    );
}

#[test]
fn rich_credits_keep_their_recording_or_work_scope_and_provenance() {
    let catalog = crate::model::tests::example_catalog();
    let track = &catalog.tracks[0];
    let path = catalog.file(track.file_id).expect("file").path.clone();
    let recording_credit = CreditLink {
        relation_id: Some("performance-rel".to_string()),
        role_id: None,
        role: "instrument".to_string(),
        direction: None,
        artist_mbid: "player-id".to_string(),
        artist_name: "A Player".to_string(),
        credited_as: None,
        attributes: Vec::new(),
        began: None,
        ended: None,
        over: None,
        order: Some(1),
    };
    let work_credit = CreditLink {
        role: "composer".to_string(),
        artist_mbid: "writer-id".to_string(),
        artist_name: "A Writer".to_string(),
        ..recording_credit.clone()
    };
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: path,
        source: MUSICBRAINZ.to_string(),
        source_id: Some("recording-id".to_string()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            credits: vec![recording_credit],
            works: vec![WorkLink {
                mbid: "work-id".to_string(),
                title: "A Work".to_string(),
                credits: vec![work_credit],
                ..Default::default()
            }],
            relationships_complete: true,
            ..Default::default()
        }),
    });

    let links = sources.credit_links(&catalog);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].recording_id, track.recording_id);
    assert!(links[0].work.is_none());
    assert_eq!(
        links[1].work.as_ref().map(|work| work.mbid.as_str()),
        Some("work-id")
    );
    assert_eq!(links[1].source, MUSICBRAINZ);
    assert_eq!(links[1].fetched_at, 42);
}

#[test]
fn edition_credit_and_one_credit_exclusion_survive_round_trip() {
    let mut catalog = crate::model::tests::example_catalog();
    catalog.releases[0].mbid = Some("edition-id".into());
    catalog.releases[0].release_group_mbid = Some("group-id".into());
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("release");
    let credit = |id: &str, name: &str| CreditLink {
        relation_id: Some(id.into()),
        role_id: Some("producer-role".into()),
        role: "producer".into(),
        direction: None,
        artist_mbid: format!("{id}-artist"),
        artist_name: name.into(),
        credited_as: None,
        attributes: Vec::new(),
        began: None,
        ended: None,
        over: None,
        order: None,
    };
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: entity.key,
        source: MUSICBRAINZ.into(),
        source_id: Some("group-id".into()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            edition_mbid: Some("edition-id".into()),
            credits: vec![credit("first", "First"), credit("second", "Second")],
            relationships_complete: true,
            ..Default::default()
        }),
    });
    let links = sources.edition_credit_links(&catalog);
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(|link| link.trusted));
    let reference = crate::graph::edition_credit_reference(&catalog, &links[0]).unwrap();
    sources.exclude_credit(reference.clone(), 123);
    let restored = from_json(&to_json(&sources)).expect("round trip");
    let links = restored.edition_credit_links(&catalog);
    assert!(links[0].excluded);
    assert!(!links[0].trusted);
    assert!(
        links[1].trusted,
        "another credit in the same source record survives"
    );
    assert_eq!(restored.credit_exclusions[0].relation, reference);
    assert!(restored.clone().restore_credit(&reference));
}

#[test]
fn fetched_edition_credits_need_identification_but_manual_credits_use_the_local_key() {
    let mut catalog = crate::model::tests::example_catalog();
    catalog.releases[0].mbid = None;
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).unwrap();
    let mut held = Sources::default();
    let mut claim = SourceRecord {
        key: entity.key,
        source: MUSICBRAINZ.into(),
        source_id: Some("group-id".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            credits: vec![CreditLink {
                relation_id: None,
                role_id: None,
                role: "producer".into(),
                direction: None,
                artist_mbid: "artist-id".into(),
                artist_name: "A Producer".into(),
                credited_as: None,
                attributes: vec![],
                began: None,
                ended: None,
                over: None,
                order: None,
            }],
            relationships_complete: true,
            ..Default::default()
        }),
    };
    let mut manual = claim.clone();
    manual.source = "manual".into();
    manual.source_id = None;
    held.set(claim.clone());
    assert!(held.edition_credit_links(&catalog).is_empty());
    let Facts::Release(facts) = &mut claim.facts else {
        panic!("release");
    };
    facts.edition_mbid = Some("edition-id".into());
    held.set(claim);
    assert!(held.edition_credit_links(&catalog).is_empty());
    catalog.releases[0].mbid = Some("another-edition".into());
    assert!(held.edition_credit_links(&catalog).is_empty());
    catalog.releases[0].mbid = Some("edition-id".into());
    assert_eq!(held.edition_credit_links(&catalog).len(), 1);

    let manual = Sources {
        records: vec![manual],
        ..Default::default()
    };
    let restored = from_json(&to_json(&manual)).unwrap();
    catalog.releases[0].mbid = None;
    let links = restored.edition_credit_links(&catalog);
    assert_eq!(
        links.len(),
        1,
        "an explicit local correction needs no external identity"
    );
    assert_eq!(links[0].source, "manual");
    assert!(links[0].trusted);
    assert!(
        restored.credit_links(&catalog).is_empty(),
        "edition scope cannot become recording scope"
    );
    catalog.releases[0].mbid = Some("later-edition-id".into());
    assert_eq!(
        restored.edition_credit_links(&catalog).len(),
        1,
        "a newly added tag does not erase the local correction"
    );
}

#[test]
fn memberships_keep_external_identity_and_link_a_local_other_endpoint() {
    let catalog = crate::model::build(
        vec![
            crate::model::tests::track(
                "/music/Band/01.flac",
                &[
                    ("title", "Band Song"),
                    ("artist", "The Band"),
                    ("musicbrainz_artistid", "band-id"),
                ],
                1,
            ),
            crate::model::tests::track(
                "/music/Player/01.flac",
                &[
                    ("title", "Solo Song"),
                    ("artist", "A Player"),
                    ("musicbrainz_artistid", "player-id"),
                ],
                1,
            ),
        ],
        vec!["/music".into()],
        1,
        &[],
    );
    let band = catalog.find_artist("The Band").expect("band");
    let player = catalog.find_artist("A Player").expect("player");
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: band.key.clone(),
        source: MUSICBRAINZ.into(),
        source_id: Some("band-id".into()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            members: vec![Membership {
                relation_id: Some("membership-id".into()),
                role_id: Some("membership-type".into()),
                direction: Some("backward".into()),
                mbid: "player-id".into(),
                name: "A Player".into(),
                credited_as: None,
                side: Side::Player,
                kind: "member of band".into(),
                attributes: vec!["guitar".into()],
                began: Some("2000".into()),
                ended: None,
                over: Some(false),
            }],
            ..Default::default()
        }),
    });

    let links = sources.membership_links(&catalog);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].artist_id, band.id);
    assert_eq!(links[0].related_artist_id, Some(player.id));
    assert_eq!(links[0].membership.years(), "2000–");
    assert_eq!(links[0].source, MUSICBRAINZ);
}

#[test]
fn only_a_confirmed_musicbrainz_label_record_is_an_identity_link() {
    let catalog = crate::model::tests::example_catalog();
    let label = &catalog.labels[0];
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: label.key.clone(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("confirmed-label".to_string()),
        fetched_at: 42,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    sources.set(SourceRecord {
        key: "another label".to_string(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("search-result".to_string()),
        fetched_at: 43,
        confidence: Confidence::matched(100),
        facts: Facts::Label(LabelFacts::default()),
    });

    let identities = sources.label_identities(&catalog);
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].label_id, label.id);
    assert_eq!(identities[0].mbid, "confirmed-label");
}

#[test]
fn sourced_works_are_navigable_only_when_the_attachment_is_certain() {
    let catalog = crate::model::tests::example_catalog();
    let paths: Vec<_> = catalog
        .tracks
        .iter()
        .take(2)
        .map(|track| catalog.file(track.file_id).expect("file").path.clone())
        .collect();
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: paths[0].clone(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("recording-one".to_string()),
        fetched_at: 10,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            works: vec![WorkLink {
                mbid: "shared-work".to_string(),
                title: "Shared Composition".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    sources.set(SourceRecord {
        key: paths[1].clone(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("recording-two".to_string()),
        fetched_at: 11,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            works: vec![WorkLink {
                mbid: "shared-work".to_string(),
                title: "Shared Composition".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    sources.set(SourceRecord {
        key: paths[0].clone(),
        source: "fingerprint".to_string(),
        source_id: Some("possible-recording".to_string()),
        fetched_at: 12,
        confidence: Confidence::matched(98),
        facts: Facts::Track(TrackFacts {
            works: vec![WorkLink {
                mbid: "possible-work".to_string(),
                title: "Possible Composition".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });

    let works = sources.find_sourced_works(&catalog, "shared-work");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].mbid, "shared-work");
    assert_eq!(works[0].links.len(), 2, "evidence remains per recording");
    assert!(
        sources
            .find_sourced_works(&catalog, "possible-work")
            .is_empty(),
        "an approximate attachment remains evidence, not navigation"
    );

    let review = sources
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.source_id.as_deref() == Some("possible-recording"))
        .expect("the approximate attachment needs review");
    sources
        .decide(&catalog, &review.id, ReviewDecision::Accepted, 20)
        .expect("accepted");
    assert_eq!(
        sources.find_sourced_works(&catalog, "possible-work").len(),
        1,
        "accepting an approximate identity makes its relationships traversable"
    );

    let back = from_json(&to_json(&sources)).expect("review round trip");
    assert_eq!(back.reviews.len(), 1);
    assert_eq!(back.reviews[0].decision, ReviewDecision::Accepted);
    sources
        .decide(&catalog, &review.id, ReviewDecision::Rejected, 21)
        .expect("rejected");
    assert!(
        sources
            .find_sourced_works(&catalog, "possible-work")
            .is_empty(),
        "a rejection keeps the evidence but removes the traversal"
    );
    sources
        .clear_review(&catalog, &review.id)
        .expect("decision undone");
    assert!(sources.reviews.is_empty());
}

#[test]
fn label_identity_reconciliation_reports_agreement_proposals_and_conflicts() {
    let mut catalog = crate::model::tests::example_catalog();
    let label_id = catalog.labels[0].id;
    let key = catalog.labels[0].key.clone();
    catalog.labels[0].mbid = Some("tag-id".to_string());

    let mut sources = Sources::default();
    assert_eq!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Local {
            mbid: "tag-id".to_string()
        })
    );

    sources.set(SourceRecord {
        key: key.clone(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("tag-id".to_string()),
        fetched_at: 20,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    assert!(matches!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Agrees { .. })
    ));

    sources.set(SourceRecord {
        key: key.clone(),
        source: MUSICBRAINZ.to_string(),
        source_id: Some("candidate-id".to_string()),
        fetched_at: 21,
        confidence: Confidence::matched(92),
        facts: Facts::Label(LabelFacts::default()),
    });
    assert!(matches!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Suggested { .. })
    ));

    sources.set(SourceRecord {
        key,
        source: MUSICBRAINZ.to_string(),
        source_id: Some("different-id".to_string()),
        fetched_at: 22,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    assert_eq!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Conflict {
            local_mbid: "tag-id".to_string(),
            sourced_mbid: "different-id".to_string(),
            source: MUSICBRAINZ.to_string(),
            fetched_at: 22,
        })
    );
    assert_eq!(catalog.labels[0].mbid.as_deref(), Some("tag-id"));
    assert!(
        sources.label_identities(&catalog).is_empty(),
        "a conflicting source ID is not exposed as a safe identity link"
    );

    let review = sources
        .review_items(&catalog)
        .into_iter()
        .next()
        .expect("the conflict needs review");
    sources
        .decide(&catalog, &review.id, ReviewDecision::Accepted, 30)
        .expect("source accepted");
    assert!(matches!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Accepted { .. })
    ));
    assert_eq!(sources.label_identities(&catalog)[0].mbid, "different-id");
    assert_eq!(
        catalog.labels[0].mbid.as_deref(),
        Some("tag-id"),
        "review never rewrites the tag-built catalog"
    );

    sources
        .decide(&catalog, &review.id, ReviewDecision::Rejected, 31)
        .expect("source rejected");
    assert!(matches!(
        sources.label_identity(&catalog, label_id),
        Some(LabelIdentityResolution::Rejected { .. })
    ));
    assert!(sources.label_identities(&catalog).is_empty());
}

#[test]
fn a_refetch_only_keeps_a_review_when_the_claim_identity_is_unchanged() {
    let catalog = crate::model::tests::example_catalog();
    let track = &catalog.tracks[0];
    let path = catalog.file(track.file_id).expect("file").path.clone();
    let claim = |source_id: &str, fetched_at| SourceRecord {
        key: path.clone(),
        source: "fingerprint".to_string(),
        source_id: Some(source_id.to_string()),
        fetched_at,
        confidence: Confidence::matched(95),
        facts: Facts::Track(TrackFacts::default()),
    };
    let mut sources = Sources::default();
    sources.set(claim("candidate-a", 1));
    let id = sources.review_items(&catalog)[0].id.clone();
    sources
        .decide(&catalog, &id, ReviewDecision::Accepted, 2)
        .expect("reviewed");

    sources.set(claim("candidate-a", 3));
    assert_eq!(
        sources.reviews.len(),
        1,
        "refreshing the same claim preserves the decision"
    );
    sources.set(claim("candidate-b", 4));
    assert!(
        sources.reviews.is_empty(),
        "a different candidate must be reviewed on its own merits"
    );
}

#[test]
fn version_one_sources_migrate_with_no_invented_reviews() {
    let old = crate::json::parse(
        r#"{"format_version":1,"records":[
          {"entity":"artist:test","source":"musicbrainz",
           "confidence":"matched","score":90,"fetched_at":1,"facts":{}}
        ]}"#,
    )
    .expect("valid old document");
    let migrated = from_json(&old).expect("version one remains readable");
    assert_eq!(migrated.records.len(), 1);
    assert!(migrated.reviews.is_empty());
    assert_eq!(
        to_json(&migrated).field_u32("format_version"),
        Some(SOURCES_FORMAT_VERSION)
    );
}

#[test]
fn a_summary_without_its_attribution_is_not_read_back() {
    // The type makes it impossible to hold the words without the credit; a
    // document written by another build, or edited by hand, can still try.
    // Dropping the summary is the only answer that keeps the promise: the
    // alternative is prose on screen with nothing to attribute it to.
    let text = format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
             {{"entity":"artist:miles davis","source":"wikipedia",
               "confidence":"identified","fetched_at":1,
               "facts":{{"summary":{{"text":"A trumpeter.","lang":"en"}}}}}}
           ]}}"#
    );
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
    assert_eq!(back.records.len(), 1, "the row itself survives");
    let Facts::Artist(artist) = &back.records[0].facts else {
        panic!("an artist row");
    };
    assert_eq!(
        artist.summary, None,
        "prose with no url and no licence is dropped, not shown uncredited"
    );
    assert!(
        back.records[0].facts.is_empty(),
        "and the row then says the source held nothing, which is true of what \
         this build may repeat"
    );
}

#[test]
fn a_label_summary_round_trips_with_its_attribution() {
    let mut held = Sources::default();
    let summary = Prose {
        text: "An independent record label.".to_string(),
        url: "https://en.wikipedia.org/wiki/Record_label".to_string(),
        lang: "en".to_string(),
        licence: "CC BY-SA 4.0".to_string(),
    };
    held.set(record(
        "roadracer records",
        "wikipedia",
        Facts::Label(LabelFacts {
            summary: Some(summary.clone()),
            ..Default::default()
        }),
    ));
    let back = from_json(&to_json(&held)).expect("a readable layer");
    assert_eq!(back.records[0].facts, held.records[0].facts);

    let text = format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
             {{"entity":"label:roadracer records","source":"wikipedia",
               "confidence":"identified","fetched_at":1,
               "facts":{{"summary":{{"text":"A label.","lang":"en"}}}}}}
           ]}}"#
    );
    let incomplete =
        from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a readable layer");
    let Facts::Label(label) = &incomplete.records[0].facts else {
        panic!("label facts");
    };
    assert_eq!(label.summary, None, "uncredited prose is discarded");
}

#[test]
fn an_expired_discogs_profile_is_hidden_when_sources_are_loaded() {
    let dir = std::env::temp_dir().join(format!(
        "aede_expired_discogs_{}",
        std::thread::current()
            .name()
            .unwrap_or("test")
            .replace("::", "_")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let path = sources_path(&dir);
    let mut held = Sources::default();
    let mut old = record(
        "roadracer records",
        crate::discogs::SOURCE,
        Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: "An old profile.".to_string(),
                url: "https://www.discogs.com/label/33088".to_string(),
                lang: "und".to_string(),
                licence: "CC0".to_string(),
            }),
            ..Default::default()
        }),
    );
    old.fetched_at = crate::clock::now_seconds() - crate::discogs::MAX_AGE_SECONDS;
    held.set(old.clone());
    save(&held, &path).expect("saved");
    let loaded = load(&path).expect("readable").expect("layer");
    assert!(
        loaded.records.is_empty(),
        "stale text cannot be displayed or exported"
    );
    let mut stored = load_all(&path).expect("readable").expect("layer");
    assert_eq!(stored.records.len(), 1, "the original text stays stored");
    stored.set(record(
        "another label",
        MUSICBRAINZ,
        Facts::Label(LabelFacts::default()),
    ));
    save(&stored, &path).expect("unrelated update");
    assert_eq!(
        load_all(&path)
            .expect("readable")
            .expect("layer")
            .records
            .len(),
        2,
        "an unrelated write must not erase the old Discogs profile"
    );

    old.fetched_at = crate::clock::now_seconds();
    held.set(old);
    save(&held, &path).expect("saved");
    let loaded = load(&path).expect("readable").expect("layer");
    assert_eq!(
        loaded.records.len(),
        1,
        "a current profile remains available"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_credit_line_names_the_page_and_the_terms() {
    let prose = Prose {
        text: "A trumpeter.".to_string(),
        url: "https://en.wikipedia.org/wiki/Miles_Davis".to_string(),
        lang: "en".to_string(),
        licence: "CC BY-SA 4.0".to_string(),
    };
    let credit = prose.credit();
    assert!(
        credit.contains("en.wikipedia.org/wiki/Miles_Davis") && credit.contains("CC BY-SA 4.0"),
        "the two things CC BY-SA asks for, in one line: {credit}"
    );
}

#[test]
fn a_known_release_without_an_identifier_is_not_read_back() {
    // The identifier is what makes "you own this one" answerable. Without it
    // the only comparison left is the title, and two records share a title
    // often enough that a wish list built on titles alone is wrong.
    let text = format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
             {{"entity":"artist:miles davis","source":"musicbrainz",
               "confidence":"identified","fetched_at":1,
               "facts":{{"discography":[
                  {{"title":"No identifier","primary_type":"Album"}},
                  {{"mbid":"c9fdb94c","title":"Kind of Blue",
                    "primary_type":"Album","secondary_types":["Live"]}}
               ]}}}}
           ]}}"#
    );
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
    let Facts::Artist(artist) = &back.records[0].facts else {
        panic!("an artist row");
    };
    assert!(
        artist.discography.is_empty(),
        "an incomplete list is not retained as a completed legacy browse"
    );
    assert!(!artist.discography_is_cached());

    // Removing the unreadable row makes the list usable again, without losing
    // the secondary types that say whether the surviving album is a live one.
    let valid = format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
             {{"entity":"artist:miles davis","source":"musicbrainz",
               "confidence":"identified","fetched_at":1,
               "facts":{{"discography":[
                  {{"mbid":"c9fdb94c","title":"Kind of Blue",
                    "primary_type":"Album","secondary_types":["Live"]}}
               ]}}}}
           ]}}"#
    );
    let back = from_json(&crate::json::parse(&valid).unwrap()).unwrap();
    let Facts::Artist(artist) = &back.records[0].facts else {
        panic!("an artist row");
    };
    assert_eq!(artist.discography.len(), 1);
    assert!(
        artist.discography_is_cached(),
        "valid legacy lists stay reusable"
    );
    assert_eq!(artist.discography[0].mbid, "c9fdb94c");
    assert_eq!(artist.discography[0].secondary_types, vec!["Live"]);
    assert!(
        !artist.discography[0].is_studio_album(),
        "and what it is survived the round trip"
    );
}

#[test]
fn a_record_left_out_of_the_report_can_say_what_the_source_calls_it() {
    // A report that hides something owes the reason, and the reason is a fact
    // the source stated — so it is quoted rather than paraphrased: somebody
    // whose next move is to correct the type on MusicBrainz needs the word that
    // is written there.
    let known = |primary: Option<&str>, secondary: &[&str]| KnownRelease {
        mbid: "x".to_string(),
        title: "A record".to_string(),
        first_released: None,
        primary_type: primary.map(str::to_string),
        secondary_types: secondary.iter().map(|s| s.to_string()).collect(),
    };
    assert_eq!(known(Some("Single"), &[]).stated_type(), "Single");
    assert_eq!(
        known(Some("Album"), &["Live"]).stated_type(),
        "Album · Live"
    );
    assert_eq!(
        known(Some("Album"), &["Compilation", "Live"]).stated_type(),
        "Album · Compilation · Live",
        "every word the source used, in the order it used them"
    );
    // A studio album is never left out, so it never has to say anything — but
    // the answer is still the source's own word rather than nothing.
    assert_eq!(known(Some("Album"), &[]).stated_type(), "Album");
    // And a record nobody has typed says so: having no type is precisely why it
    // is not a studio album, and an empty cell would read as an unexplained
    // omission.
    assert_eq!(known(None, &[]).stated_type(), "no type");
    assert!(!known(None, &[]).is_studio_album());
}

#[test]
fn a_document_of_another_version_is_refused() {
    let mut root = Json::obj();
    root.set("format_version", (SOURCES_FORMAT_VERSION + 1).into());
    root.set("records", Json::Arr(vec![]));
    assert!(
        from_json(&root).is_err(),
        "a newer document is refused, not read approximately"
    );
}

#[test]
fn a_row_this_build_cannot_read_is_skipped_and_the_rest_survives() {
    // The layer is additional by nature: one unreadable fetched fact must
    // not stop the program from starting.
    let text = format!(
        r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
             {{"entity":"nonsense","source":"x","facts":{{}}}},
             {{"entity":"genre:jazz","source":"x","facts":{{}}}},
             {{"entity":"artist:miles davis","source":"musicbrainz",
               "confidence":"identified","fetched_at":1,
               "facts":{{"area":"United States"}}}}
           ]}}"#
    );
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
    assert_eq!(back.records.len(), 1, "the readable row survived alone");
    assert_eq!(back.records[0].key, "miles davis");
}

#[test]
fn unreadable_confidence_never_promotes_a_claim_to_identified() {
    let catalog = crate::model::Catalog::default();
    let mut original = Sources::default();
    original.set(record(
        "someone",
        MUSICBRAINZ,
        Facts::Artist(ArtistFacts::default()),
    ));
    for malformed in [
        Json::Null,
        Json::Str("identifier-ish".into()),
        Json::from(99u32),
    ] {
        let mut encoded = to_json(&original);
        let mut rows = encoded.get("records").unwrap().as_arr().unwrap().to_vec();
        rows[0].set("confidence", malformed);
        encoded.set("records", Json::Arr(rows));
        let decoded = from_json(&encoded).expect("the evidence remains readable");
        assert_eq!(decoded.records.len(), 1);
        assert!(!decoded.is_trusted(&catalog, &decoded.records[0]));
        assert_eq!(decoded.records[0].confidence, Confidence::matched(0));
    }
}

#[test]
fn malformed_source_tables_are_refused_before_they_can_be_saved_as_empty() {
    for table in ["records", "reviews", "credit_exclusions"] {
        let mut encoded = to_json(&Sources::default());
        encoded.set(table, Json::obj());
        assert!(from_json(&encoded).is_err(), "invalid table {table}");
    }
    let mut missing = Json::obj();
    missing.set("format_version", SOURCES_FORMAT_VERSION.into());
    assert!(from_json(&missing).is_err(), "records are required");
}

#[test]
fn supported_source_snapshots_require_a_facts_object_without_requiring_fields() {
    let document = |entity: &str, facts: Option<Json>, version: u32| {
        let mut row = Json::obj();
        row.set("entity", entity.into());
        row.set("source", "manual".into());
        row.set("confidence", "identified".into());
        if let Some(facts) = facts {
            row.set("facts", facts);
        }
        let mut value = Json::obj();
        value.set("format_version", version.into());
        value.set("records", Json::Arr(vec![row]));
        value
    };
    let mut admitted = Vec::new();
    for version in [1, SOURCES_FORMAT_VERSION] {
        for entity in ["artist:one", "release:one", "track:one", "label:one"] {
            for facts in [
                None,
                Some(Json::Null),
                Some(Json::Bool(false)),
                Some(1u32.into()),
                Some("facts".into()),
                Some(Json::Arr(vec![])),
            ] {
                let value = document(entity, facts.clone(), version);
                if from_json(&value).is_ok() {
                    admitted.push(format!("version {version}, {entity}, {facts:?}"));
                }
            }
            let held = from_json(&document(entity, Some(Json::obj()), version)).unwrap();
            assert_eq!(
                held.records.len(),
                1,
                "an intentionally empty object is valid"
            );
            assert_eq!(held.records[0].entity().to_token(), entity);
        }
        for entity in ["nonsense", "genre:jazz", "recording:one", "work:one"] {
            let held = from_json(&document(entity, Some(Json::Null), version)).unwrap();
            assert!(held.records.is_empty(), "unsupported rows remain ignored");
        }
    }
    assert!(
        admitted.is_empty(),
        "unreadable facts must not become empty replacement snapshots: {admitted:?}"
    );
}

#[test]
fn duplicate_source_snapshots_are_refused_instead_of_picking_the_first() {
    let mut original = Sources::default();
    original.set(record(
        "someone",
        MUSICBRAINZ,
        Facts::Artist(ArtistFacts::default()),
    ));
    let mut encoded = to_json(&original);
    let mut rows = encoded.get("records").unwrap().as_arr().unwrap().to_vec();
    rows.push(rows[0].clone());
    encoded.set("records", Json::Arr(rows));
    assert!(from_json(&encoded).is_err());
}

#[test]
fn duplicate_exact_review_keys_are_refused_without_collapsing_decisions() {
    let claim = record(
        "someone",
        MUSICBRAINZ,
        Facts::Artist(ArtistFacts::default()),
    );
    let review = SourceReview {
        entity: claim.entity(),
        source: claim.source.clone(),
        source_id: claim.source_id.clone(),
        decision: ReviewDecision::Rejected,
        reviewed_at: 200,
    };
    let original = Sources {
        records: vec![claim],
        reviews: vec![review.clone()],
        ..Default::default()
    };
    for source_id in [review.source_id.clone(), None] {
        for decision in [ReviewDecision::Rejected, ReviewDecision::Accepted] {
            let mut encoded = to_json(&original);
            let mut first = encoded.get("reviews").unwrap().as_arr().unwrap()[0].clone();
            first.set("source_id", opt_str(&source_id));
            let mut second = first.clone();
            second.set(
                "decision",
                match decision {
                    ReviewDecision::Accepted => "accepted",
                    ReviewDecision::Rejected => "rejected",
                }
                .into(),
            );
            second.set("reviewed_at", 100u64.into());
            encoded.set("reviews", Json::Arr(vec![first, second]));
            assert!(
                from_json(&encoded).is_err(),
                "duplicate exact review key: {source_id:?}, {decision:?}"
            );
        }
    }

    // Decisions about distinct claims remain independent, including retained
    // decisions whose source snapshot is no longer present.
    let mut distinct = original.clone();
    distinct.reviews.extend([
        SourceReview {
            source_id: Some("other-identifier".into()),
            ..review.clone()
        },
        SourceReview {
            source_id: None,
            ..review.clone()
        },
        SourceReview {
            source: "another-source".into(),
            ..review.clone()
        },
        SourceReview {
            entity: EntityRef {
                kind: EntityKind::Artist,
                key: "someone else".into(),
            },
            ..review.clone()
        },
        SourceReview {
            entity: EntityRef {
                kind: EntityKind::Release,
                key: "someone".into(),
            },
            ..review
        },
    ]);
    assert_eq!(from_json(&to_json(&distinct)).unwrap(), distinct);
    let mut legacy = to_json(&original);
    legacy.set("format_version", 1u32.into());
    let migrated = from_json(&legacy).unwrap();
    assert_eq!(migrated.records, original.records);
    assert!(migrated.reviews.is_empty());
}

#[test]
fn work_credit_scope_reaches_exact_performances_without_inventing_observations() {
    use crate::model::ScannedFile;
    use crate::tags::RawTags;
    use crate::user::UserData;
    use crate::{contributors, graph, query};

    const WORK: &str = "11111111-1111-1111-1111-111111111111";
    let files = [Some(WORK), Some(WORK), Some("different-work-id"), None]
        .into_iter()
        .enumerate()
        .map(|(index, work)| {
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "A Player"),
                ("albumartist", "A Player"),
                ("album", "An Album"),
            ] {
                tags.insert(key, value);
            }
            tags.insert("title", format!("Song {index}"));
            tags.insert("musicbrainz_recordingid", format!("recording-{index}"));
            if let Some(work) = work {
                tags.insert("musicbrainz_workid", work);
                tags.insert("work", "One shared title");
            }
            ScannedFile {
                path: format!("/music/{index}.flac"),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = crate::model::build(files, vec!["/music".into()], 1, &[]);
    let credit = |name: &str, role: &str| CreditLink {
        relation_id: Some(format!("manual-{role}")),
        role: role.into(),
        artist_mbid: format!("{role}-id"),
        artist_name: name.into(),
        role_id: None,
        direction: None,
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        over: None,
        order: None,
    };
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: "/music/0.flac".into(),
        source: "manual".into(),
        source_id: None,
        fetched_at: 7,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            credits: vec![credit("A Producer", "producer")],
            works: vec![WorkLink {
                mbid: WORK.into(),
                title: "A different source spelling".into(),
                credits: vec![credit("A Writer", "composer")],
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    let membership = SourceRecord {
        key: "/music/3.flac".into(),
        source: MUSICBRAINZ.into(),
        source_id: Some("recording-3".into()),
        fetched_at: 8,
        confidence: Confidence::matched(90),
        facts: Facts::Track(TrackFacts {
            works: vec![WorkLink {
                mbid: WORK.into(),
                title: "A sourced realization".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    };
    held.set(membership.clone());
    let personal = UserData::default();
    let query_ids = |held: &Sources, expression: &str| {
        let context = query::Context::new(&catalog, &personal, "local").with_sources(held);
        query::run(&query::parse(expression).unwrap(), &context)
    };
    let before = to_json(&held);
    let coverage = crate::credit_coverage::recordings(&catalog, &held);
    assert!(coverage.iter().all(|recording| {
        recording.status == crate::credit_coverage::CreditStatus::Waiting
            && recording.work_credits == 0
            && recording.recording_credits == 0
    }));
    let raw = held.credit_links(&catalog);
    assert_eq!(
        raw.len(),
        2,
        "one direct and one work assertion, each observed once"
    );
    assert!(
        raw.iter()
            .all(|link| link.recording_id == 0 && link.fetched_at == 7 && link.source == "manual")
    );
    let composer_edge = graph::edges(&catalog, &held)
        .into_iter()
        .find(|edge| edge.reference.kind == "credit:composer")
        .unwrap();
    assert_eq!(composer_edge.weight, 1);
    let edge_id = composer_edge.reference.id();
    assert_eq!(
        query_ids(&held, "composer:composer-id"),
        [0, 1],
        "a composition credit covers both exact local realizations"
    );
    assert_eq!(
        query_ids(&held, "producer:producer-id"),
        [0],
        "recording credits stay on their recording"
    );
    let writer = contributors::sourced(&catalog, &held)
        .into_iter()
        .find(|artist| artist.mbid == "composer-id")
        .unwrap();
    assert_eq!(writer.recording_ids, [0, 1]);
    assert_eq!(
        crate::credit_coverage::recordings(&catalog, &held),
        coverage
    );
    assert_eq!(
        to_json(&held),
        before,
        "projections do not rewrite source hosts"
    );

    held.set_review(SourceReview {
        entity: membership.entity(),
        source: MUSICBRAINZ.into(),
        source_id: membership.source_id.clone(),
        decision: ReviewDecision::Accepted,
        reviewed_at: 9,
    });
    assert_eq!(query_ids(&held, "composer:composer-id"), [0, 1, 3]);
    held.set_review(SourceReview {
        entity: membership.entity(),
        source: MUSICBRAINZ.into(),
        source_id: membership.source_id,
        decision: ReviewDecision::Rejected,
        reviewed_at: 10,
    });
    assert_eq!(query_ids(&held, "composer:composer-id"), [0, 1]);

    held.exclude_credit_in(&catalog, composer_edge.reference.clone(), 11);
    assert!(query_ids(&held, "composer:composer-id").is_empty());
    let excluded = graph::select(&graph::edges(&catalog, &held), &edge_id).unwrap();
    assert!(excluded.excluded && !excluded.trusted);
    assert_eq!(excluded.weight, 1);
    assert!(held.restore_credit_in(&catalog, &excluded.reference));
    let restored = from_json(&to_json(&held)).unwrap();
    assert_eq!(query_ids(&restored, "composer:composer-id"), [0, 1]);
    assert_eq!(
        graph::select(&graph::edges(&catalog, &restored), &edge_id)
            .unwrap()
            .weight,
        1
    );
    assert_eq!(restored.credit_links(&catalog).len(), 2);
    assert!(
        restored
            .credit_links(&catalog)
            .iter()
            .all(|link| link.recording_id == 0)
    );
    let mut rejected = restored.clone();
    rejected.set_review(SourceReview {
        entity: crate::user::EntityRef::new(EntityKind::Track, "/music/0.flac"),
        source: "manual".into(),
        source_id: None,
        decision: ReviewDecision::Rejected,
        reviewed_at: 12,
    });
    assert!(
        rejected
            .credit_links(&catalog)
            .iter()
            .all(|link| { !link.trusted && link.applicable_recording_ids.as_ref() == [0] })
    );
    assert!(query_ids(&rejected, "composer:composer-id").is_empty());
    let mut waiting = restored.clone();
    waiting
        .records
        .iter_mut()
        .find(|record| record.source == "manual")
        .unwrap()
        .key = "/not-in-this-catalog/host.flac".into();
    let waiting_before = to_json(&waiting);
    assert!(waiting.credit_links(&catalog).is_empty());
    assert!(query_ids(&waiting, "composer:composer-id").is_empty());
    assert!(contributors::sourced(&catalog, &waiting).is_empty());
    assert_eq!(
        to_json(&waiting),
        waiting_before,
        "an orphan claim waits without being rewritten"
    );
}

/// One membership, with only the fields a test cares about.
fn played(name: &str, began: Option<&str>, ended: Option<&str>, over: Option<bool>) -> Membership {
    Membership {
        relation_id: None,
        role_id: None,
        direction: None,
        mbid: format!("mbid-{name}"),
        name: name.to_string(),
        credited_as: None,
        side: Side::Player,
        kind: "member of band".to_string(),
        attributes: Vec::new(),
        began: began.map(str::to_string),
        ended: ended.map(str::to_string),
        over,
    }
}

#[test]
fn one_relation_read_from_either_end_is_two_different_lists() {
    // MusicBrainz states a membership once and returns it on both artists,
    // with a direction. Losing that direction does not lose data — it inverts
    // it, and puts Judas Priest among Ozzy Osbourne's members.
    let facts = ArtistFacts {
        members: vec![
            Membership {
                side: Side::Player,
                ..played("Tony Iommi", Some("1968"), None, Some(false))
            },
            Membership {
                side: Side::Group,
                ..played("Black Sabbath", Some("1968"), Some("1979"), Some(true))
            },
        ],
        ..Default::default()
    };
    let line_up: Vec<&str> = facts.line_up().map(|m| m.name.as_str()).collect();
    let bands: Vec<&str> = facts.bands().map(|m| m.name.as_str()).collect();
    assert_eq!(line_up, vec!["Tony Iommi"]);
    assert_eq!(bands, vec!["Black Sabbath"]);
}

#[test]
fn a_year_is_covered_only_where_the_source_dates_say_so() {
    // Three answers, not two. `None` is what the caller has to decide about,
    // and handing back `false` instead would be a silence dressed as knowledge.
    let ozzy = played("Ozzy", Some("1968"), Some("1979"), Some(true));
    assert_eq!(ozzy.covers(1970), Some(true), "Paranoid");
    assert_eq!(ozzy.covers(1967), Some(false), "before he joined");
    assert_eq!(ozzy.covers(1985), Some(false), "after he left");
    // A full date, not just a year, and the year is what decides.
    let dated = played("Dated", Some("1968-02-13"), Some("1979-04-27"), Some(true));
    assert_eq!(dated.covers(1979), Some(true));

    // Still in the band: the source says so with `ended: false`, and that is
    // an answer rather than a missing end date.
    let current = played("Tony", Some("1968"), None, Some(false));
    assert_eq!(current.covers(2020), Some(true));

    // Over, but nobody wrote down when. The end is somewhere and "somewhere"
    // cannot be compared with a year.
    let vague = played("Vague", Some("1968"), None, Some(true));
    assert_eq!(vague.covers(1970), None);
    // And nothing said at all is the same silence.
    assert_eq!(played("Quiet", Some("1968"), None, None).covers(1970), None);
    // No start date places nobody, whatever else is known.
    assert_eq!(
        played("Undated", None, Some("1979"), None).covers(1970),
        None
    );
}

#[test]
fn the_line_up_of_a_year_counts_who_it_cannot_place() {
    // The cross that makes the dates worth fetching — and the filter is
    // counted rather than applied in silence, because a reader who knows
    // somebody was there should be told why the page disagrees.
    let facts = ArtistFacts {
        members: vec![
            played("Ozzy Osbourne", Some("1968"), Some("1979"), Some(true)),
            played("Tony Iommi", Some("1968"), None, Some(false)),
            played("Ronnie James Dio", Some("1979"), Some("1982"), Some(true)),
            played("Somebody", Some("1968"), None, Some(true)),
            played("Nobody", None, None, None),
        ],
        ..Default::default()
    };
    let (placed, undated) = facts.line_up_in(1970);
    let named: Vec<&str> = placed.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(named, vec!["Ozzy Osbourne", "Tony Iommi"]);
    assert_eq!(undated, 2, "and the page says so rather than hiding them");

    // The same band, nine years later, is a different band.
    let (later, _) = facts.line_up_in(1980);
    let named: Vec<&str> = later.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(named, vec!["Tony Iommi", "Ronnie James Dio"]);
}

#[test]
fn a_membership_never_shows_an_end_the_source_did_not_give() {
    // A dash trailing into nothing claims the musician is still in the band,
    // which is a claim MusicBrainz makes with `ended: false` and not with a
    // missing date.
    assert_eq!(
        played("x", Some("1968"), Some("1979"), Some(true)).years(),
        "1968–1979"
    );
    assert_eq!(
        played("x", Some("1968"), None, Some(false)).years(),
        "1968–"
    );
    assert_eq!(
        played("x", Some("1968"), None, Some(true)).years(),
        "1968–?"
    );
    assert_eq!(played("x", Some("1968"), None, None).years(), "1968");
}

#[test]
fn two_spells_in_one_band_are_two_rows_and_not_a_span() {
    // Ozzy Osbourne was in Black Sabbath 1968–1979 and again from 1997.
    // Folding the two into one range would claim he was there in 1985.
    let facts = ArtistFacts {
        members: vec![
            played("Ozzy Osbourne", Some("1968"), Some("1979"), Some(true)),
            played("Ozzy Osbourne", Some("1997"), Some("2017"), Some(true)),
        ],
        ..Default::default()
    };
    assert_eq!(facts.line_up_in(1970).0.len(), 1);
    assert_eq!(facts.line_up_in(2013).0.len(), 1);
    assert!(facts.line_up_in(1985).0.is_empty(), "he had left");
}

#[test]
fn work_parent_scope_reaches_exact_performances_without_reassigning_observations() {
    use crate::model::ScannedFile;
    use crate::tags::RawTags;
    use crate::user::UserData;
    use crate::{graph, query};
    const CHILD: &str = "11111111-1111-4111-8111-111111111111";
    const PARENT: &str = "22222222-2222-4222-8222-222222222222";
    let files = [
        Some(CHILD),
        Some(CHILD),
        Some("33333333-3333-4333-8333-333333333333"),
        None,
        None,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, work)| {
        let mut tags = RawTags::default();
        tags.insert("artist", "An Orchestra");
        tags.insert("albumartist", "An Orchestra");
        tags.insert("album", "Performances");
        tags.insert("title", format!("Performance {index}"));
        tags.insert("musicbrainz_recordingid", format!("recording-{index}"));
        if let Some(work) = work {
            tags.insert("musicbrainz_workid", work);
            tags.insert("work", "One Movement");
        }
        ScannedFile {
            path: format!("/music/{index}.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    })
    .collect();
    let catalog = crate::model::build(files, vec!["/music".into()], 1, &[]);
    let mut held = Sources::default();
    for index in [0, 3, 4] {
        held.set(SourceRecord {
            key: format!("/music/{index}.flac"),
            source: MUSICBRAINZ.into(),
            source_id: Some(format!("recording-{index}")),
            fetched_at: 1,
            confidence: if index == 4 {
                Confidence::matched(42)
            } else {
                Confidence::Identified
            },
            facts: Facts::Track(TrackFacts {
                works: vec![WorkLink {
                    mbid: CHILD.into(),
                    title: "A Different Source Spelling".into(),
                    parents: if index == 0 {
                        vec![WorkParentLink {
                            mbid: PARENT.into(),
                            title: "Whole Symphony".into(),
                            relation_id: Some("part-relation".into()),
                            order: Some(2),
                            ..Default::default()
                        }]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }),
        });
    }
    let data = UserData::default();
    let selected = |sources: &Sources, mbid: &str| {
        let context = query::Context::new(&catalog, &data, "local").with_sources(sources);
        query::run(&query::parse(&format!("work:{mbid}")).unwrap(), &context)
    };
    let before = to_json(&held);
    let raw = held.work_parent_links(&catalog);
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].recording_id, 0);
    assert_eq!(raw[0].parent.order, Some(2));
    assert_eq!(selected(&held, CHILD), [0, 1, 3]);
    assert_eq!(
        selected(&held, PARENT),
        [0, 1, 3],
        "a work-to-work relationship covers every exact realization of its child"
    );
    assert_eq!(
        held.work_links(&catalog).len(),
        3,
        "recording-to-work observations are not propagated"
    );
    let edge = graph::edges(&catalog, &held)
        .into_iter()
        .find(|edge| edge.reference.kind == "part_of_work")
        .unwrap();
    assert_eq!(edge.weight, 1);
    let edge_id = edge.reference.id();
    assert_eq!(to_json(&held), before);
    let pending = held
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.entity.key == "/music/4.flac")
        .unwrap()
        .id;
    held.decide(&catalog, &pending, ReviewDecision::Accepted, 2)
        .unwrap();
    assert_eq!(selected(&held, PARENT), [0, 1, 3, 4]);
    held.decide(&catalog, &pending, ReviewDecision::Rejected, 3)
        .unwrap();
    assert_eq!(selected(&held, PARENT), [0, 1, 3]);
    let restored = from_json(&to_json(&held)).unwrap();
    assert_eq!(selected(&restored, PARENT), [0, 1, 3]);
    assert_eq!(
        graph::select(&graph::edges(&catalog, &restored), &edge_id)
            .unwrap()
            .weight,
        1
    );
    assert_eq!(restored.work_parent_links(&catalog).len(), 1);
    assert_eq!(restored.work_parent_links(&catalog)[0].recording_id, 0);
    let mut uncertain = restored.clone();
    uncertain
        .records
        .iter_mut()
        .find(|record| record.key == "/music/0.flac")
        .unwrap()
        .confidence = Confidence::matched(42);
    let proposal = uncertain
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.entity.key == "/music/0.flac")
        .unwrap()
        .id;
    assert!(selected(&uncertain, PARENT).is_empty());
    assert_eq!(
        selected(&uncertain, CHILD),
        [0, 1, 3],
        "parent trust does not remove canonical child membership"
    );
    uncertain
        .decide(&catalog, &proposal, ReviewDecision::Accepted, 4)
        .unwrap();
    assert_eq!(selected(&uncertain, PARENT), [0, 1, 3]);
    uncertain
        .decide(&catalog, &proposal, ReviewDecision::Rejected, 5)
        .unwrap();
    assert!(selected(&uncertain, PARENT).is_empty());
    let mut conflict = restored.clone();
    conflict
        .records
        .iter_mut()
        .find(|record| record.key == "/music/0.flac")
        .unwrap()
        .source_id = Some("conflicting-recording".into());
    assert!(selected(&conflict, PARENT).is_empty());
    let conflict_id = conflict
        .review_items(&catalog)
        .into_iter()
        .find(|item| item.entity.key == "/music/0.flac")
        .unwrap()
        .id;
    conflict
        .decide(&catalog, &conflict_id, ReviewDecision::Accepted, 6)
        .unwrap();
    assert_eq!(selected(&conflict, PARENT), [0, 1, 3]);
    let mut orphan = restored;
    orphan
        .records
        .iter_mut()
        .find(|record| record.key == "/music/0.flac")
        .unwrap()
        .key = "/not-in-this-catalog.flac".into();
    let orphan_before = to_json(&orphan);
    assert!(orphan.work_parent_links(&catalog).is_empty());
    assert!(selected(&orphan, PARENT).is_empty());
    assert_eq!(to_json(&orphan), orphan_before);
}

#[test]
fn complete_credit_snapshots_refuse_unreadable_relationship_collections() {
    let document = |kind: &str, facts: Json, version: u32| {
        let mut row = Json::obj();
        row.set("entity", format!("{kind}:/music/example").into());
        row.set("source", MUSICBRAINZ.into());
        row.set("confidence", "identified".into());
        row.set("facts", facts);
        let mut value = Json::obj();
        value.set("format_version", version.into());
        value.set("records", Json::Arr(vec![row]));
        value
    };
    let direct = [
        r#"{"credits":{}}"#,
        r#"{"credits":null}"#,
        r#"{"credits":[null]}"#,
        r#"{"credits":[{}]}"#,
        r#"{"credits":[{"role":"composer"}]}"#,
        r#"{"credits":[{"role":"composer","artist_mbid":42}]}"#,
    ];
    let work = [
        r#"{"works":{}}"#,
        r#"{"works":null}"#,
        r#"{"works":[null]}"#,
        r#"{"works":[{}]}"#,
        r#"{"works":[{"mbid":42}]}"#,
        r#"{"works":[{"mbid":"work-id","credits":{}}]}"#,
        r#"{"works":[{"mbid":"work-id","credits":null}]}"#,
        r#"{"works":[{"mbid":"work-id","credits":[null]}]}"#,
        r#"{"works":[{"mbid":"work-id","credits":[{}]}]}"#,
    ];
    for kind in ["track", "release"] {
        let malformed: Vec<_> = direct
            .iter()
            .chain(if kind == "track" {
                work.as_slice()
            } else {
                &[]
            })
            .collect();
        for body in malformed {
            let original = crate::json::parse(body).unwrap();
            let mut complete = original.clone();
            complete.set("relationships_complete", true.into());
            assert!(
                from_json(&document(kind, complete, SOURCES_FORMAT_VERSION)).is_err(),
                "{kind}: {body}"
            );
            // Incomplete older snapshots still use their established tolerant reader.
            assert!(from_json(&document(kind, original.clone(), SOURCES_FORMAT_VERSION)).is_ok());
            let mut incomplete = original;
            incomplete.set("relationships_complete", false.into());
            assert!(from_json(&document(kind, incomplete, SOURCES_FORMAT_VERSION)).is_ok());
        }
        for version in [1, SOURCES_FORMAT_VERSION] {
            for body in [
                r#"{}"#,
                r#"{"credits":[],"works":[]}"#,
                r#"{"credits":[{"role":"composer","artist_mbid":""}]}"#,
                r#"{"works":[{"mbid":"work-id"}]}"#,
                r#"{"works":[{"mbid":"work-id","credits":[]}]}"#,
                r#"{"works":[{"mbid":"work-id","credits":[{"role":"composer","artist_mbid":"artist-id"}]}]}"#,
            ] {
                let mut complete = crate::json::parse(body).unwrap();
                complete.set("relationships_complete", true.into());
                let held = from_json(&document(kind, complete, version)).unwrap();
                match &held.records[0].facts {
                    Facts::Track(facts) => assert!(facts.relationships_complete),
                    Facts::Release(facts) => assert!(facts.relationships_complete),
                    _ => panic!("the requested source kind remains available"),
                }
                assert_eq!(from_json(&to_json(&held)).unwrap(), held);
            }
        }
    }
}

#[test]
fn release_identity_reviews_compare_the_retained_musicbrainz_namespace() {
    use crate::model::builder::{ScannedFile, build};
    use crate::tags::RawTags;

    let mut tags = RawTags::default();
    for (key, value) in [
        ("artist", "Example Artist"),
        ("albumartist", "Example Artist"),
        ("album", "Example Album"),
        ("title", "Example Track"),
        ("musicbrainz_albumid", "edition-local"),
        ("musicbrainz_releasegroupid", "group-local"),
    ] {
        tags.insert(key, value);
    }
    let catalog = build(
        vec![ScannedFile {
            path: "/music/example/01.flac".into(),
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
    let entity = EntityRef::of(&catalog, EntityKind::Release, catalog.releases[0].id).unwrap();
    for (name, response, trusted) in [
        (
            "normal exact group",
            r#"{"id":"edition-local","release-group":{"id":"group-local"}}"#,
            true,
        ),
        ("exact edition fallback", r#"{"id":"edition-local"}"#, true),
        (
            "conflicting edition fallback",
            r#"{"id":"edition-other"}"#,
            false,
        ),
        (
            "conflicting group",
            r#"{"id":"edition-local","release-group":{"id":"group-other"}}"#,
            false,
        ),
    ] {
        let candidate =
            crate::musicbrainz::release(&crate::json::parse(response).unwrap()).unwrap();
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: MUSICBRAINZ.into(),
            source_id: Some(candidate.mbid),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Release(candidate.facts),
        });
        let before = to_json(&held);
        assert_eq!(
            held.is_trusted(&catalog, &held.records[0]),
            trusted,
            "{name}"
        );
        let items = held.review_items(&catalog);
        assert_eq!(items.is_empty(), trusted, "{name}");
        assert_eq!(
            to_json(&held),
            before,
            "reads preserve the evidence: {name}"
        );
        assert_eq!(to_json(&from_json(&before).unwrap()), before, "{name}");
        if !trusted {
            let item = &items[0];
            let expected_local = if name == "conflicting edition fallback" {
                "edition-local"
            } else {
                "group-local"
            };
            assert!(
                matches!(
                    &item.reason,
                    ReviewReason::IdentityConflict { local_id, .. } if local_id == expected_local
                ),
                "{name}: {:?}",
                item.reason
            );
            for (decision, expected) in [
                (ReviewDecision::Accepted, true),
                (ReviewDecision::Rejected, false),
            ] {
                held.decide(&catalog, &item.id, decision, 2).unwrap();
                assert_eq!(
                    held.is_trusted(&catalog, &held.records[0]),
                    expected,
                    "{name}"
                );
                assert_eq!(
                    held.records,
                    from_json(&before).unwrap().records,
                    "review leaves the claim intact"
                );
            }
            held.clear_review(&catalog, &item.id).unwrap();
            assert!(!held.is_trusted(&catalog, &held.records[0]), "{name}");
            assert_eq!(
                to_json(&held),
                before,
                "undo restores the precise prior decision state"
            );
        }
    }

    // Old source documents without an edition field still name the group.
    for (source_id, trusted) in [("group-local", true), ("group-other", false)] {
        let held = Sources {
            records: vec![SourceRecord {
                key: entity.key.clone(),
                source: MUSICBRAINZ.into(),
                source_id: Some(source_id.into()),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Release(ReleaseFacts::default()),
            }],
            ..Default::default()
        };
        let before = to_json(&held);
        assert_eq!(held.is_trusted(&catalog, &held.records[0]), trusted);
        assert_eq!(held.review_items(&catalog).is_empty(), trusted);
        assert_eq!(
            to_json(&held),
            before,
            "legacy reads do not infer an edition"
        );
    }
}

#[test]
fn artist_relationship_completion_is_optional_versioned_evidence_not_an_inferred_absence() {
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: "example".into(),
        source: MUSICBRAINZ.into(),
        source_id: Some("11111111-1111-4111-8111-111111111111".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            relationships_complete: true,
            ..Default::default()
        }),
    });
    let encoded = to_json(&held).to_string_pretty();
    assert_eq!(
        from_json(&crate::json::parse(&encoded).unwrap()).unwrap(),
        held
    );
    let invalid = encoded.replace(
        "\"relationships_complete\": true",
        "\"relationships_complete\": 1",
    );
    assert!(from_json(&crate::json::parse(&invalid).unwrap()).is_err());
    let legacy = encoded.replace("\"relationships_complete\": true,", "");
    let back = from_json(&crate::json::parse(&legacy).unwrap()).unwrap();
    let Facts::Artist(facts) = &back.records[0].facts else {
        panic!("artist");
    };
    assert!(!facts.relationships_complete);
}

#[test]
fn a_trusted_new_wikidata_identity_does_not_authenticate_a_previous_cached_biography() {
    let catalog = crate::model::Catalog::default();
    for kind in [EntityKind::Artist, EntityKind::Label] {
        let entity = EntityRef {
            kind,
            key: "example subject".into(),
        };
        let facts = |wikidata: Option<String>| match kind {
            EntityKind::Artist => Facts::Artist(ArtistFacts {
                wikidata,
                ..Default::default()
            }),
            EntityKind::Label => Facts::Label(LabelFacts {
                wikidata,
                ..Default::default()
            }),
            _ => unreachable!(),
        };
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: crate::wikipedia::SOURCE.into(),
            source_id: Some("Q111".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: facts(None),
        });
        assert!(
            held.is_trusted(
                &catalog,
                held.get(&entity, crate::wikipedia::SOURCE).unwrap()
            ),
            "retain independent legacy evidence when no contrary identity is known"
        );
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: MUSICBRAINZ.into(),
            source_id: Some("11111111-1111-4111-8111-111111111111".into()),
            fetched_at: 2,
            confidence: Confidence::Matched(95),
            facts: facts(Some("https://www.wikidata.org/wiki/Q222".into())),
        });
        assert!(
            held.is_trusted(
                &catalog,
                held.get(&entity, crate::wikipedia::SOURCE).unwrap()
            ),
            "an unreviewed proposal cannot invalidate another source"
        );
        let id = held
            .review_items(&catalog)
            .into_iter()
            .find(|row| row.source == MUSICBRAINZ)
            .unwrap()
            .id;
        held.decide(&catalog, &id, ReviewDecision::Accepted, 3)
            .unwrap();
        let before = held.clone();
        assert!(
            !held.is_trusted(
                &catalog,
                held.get(&entity, crate::wikipedia::SOURCE).unwrap()
            ),
            "Q111 is not the now-validated Q222"
        );
        assert_eq!(held, before, "stale evidence must be retained, not erased");
        held.records[1].facts = facts(Some("https://www.wikidata.org/wiki/Q111".into()));
        assert!(held.is_trusted(
            &catalog,
            held.get(&entity, crate::wikipedia::SOURCE).unwrap()
        ));
        held.records[1].facts = facts(Some("https://www.wikidata.org/wiki/Q222".into()));
        held.set_review(SourceReview {
            entity,
            source: crate::wikipedia::SOURCE.into(),
            source_id: Some("Q111".into()),
            decision: ReviewDecision::Accepted,
            reviewed_at: 4,
        });
        assert!(
            held.is_trusted(&catalog, &held.records[0]),
            "an explicit review of that exact Wikipedia claim retains priority"
        );
    }
}

#[test]
fn a_recent_discogs_biography_for_another_validated_label_is_not_trusted() {
    let catalog = crate::model::Catalog::default();
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: "example records".into(),
        source: MUSICBRAINZ.into(),
        source_id: Some("11111111-1111-4111-8111-111111111111".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            discogs: Some(crate::discogs::label_url(12345)),
            ..Default::default()
        }),
    });
    held.set(SourceRecord {
        key: "example records".into(),
        source: crate::discogs::SOURCE.into(),
        source_id: Some("99999".into()),
        fetched_at: crate::clock::now_seconds(),
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    let before = held.clone();
    assert!(
        !held.is_trusted(&catalog, &held.records[1]),
        "Discogs 99999 is not the validated linked label 12345"
    );
    assert_eq!(held, before);
    held.records[1].source_id = Some("12345".into());
    assert!(held.is_trusted(&catalog, &held.records[1]));
}
