use super::*;
#[test]
fn job_credentials_are_bounded_and_debug_never_contains_their_values() {
    let keys: FetchServiceKeys = serde_json::from_str(
        r#"{"fanarttv":"fictional-fanart-key","acoustid":"fictional-acoustid-key"}"#,
    )
    .unwrap();
    assert!(keys.valid());
    let debug = format!("{keys:?}");
    assert!(!debug.contains("fictional-fanart-key"));
    assert!(!debug.contains("fictional-acoustid-key"));
    assert!(debug.contains("true"));
    for key in [
        "".into(),
        " \t".into(),
        "unsafe\nvalue".into(),
        "x".repeat(1025),
    ] {
        assert!(
            !FetchServiceKeys {
                fanarttv: Some(key),
                ..Default::default()
            }
            .valid()
        );
    }
    assert!(serde_json::from_str::<FetchServiceKeys>(r#"{"AEDE_ADMIN_TOKEN":"bad"}"#).is_err());
}
