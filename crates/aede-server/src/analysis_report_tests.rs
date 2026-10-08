use super::*;
use crate::test_support::{sample_state, start_server, test_runtime};
#[test]
fn report_route_rejects_paths_and_wrong_entity_kinds_and_exports_original_entries() {
    test_runtime().block_on(async {
        let state = sample_state();
        let track_ref;
        {
            let mut held = state.catalog.write().await;
            let catalog = held.as_mut().unwrap();
            track_ref = reference(catalog, EntityKind::Track, 0).unwrap();
            let file = &catalog.files[0];
            catalog.analyses.push(aede_core::analysis::FileAnalysis {
                path: file.path.clone(),
                source: "flaccompagnon".into(),
                source_version: 1,
                size_bytes: file.size,
                modified_unix: file.mtime,
                source_data: Some(
                    aede_core::json::parse(r#"{"path":"/music/album/01.flac","unknown":true}"#)
                        .unwrap(),
                ),
                ..Default::default()
            });
        }
        let (address, server) = start_server(state).await;
        let encoded: String = track_ref
            .bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect();
        let response = crate::test_support::request(
            address,
            &format!("/api/v1/analysis/report?ref={encoded}"),
        );
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("attachment; filename=\"flaccompagnon-track.json\""));
        let parsed: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(parsed["report"]["files"][0]["unknown"], true);
        for query in [
            "ref=%2Fprivate%2Freport.json",
            "ref=artist%3Aac%2Fdc",
            "ref=track%3Amissing",
            "ref=track%3Amissing&path=%2Fprivate",
        ] {
            let response =
                crate::test_support::request(address, &format!("/api/v1/analysis/report?{query}"));
            assert!(!response.starts_with("HTTP/1.1 200"), "{response}");
        }
        server.abort();
    });
}
