//! Tests for [`super`], split out of `sources.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;

fn release(primary: &str, label: &str) -> Facts {
    Facts::Release(ReleaseFacts {
        primary_type: Some(primary.to_string()),
        secondary_types: vec!["Live".to_string()],
        first_released: Some("1973".to_string()),
        label: Some(label.to_string()),
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
            area: Some("United States".to_string()),
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
            summary: Some(Prose {
                text: "An American trumpeter and bandleader.".to_string(),
                url: "https://en.wikipedia.org/wiki/Miles_Davis".to_string(),
                lang: "en".to_string(),
                licence: "CC BY-SA 4.0".to_string(),
            }),
        }),
    });

    let text = to_json(&sources).to_string_pretty();
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
    assert_eq!(back, sources, "written and read back are the same layer");
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
