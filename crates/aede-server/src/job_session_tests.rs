//! Exercise the body-read boundary with real account sessions, without sockets or timing sleeps.
use super::*;
use crate::admin_session_test_support::signed_request;
use crate::{accounts_test_support::Fixture, test_support::test_runtime};
#[test]
fn logout_while_a_biography_request_body_arrives_cannot_enqueue_work() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let request = signed_request(
            &fixture,
            r#"{"artist":"artist:ac/dc","summaries":true,"yes":true}"#,
        );
        let response = fetch_route(State(fixture.0.clone()), request).await;
        let failure =
            response.expect_err("revoked login must be rejected before accepting the task");
        assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
        assert!(fixture.0.jobs.data.lock().unwrap().tasks.is_empty());
    });
}
#[test]
fn logout_during_scan_admission_cannot_enqueue_or_modify_a_task() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let request = signed_request(&fixture, r#"{"full":true}"#);
        let failure = scan_route(State(fixture.0.clone()), request)
            .await
            .expect_err("revoked scan admission");
        assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
        assert!(fixture.0.jobs.data.lock().unwrap().tasks.is_empty());
    });
}
#[test]
fn logout_during_task_body_reads_cannot_expose_results_or_cancel_accepted_work() {
    test_runtime().block_on(async {
        for operation in ["list", "get", "cancel"] {
            let fixture = Fixture::new();
            let cancellation = fixture.0.jobs.insert(1, "identification").unwrap();
            let request = signed_request(&fixture, "");
            let outcome = match operation {
                "list" => task_list_route(State(fixture.0.clone()), request)
                    .await
                    .map(|_| ()),
                "get" => task_route(State(fixture.0.clone()), Path("1".into()), request)
                    .await
                    .map(|_| ()),
                _ => cancel_route(State(fixture.0.clone()), Path("1".into()), request)
                    .await
                    .map(|_| ()),
            };
            assert_eq!(
                outcome.expect_err("task access after logout").status,
                StatusCode::UNAUTHORIZED,
                "{operation}"
            );
            assert!(
                !cancellation.load(Ordering::Acquire),
                "logout does not stop already accepted work"
            );
        }
    });
}
