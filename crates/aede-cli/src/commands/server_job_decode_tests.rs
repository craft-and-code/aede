use super::*;
#[test]
fn incomplete_lines_never_publish_and_complete_rows_keep_the_ordinary_json_capture() {
    let request =
        aede_server::CheckRequest::from(vec!["/first.flac".into(), "/second.flac".into()]);
    let mut snapshots = Snapshots::new(Some(request), vec![]);
    let prefix = b"{\"version\":1,\"files\":[\n{\"path\":\"/first.flac\",\"status\":\"ok\",\"verifier\":\"Rust\"}";
    snapshots.observe(prefix, false);
    assert!(snapshots.files.is_empty());
    let mut bytes = prefix.to_vec();
    bytes.push(b'\n');
    snapshots.observe(&bytes, false);
    assert_eq!(snapshots.files.len(), 1);
    snapshots.observe(&bytes, false);
    assert_eq!(snapshots.files.len(), 1);
    bytes.extend_from_slice(b",{\"path\":\"/second.flac\",\"status\":\"ko\",\"verifier\":\"Failed\",\"message\":\"MD5 mismatch\"}\n]}\n");
    snapshots.observe(&bytes, false);
    assert_eq!(snapshots.files.len(), 2);
    assert!(json::parse(std::str::from_utf8(&bytes).unwrap()).is_ok());
}
#[test]
fn truncated_output_stops_snapshot_publication() {
    let request = aede_server::CheckRequest::from(vec!["/first.flac".into()]);
    let mut snapshots = Snapshots::new(Some(request), vec![]);
    snapshots.observe(b"{\"version\":1,\"files\":[\n", true);
    assert!(snapshots.check.is_none());
    assert!(snapshots.files.is_empty());
}

#[test]
fn private_snapshots_redact_reflected_credentials_before_publication() {
    let request = aede_server::CheckRequest::from(vec!["/first.flac".into()]);
    let mut snapshots = Snapshots::new(Some(request), vec!["private-fixture-token".into()]);
    snapshots.observe(b"{\"version\":1,\"files\":[\n{\"path\":\"/first.flac\",\"status\":\"ko\",\"verifier\":\"Rust\",\"message\":\"private-fixture-token\"}\n", false);
    let output = snapshots.output();
    assert!(!output.stdout.contains("private-fixture-token"));
    assert!(output.stdout.contains("[redacted]"));
    assert_eq!(output.exit_code, 1);
    assert!(!output.output_truncated);
}

#[test]
fn bytewise_unicode_rows_and_incomplete_cancelled_suffix_never_invent_a_verdict() {
    let paths = vec![
        "/fixture/écho\n\"un\".flac".to_string(),
        "/fixture/second.flac".to_string(),
    ];
    let first = encoded_verdict(&paths[0]);
    let bytes = format!("{{\"version\":1,\"files\":[\n{first}\n,{{\"path\":\"/fixture/second.flac\",\"status\":\"ok\"").into_bytes();
    let mut snapshots =
        Snapshots::new(Some(aede_server::CheckRequest::from(paths.clone())), vec![]);
    for end in 1..=bytes.len() {
        snapshots.observe(&bytes[..end], false);
    }
    assert_eq!(snapshots.files.len(), 1);
    let report = json::parse(&snapshots.output().stdout).unwrap();
    assert_eq!(
        report.get("files").unwrap().as_arr().unwrap()[0]
            .get("path")
            .unwrap()
            .as_str(),
        Some(paths[0].as_str())
    );
    // Repeated EOF/cancellation observations still cannot promote the unfinished second row.
    snapshots.observe(&bytes, false);
    assert_eq!(snapshots.files.len(), 1);
}
fn encoded_verdict(path: &str) -> String {
    let mut value = Json::obj();
    value.set("path", Json::Str(path.into()));
    value.set("status", Json::Str("ok".into()));
    value.set("verifier", Json::Str("Rust".into()));
    value.to_string_compact()
}

#[test]
fn json_escaped_credentials_cannot_be_recovered_from_progress_or_final_capture() {
    let secret = "fictional-\"quoted\\credential";
    let mut file = Json::obj();
    file.set("path", Json::Str("/fixture.flac".into()));
    file.set("status", Json::Str("ko".into()));
    file.set("verifier", Json::Str("Failed".into()));
    file.set("message", Json::Str(format!("decoder reflected {secret}")));
    let bytes = format!(
        "{{\"version\":1,\"files\":[\n{}\n]}}\n",
        file.to_string_compact()
    )
    .into_bytes();
    let secrets = vec![secret.to_string()];
    let mut snapshots = Snapshots::new(
        Some(aede_server::CheckRequest::from(vec![
            "/fixture.flac".into(),
        ])),
        secrets.clone(),
    );
    snapshots.observe(&bytes, false);
    let final_output = super::super::redact_capture((bytes, false), &secrets).0;
    for output in [snapshots.output().stdout, final_output] {
        let parsed = json::parse(&output).unwrap();
        let message = parsed.get("files").unwrap().as_arr().unwrap()[0]
            .get("message")
            .unwrap()
            .as_str()
            .unwrap();
        assert!(
            !message.contains(secret),
            "JSON decoding recovered a configured secret"
        );
        assert!(message.contains("[redacted]"));
    }
}
