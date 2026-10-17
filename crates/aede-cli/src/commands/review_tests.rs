use super::*;

#[test]
fn review_cards_link_to_the_source_identity_not_a_different_musicbrainz_entity() {
    const EDITION: &str = "11111111-1111-1111-1111-111111111111";
    const GROUP: &str = "22222222-2222-2222-2222-222222222222";
    let catalog = Catalog::default();

    // Both shapes are supported lookup answers: an edition usually carries a
    // distinct group, but an answer without one retains its edition identity.
    for (group, route, identifier) in [
        (Some(GROUP), "release-group", GROUP),
        (None, "release", EDITION),
    ] {
        let mut response = aede_core::json::Json::obj();
        response.set("id", EDITION.into());
        response.set("title", "A Reviewed Album".into());
        response.set("relations", aede_core::json::Json::Arr(Vec::new()));
        if let Some(group) = group {
            let mut details = aede_core::json::Json::obj();
            details.set("id", group.into());
            details.set("title", "A Reviewed Album".into());
            response.set("release-group", details);
        }
        let candidate = aede_core::musicbrainz::release(&response).unwrap();
        assert_eq!(candidate.mbid, identifier);
        assert_eq!(candidate.facts.edition_mbid.as_deref(), Some(EDITION));
        let record = SourceRecord {
            key: "reviewed album".into(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(candidate.mbid),
            fetched_at: 1,
            confidence: sources::Confidence::Matched(80),
            facts: Facts::Release(candidate.facts),
        };
        let mut held = Sources::default();
        held.set(record.clone());
        let item = held.review_items(&catalog).pop().unwrap();
        let mut output = Vec::new();
        render_card(&mut output, &catalog, &record, &item, 1, 1).unwrap();
        let rendered = String::from_utf8(output).unwrap();
        let expected = format!("https://musicbrainz.org/{route}/{identifier}");
        assert!(rendered.contains(&expected), "{rendered}");
        if group.is_some() {
            assert!(!rendered.contains(&format!("https://musicbrainz.org/release/{GROUP}")));
            assert!(
                !rendered.contains(&format!("https://musicbrainz.org/release-group/{EDITION}"))
            );
        }
    }

    // The adjacent proposal cards keep the identifier's own entity type.
    for (facts, route) in [
        (Facts::Artist(Default::default()), "artist"),
        (Facts::Track(Default::default()), "recording"),
        (Facts::Label(Default::default()), "label"),
    ] {
        let record = SourceRecord {
            key: "reviewed entity".into(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(GROUP.into()),
            fetched_at: 1,
            confidence: sources::Confidence::Matched(80),
            facts,
        };
        let mut held = Sources::default();
        held.set(record.clone());
        let item = held.review_items(&catalog).pop().unwrap();
        let mut output = Vec::new();
        render_card(&mut output, &catalog, &record, &item, 1, 1).unwrap();
        let rendered = String::from_utf8(output).unwrap();
        assert!(
            rendered.contains(&format!("https://musicbrainz.org/{route}/{GROUP}")),
            "{rendered}"
        );
    }
}

#[test]
fn acoustid_review_cards_link_to_the_recording_retained_from_the_lookup() {
    const RECORDING: &str = "11111111-1111-1111-1111-111111111111";
    const ACOUSTID_TRACK: &str = "22222222-2222-2222-2222-222222222222";
    let response = aede_core::json::parse(
        r#"{"status":"ok","results":[{
          "id":"22222222-2222-2222-2222-222222222222","score":0.97,
          "recordings":[{"id":"11111111-1111-1111-1111-111111111111",
            "title":"A Reviewed Performance","artists":[{"name":"A Reviewed Artist"}]}]
        }]}"#,
    )
    .unwrap();
    let heard = aede_core::acoustid::best(&response).unwrap();
    assert_eq!(heard.recording, RECORDING);
    assert_ne!(heard.recording, ACOUSTID_TRACK);

    // Identification retains the MusicBrainz recording, not the service's
    // outer result ID. The review must address that retained entity.
    let record = SourceRecord {
        key: "/music/reviewed/01.flac".into(),
        source: aede_core::acoustid::SOURCE.into(),
        source_id: Some(heard.recording.clone()),
        fetched_at: 1,
        confidence: sources::Confidence::Matched(97),
        facts: Facts::Track(aede_core::sources::TrackFacts {
            recording: Some(heard.recording),
            score: Some(97),
            title: heard.title,
            artists: heard.artists,
            album: heard.album,
            ..Default::default()
        }),
    };
    let no_identity = SourceRecord {
        source_id: None,
        facts: Facts::Track(Default::default()),
        ..record.clone()
    };
    let unknown_source = SourceRecord {
        source: "unrecognized-service".into(),
        ..record.clone()
    };
    let expected_url = format!("https://musicbrainz.org/recording/{RECORDING}");
    let catalog = Catalog::default();
    for (record, expected) in [
        (record, Some(expected_url)),
        (no_identity, None),
        (unknown_source, None),
    ] {
        let mut held = Sources::default();
        held.set(record.clone());
        let item = held.review_items(&catalog).pop().unwrap();
        let mut output = Vec::new();
        render_card(&mut output, &catalog, &record, &item, 1, 1).unwrap();
        let rendered = String::from_utf8(output).unwrap();
        match expected {
            Some(url) => assert!(rendered.contains(&url), "{rendered}"),
            None => assert!(!rendered.contains("https://"), "{rendered}"),
        }
        assert!(
            !rendered.contains("https://acoustid.org/track/"),
            "{rendered}"
        );
        assert!(!rendered.contains(ACOUSTID_TRACK), "{rendered}");
    }
}
