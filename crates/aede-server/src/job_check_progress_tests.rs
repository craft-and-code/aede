use super::*;
use crate::jobs::{JobRegistry, JobRequest};
fn output(files: serde_json::Value, failed: bool) -> JobOutput {
    JobOutput {
        stdout: serde_json::json!({"version":1,"files":files}).to_string(),
        exit_code: i32::from(failed),
        ..Default::default()
    }
}
fn row(path: &str) -> serde_json::Value {
    serde_json::json!({"path":path,"status":"ok","verifier":"Rust"})
}
#[test]
fn snapshots_accept_only_bounded_exact_unique_and_consistent_completed_files() {
    let expected: Vec<String> = vec!["/fixture/first.flac".into(), "/fixture/second.flac".into()];
    let first = output(serde_json::json!([row(&expected[0])]), false);
    assert_eq!(snapshot_count(&first, &expected), Some(1));
    for invalid in [
        output(serde_json::json!([]), false),
        output(serde_json::json!([row("/foreign.flac")]), false),
        output(
            serde_json::json!([row(&expected[0]), row(&expected[0])]),
            false,
        ),
        JobOutput {
            output_truncated: true,
            ..first.clone()
        },
        JobOutput {
            exit_code: 1,
            ..first.clone()
        },
        JobOutput {
            stderr: "unexpected".into(),
            ..first.clone()
        },
        JobOutput {
            stdout: "x".repeat(MAX_OUTPUT + 1),
            ..first.clone()
        },
    ] {
        assert_eq!(snapshot_count(&invalid, &expected), None);
    }
}
#[test]
fn snapshots_are_live_monotone_and_cannot_change_terminal_or_other_tasks() {
    let mut state = crate::test_support::sample_state();
    state.jobs = Arc::new(JobRegistry::default());
    state.jobs.insert(1, "album_decode_test").unwrap();
    state.jobs.insert(2, "album_decode_test").unwrap();
    state.jobs.running(1).unwrap();
    let check = bind(
        &state,
        1,
        CheckRequest::from(vec!["/first.flac".into(), "/second.flac".into()]),
    );
    check.report(output(serde_json::json!([row("/first.flac")]), false));
    let first = state.jobs.get(1).unwrap();
    assert_eq!(first.status, Status::Running);
    assert!(first.result.is_none());
    assert!(first.partial_result.is_some());
    assert!(state.jobs.get(2).unwrap().partial_result.is_none());
    check.report(output(
        serde_json::json!([row("/first.flac"), row("/second.flac")]),
        false,
    ));
    check.report(output(serde_json::json!([row("/first.flac")]), false));
    assert_eq!(
        snapshot_count(
            state.jobs.get(1).unwrap().partial_result.as_ref().unwrap(),
            &check.files
        ),
        Some(2)
    );
    state.jobs.finish(1, Ok(JobOutput::default())).unwrap();
    check.report(output(serde_json::json!([row("/first.flac")]), false));
    assert_eq!(state.jobs.get(1).unwrap().status, Status::Completed);
    let _ = JobRequest::Check(check);
}
