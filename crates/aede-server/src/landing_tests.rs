use crate::{routing::router, test_support::sample_state};
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{StatusCode, header},
};
use tower::ServiceExt;

#[test]
fn browser_entry_is_static_and_keeps_unknown_api_errors() {
    crate::test_support::test_runtime().block_on(async {
        let router = router(sample_state(), "127.0.0.1:8787".parse().unwrap());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Host", "127.0.0.1:8787")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("default-src 'none'")
        );
        let body = String::from_utf8(to_bytes(response.into_body(), 8192).await.unwrap().to_vec())
            .unwrap();
        assert!(body.contains("Phémios"));
        assert!(!body.contains("Back in Black"));
        assert!(!body.contains("/music/"));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/unknown")
                    .header("Host", "127.0.0.1:8787")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    });
}
