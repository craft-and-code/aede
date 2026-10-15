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
