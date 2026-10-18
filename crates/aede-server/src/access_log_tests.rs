use super::*;
use axum::{
    Router,
    body::Body,
    http::{Method, StatusCode},
    middleware,
    routing::get,
};
use tower::ServiceExt;

#[test]
fn summaries_keep_route_templates_and_status_without_user_supplied_secrets() {
    crate::test_support::test_runtime().block_on(async {
        let (tx, rx) = mpsc::sync_channel(4);
        let router = Router::new()
            .route("/api/example/:id", get(|| async { StatusCode::NO_CONTENT }))
            .layer(middleware::from_fn(label))
            .layer(middleware::from_fn_with_state(AccessLog(tx), record));
        for (path, status) in [
            ("/api/example/private-id?password=secret", 204),
            ("/secret-token?key=private", 404),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("Authorization", "Bearer secret")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
            let summary = serde_json::to_value(rx.try_recv().unwrap()).unwrap();
            assert_eq!(summary["status"], status);
            assert_eq!(
                summary["route"],
                if status == 204 {
                    "/api/example/:id"
                } else {
                    "unmatched/rejected"
                }
            );
            assert!(!summary.to_string().contains("private"));
            assert!(!summary.to_string().contains("secret"));
        }
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/example/name")
                    .body(Body::from("password=secret"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(rx.try_recv().unwrap().status, 405);
    });
}
#[test]
fn saturated_diagnostic_queue_does_not_block_requests() {
    crate::test_support::test_runtime().block_on(async {
        let (tx, _rx) = mpsc::sync_channel(1);
        let router = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(AccessLog(tx), record));
        for _ in 0..3 {
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                router.clone().oneshot(Request::new(Body::empty())),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    });
}
