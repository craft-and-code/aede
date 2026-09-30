use super::*;

#[test]
fn a_musicbrainz_discogs_link_yields_only_a_numeric_label_id() {
    assert_eq!(label_id("https://www.discogs.com/label/33088"), Some(33088));
    assert_eq!(
        label_id("https://www.discogs.com/fr/label/33088-Roadracer-Records"),
        Some(33088)
    );
    for url in [
        "https://evil.example/label/33088",
        "https://www.discogs.com/artist/33088",
        "https://www.discogs.com/label/33088evil",
        "https://www.discogs.com/label/0",
    ] {
        assert_eq!(label_id(url), None, "{url}");
    }
}

#[test]
fn a_label_profile_keeps_its_source_and_refuses_another_identity() {
    let response = crate::json::parse(
        r#"{"id":33088,"name":"Roadracer Records","profile":"Label code: LC 9321."}"#,
    )
    .expect("fixture");
    let prose = profile(&response, 33088, "Roadracer Records").expect("profile");
    assert_eq!(prose.text, "Label code: LC 9321.");
    assert_eq!(prose.url, "https://www.discogs.com/label/33088");
    assert_eq!(prose.licence, "CC0");
    assert!(profile(&response, 42, "Roadracer Records").is_none());
    assert!(profile(&response, 33088, "Another Label").is_none());
}

#[test]
fn a_cached_profile_expires_before_six_hours() {
    let fetched = 100_000;
    assert!(fresh(fetched, fetched + MAX_AGE_SECONDS - 1));
    assert!(!fresh(fetched, fetched + MAX_AGE_SECONDS));
    assert!(!fresh(fetched + 1, fetched));
    assert!(!fresh(0, fetched));
}

#[test]
fn profile_markup_names_linked_labels_and_removes_presentation_tags() {
    let raw = "Originally a Dutch label. [b]Label Code: LC 9231[/b] Please use [l30552] and/or [l261823]. Counterfeits should be attached to [l231268].";
    assert_eq!(referenced_labels(raw), vec![30552, 261823, 231268]);
    let names = std::collections::BTreeMap::from([
        (30552, "First Label".to_string()),
        (261823, "Second Label".to_string()),
        (231268, "Counterfeit Label".to_string()),
    ]);
    assert_eq!(
        render_profile(raw, &names, &Default::default()),
        "Originally a Dutch label. Label Code: LC 9231 Please use First Label and/or Second Label. Counterfeits should be attached to Counterfeit Label."
    );
    assert_eq!(
        render_profile(
            "[l=Warp Records] [l999] [Stereo]",
            &names,
            &Default::default()
        ),
        "Warp Records Label #999 [Stereo]"
    );
}

#[test]
fn malformed_markup_is_left_as_text_and_label_identity_is_checked() {
    assert_eq!(
        render_profile(
            "[l12x] [b unclosed",
            &Default::default(),
            &Default::default()
        ),
        "[l12x] [b unclosed"
    );
    let response = crate::json::parse(r#"{"id":30552,"name":"First Label"}"#).expect("fixture");
    assert_eq!(label_name(&response, 30552).as_deref(), Some("First Label"));
    assert!(label_name(&response, 261823).is_none());
}

#[test]
fn profile_markup_also_names_artists_without_a_local_catalog_entry() {
    let raw = "Founded by [a1258936]; [a=De La Soul] also appears here.";
    assert_eq!(referenced_artists(raw), vec![1258936]);
    let artists = std::collections::BTreeMap::from([(1258936, "Founder".to_string())]);
    assert_eq!(
        render_profile(raw, &Default::default(), &artists),
        "Founded by Founder; De La Soul also appears here."
    );
    assert_eq!(
        artist_api_url(1258936),
        "https://api.discogs.com/artists/1258936"
    );
    let response = crate::json::parse(r#"{"id":1258936,"name":"Founder"}"#).expect("fixture");
    assert_eq!(artist_name(&response, 1258936).as_deref(), Some("Founder"));
    assert!(artist_name(&response, 42).is_none());
}
