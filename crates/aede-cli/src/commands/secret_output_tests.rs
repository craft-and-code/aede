use super::*;

#[test]
fn credentials_are_redacted_in_raw_trimmed_and_encoded_service_errors() {
    let text = "  aB/c+  ; aB/c+; client=aB%2Fc%2B; client=aB%2fc%2b";
    let redacted = redact(text.into(), &["  aB/c+  ".into()]);
    assert_eq!(
        redacted,
        "[redacted]; [redacted]; client=[redacted]; client=[redacted]"
    );
}

#[test]
fn overlapping_credentials_mask_the_entire_original_span_in_either_order() {
    for secrets in [
        vec![
            "fictional-prefix".into(),
            "fictional-prefix-private-tail".into(),
        ],
        vec![
            "fictional-prefix-private-tail".into(),
            "fictional-prefix".into(),
        ],
        vec!["fictional-prefix".into(), "prefix-private-tail".into()],
        vec!["aba".into()],
    ] {
        let text = if secrets.len() == 1 {
            "ababa"
        } else {
            "fictional-prefix-private-tail"
        };
        assert_eq!(redact(text.into(), &secrets), "[redacted]");
    }
}

#[test]
fn redaction_markers_are_never_reprocessed_as_credentials() {
    assert_eq!(
        redact(
            "prefix[redacted]suffix".into(),
            &["acted".into(), "prefix[redacted]suffix".into()]
        ),
        "[redacted]"
    );
}
