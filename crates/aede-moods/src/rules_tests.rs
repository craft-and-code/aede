use super::*;
#[test]
fn atmosphere_rules_are_explicit_exact_and_explainable() {
    let rule = &presets()[0];
    assert!(rule.valid());
    assert!(rule.matches(&["not calm".into()], &[]).is_none());
    assert_eq!(
        rule.matches(&["relaxation".into()], &[]),
        Some(Match {
            tag: Some("relaxation".into()),
            genre: None
        })
    );
    assert_eq!(
        rule.matches(&[], &["ambient".into()]),
        Some(Match {
            tag: None,
            genre: Some("ambient".into())
        })
    );
    assert!(rule.matches(&[], &[]).is_none());
}
#[test]
fn empty_and_oversized_custom_rules_are_refused() {
    let mut rule = presets().remove(0);
    rule.tags.clear();
    rule.genres.clear();
    assert!(!rule.valid());
    rule.tags = vec!["calm".into(); 33];
    assert!(!rule.valid());
}

#[test]
fn measured_recipe_ids_cannot_be_used_by_semantic_rules() {
    let mut rule = presets().remove(0);
    for recipe in audio_recipes() {
        rule.id = recipe.id.into();
        assert!(
            !rule.valid(),
            "{} belongs to the measured recipe",
            recipe.id
        );
    }
    for id in ["calm", "winter", "@aede/audio/custom"] {
        rule.id = id.into();
        assert!(rule.valid(), "{id} remains a configurable semantic rule");
    }
}
