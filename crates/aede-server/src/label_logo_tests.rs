use super::*;
use crate::accounts_test_support::Fixture;
use aede_core::model::Label;
use aede_core::sources::{Confidence, Facts, LabelFacts, SourceRecord, Sources};

const MBID: &str = "11111111-1111-4111-8111-111111111111";

fn fixture(mbid: Option<&str>) -> Fixture {
    let fixture = Fixture::new();
    {
        let mut guard = fixture.0.catalog.blocking_write();
        let catalog = guard.as_mut().unwrap();
        catalog.labels = vec![Label {
            id: 0,
            key: "example-label".into(),
            name: "Example Label".into(),
            mbid: mbid.map(str::to_string),
        }];
        catalog.releases[0].label_ids = vec![0];
    }
    fixture
}

fn write_logo(fixture: &Fixture) -> PathBuf {
    let folder = coverart::asset_destination(&fixture.0.data_dir, EntityKind::Label, MBID).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("logo.png");
    std::fs::write(
        &path,
        include_bytes!("../../aede-core/tests/fixtures/images/rgba.png"),
    )
    .unwrap();
    path
}

fn params() -> Parameters {
    Parameters {
        reference: "label:example-label".into(),
        kind: "logo".into(),
        size: None,
    }
}

#[test]
fn label_logo_reads_the_same_existing_cache_as_explicit_enrichment() {
    let fixture = fixture(Some(MBID));
    let path = write_logo(&fixture);
    let catalog = fixture.0.catalog.blocking_read();
    let catalog = catalog.as_ref().unwrap();
    assert_eq!(
        select(catalog, &fixture.0.data_dir, &params())
            .unwrap()
            .path,
        path
    );
    assert!(
        select(
            catalog,
            &fixture.0.data_dir,
            &Parameters {
                kind: "cover".into(),
                ..params()
            }
        )
        .is_err()
    );
    std::fs::remove_file(path).unwrap();
    assert!(select(catalog, &fixture.0.data_dir, &params()).is_err());
}

#[test]
fn label_logo_uses_identified_sources_but_never_a_name_match_or_uncertain_claim() {
    let fixture = fixture(None);
    let path = write_logo(&fixture);
    let guard = fixture.0.catalog.blocking_read();
    let catalog = guard.as_ref().unwrap();
    let entity = EntityRef::of(catalog, EntityKind::Label, 0).unwrap();
    for (confidence, available) in [
        (Confidence::Matched(70), false),
        (Confidence::Identified, true),
    ] {
        let mut sources = Sources::default();
        sources.set(SourceRecord {
            key: entity.key.clone(),
            source: aede_core::sources::MUSICBRAINZ.into(),
            source_id: Some(MBID.into()),
            fetched_at: 1,
            confidence,
            facts: Facts::Label(LabelFacts::default()),
        });
        let selected = label_logo(catalog, &sources, 0, &fixture.0.data_dir);
        assert_eq!(selected.is_some(), available);
        if let Some(selected) = selected {
            assert_eq!(selected.path, path);
        }
        if !available {
            let item = sources
                .review_items(catalog)
                .into_iter()
                .find(|item| item.entity.kind == EntityKind::Label)
                .unwrap();
            sources
                .decide(
                    catalog,
                    &item.id,
                    aede_core::sources::ReviewDecision::Accepted,
                    2,
                )
                .unwrap();
            assert!(label_logo(catalog, &sources, 0, &fixture.0.data_dir).is_some());
            sources
                .decide(
                    catalog,
                    &item.id,
                    aede_core::sources::ReviewDecision::Rejected,
                    2,
                )
                .unwrap();
            assert!(label_logo(catalog, &sources, 0, &fixture.0.data_dir).is_none());
        }
    }
    assert!(label_logo(catalog, &Sources::default(), 0, &fixture.0.data_dir).is_none());
}

#[test]
fn label_logo_refuses_path_shaped_identifiers() {
    let fixture = fixture(Some("../outside"));
    write_logo(&fixture);
    let guard = fixture.0.catalog.blocking_read();
    assert!(select(guard.as_ref().unwrap(), &fixture.0.data_dir, &params()).is_err());
}

#[cfg(unix)]
#[test]
fn label_logo_refuses_symlinked_images_and_asset_directories() {
    let fixture = fixture(Some(MBID));
    let path = write_logo(&fixture);
    let outside = fixture.0.data_dir.join("outside.png");
    std::fs::rename(&path, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &path).unwrap();
    let guard = fixture.0.catalog.blocking_read();
    let catalog = guard.as_ref().unwrap();
    assert!(select(catalog, &fixture.0.data_dir, &params()).is_err());
    std::fs::remove_file(&path).unwrap();
    let folder = path.parent().unwrap();
    std::fs::remove_dir(folder).unwrap();
    let other = fixture.0.data_dir.join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::rename(outside, other.join("logo.png")).unwrap();
    std::os::unix::fs::symlink(other, folder).unwrap();
    assert!(select(catalog, &fixture.0.data_dir, &params()).is_err());
}

#[test]
fn label_logo_http_presence_and_local_image_agree_without_enrichment() {
    use tower::ServiceExt;
    let fixture = fixture(Some(MBID));
    let path = write_logo(&fixture);
    crate::test_support::test_runtime().block_on(async {
        let accounts = fixture.accounts();
        let principal = auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap())
            .unwrap()
            .0;
        let bearer = format!("Bearer {}", auth::session_token(&principal));
        let app = crate::router(fixture.0.clone(), "127.0.0.1:17919".parse().unwrap());
        for (route, listed) in [
            ("/api/v1/labels", true),
            ("/api/v1/label?ref=label:example-label", false),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(route)
                        .header(header::HOST, "127.0.0.1:17919")
                        .header("authorization", &bearer)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                if listed {
                    &value["items"][0]["logo"]
                } else {
                    &value["logo"]
                },
                &serde_json::json!(true)
            );
        }
        let image = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/artwork?ref=label:example-label&kind=logo")
                    .header(header::HOST, "127.0.0.1:17919")
                    .header("authorization", &bearer)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(image.status(), StatusCode::OK);
        assert_eq!(image.headers()[header::CONTENT_TYPE], "image/png");
        let bytes = axum::body::to_bytes(image.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        std::fs::remove_file(path).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/label?ref=label:example-label")
                    .header(header::HOST, "127.0.0.1:17919")
                    .header("authorization", bearer)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["logo"],
            false
        );
    });
}
