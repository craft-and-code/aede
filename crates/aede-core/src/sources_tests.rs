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
        label_mbid: None,
        cover_art: None,
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
            discography: vec![KnownRelease {
                mbid: "c9fdb94c".to_string(),
                title: "Kind of Blue".to_string(),
                first_released: Some("1959-08-17".to_string()),
                primary_type: Some("Album".to_string()),
                secondary_types: vec![],
            }],
            members: vec![Membership {
                mbid: "b1a9c0e9".to_string(),
                name: "The Miles Davis Quintet".to_string(),
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
    assert_eq!(artist.discography.len(), 1, "the nameless row was dropped");
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

/// One membership, with only the fields a test cares about.
fn played(name: &str, began: Option<&str>, ended: Option<&str>, over: Option<bool>) -> Membership {
    Membership {
        mbid: format!("mbid-{name}"),
        name: name.to_string(),
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
