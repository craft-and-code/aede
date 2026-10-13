use super::*;

#[test]
fn precise_recognized_tags_override_conflicting_inherited_tags() {
    let selector = TagSelector::new(&crate::presets());
    let track = vec!["calm".into()];
    let album = vec!["party".into()];
    let artist = vec!["energy".into()];
    assert_eq!(selector.scope([&track, &album, &artist]), TagScope::Track);
    assert_eq!(selector.scope([&[], &album, &artist]), TagScope::Album);
    assert_eq!(selector.scope([&[], &[], &artist]), TagScope::Artist);
}

#[test]
fn unrelated_tags_do_not_suppress_mood_or_genre_fallback() {
    let selector = TagSelector::new(&crate::presets());
    let unrelated = vec!["music".into(), "not calm".into()];
    let artist = vec!["energy".into()];
    assert_eq!(selector.scope([&unrelated, &[], &artist]), TagScope::Artist);
    assert_eq!(selector.scope([&unrelated, &[], &[]]), TagScope::Genre);
}

#[test]
fn custom_rule_vocabulary_is_respected_without_implicit_presets() {
    let rule = Rule {
        id: "winter".into(),
        name: "Winter".into(),
        tags: vec!["snow".into()],
        genres: vec![],
    };
    let selector = TagSelector::new(&[rule]);
    assert_eq!(
        selector.scope([&["snow".into()], &[], &[]]),
        TagScope::Track
    );
    assert_eq!(
        selector.scope([&["calm".into()], &[], &[]]),
        TagScope::Genre
    );
}
