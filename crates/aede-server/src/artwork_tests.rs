use super::*;
use crate::test_support::sample_state;

#[test]
fn artist_artwork_cache_follows_trusted_musicbrainz_identity_after_reload_and_retagging() {
    use crate::accounts_test_support::Fixture;
    use aede_core::sources::{ArtistFacts, Confidence, Facts, SourceRecord, Sources};
    use tower::ServiceExt;

    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let original = "11111111-1111-4111-8111-111111111111";
        let canonical = "22222222-2222-4222-8222-222222222222";
        let successor = "33333333-3333-4333-8333-333333333333";
        let entity = {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            catalog.roots = vec![fixture.0.data_dir.join("music").to_str().unwrap().into()];
            catalog.releases[0].folder = fixture
                .0
                .data_dir
                .join("music/Album")
                .to_str()
                .unwrap()
                .into();
            EntityRef::of(catalog, EntityKind::Artist, 0).unwrap()
        };
        // This is the same fallback destination selected by CLI enrichment
        // when an artist's albums share a watched root rather than an artist folder.
        let destination = {
            let guard = fixture.0.catalog.read().await;
            let catalog = guard.as_ref().unwrap();
            coverart::artist_destination(
                catalog,
                &[&catalog.releases[0]],
                &fixture.0.data_dir,
                canonical,
            )
            .unwrap()
        };
        std::fs::create_dir_all(&destination).unwrap();
        let png = include_bytes!("../../aede-core/tests/fixtures/images/rgba.png");
        for kind in [Kind::Artist, Kind::Logo, Kind::Banner, Kind::Background] {
            std::fs::write(
                destination.join(coverart::image_name(kind, "png", 0, 1)),
                png,
            )
            .unwrap();
        }
        let path = aede_core::sources::sources_path(&fixture.0.data_dir);
        let mut sources = Sources::default();
        let accounts = fixture.accounts();
        let principal = auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap())
            .unwrap()
            .0;
        let bearer = format!("Bearer {}", auth::session_token(&principal));
        let address: SocketAddr = "127.0.0.1:17907".parse().unwrap();
        let app = crate::router(fixture.0.clone(), address);
        for (source_id, alias, tag) in [
            (canonical, Some(original), None),
            (canonical, Some(original), Some(original)),
            (canonical, Some(original), Some(canonical)),
            (successor, Some(canonical), Some(canonical)),
            (successor, Some(canonical), Some(successor)),
        ] {
            sources.set(SourceRecord {
                key: entity.key.clone(),
                source: aede_core::sources::MUSICBRAINZ.into(),
                source_id: Some(source_id.into()),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Artist(ArtistFacts {
                    lookup_mbid: alias.map(str::to_string),
                    ..Default::default()
                }),
            });
            aede_core::sources::save(&sources, &path).unwrap();
            fixture.0.catalog.write().await.as_mut().unwrap().artists[0].mbid =
                tag.map(str::to_string);
            for kind in ["portrait", "logo", "banner", "background"] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(format!(
                                "/api/v1/artwork?ref={}&kind={kind}",
                                crate::test_support::encoded(&entity.to_token())
                            ))
                            .header(header::HOST, address.to_string())
                            .header(header::AUTHORIZATION, bearer.clone())
                            .body(axum::body::Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "source={source_id}, tag={tag:?}, kind={kind}"
                );
                assert_eq!(
                    to_bytes(response.into_body(), 100_000)
                        .await
                        .unwrap()
                        .as_ref(),
                    png
                );
            }
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/presentation/artists")
                        .header(header::HOST, address.to_string())
                        .header(header::AUTHORIZATION, bearer.clone())
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
            let view: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            for kind in ["portrait", "logo", "banner", "background"] {
                assert_eq!(
                    view["items"][0]["media"][kind], true,
                    "source={source_id}, tag={tag:?}, kind={kind}"
                );
            }
        }
        auth::end_session(&fixture.0, &principal).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/v1/artwork?ref={}&kind=logo",
                        crate::test_support::encoded(&entity.to_token())
                    ))
                    .header(header::HOST, address.to_string())
                    .header(header::AUTHORIZATION, bearer)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "a populated shared cache cannot bypass logout"
        );
    });
}

#[test]
fn provider_artwork_cache_candidates_require_trust_and_confined_identifiers() {
    use aede_core::sources::{
        ArtistFacts, Confidence, Facts, ReviewDecision, SourceRecord, SourceReview, Sources,
    };

    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.roots = vec!["/music".into()];
    catalog.releases[0].folder = "/music/Album".into();
    let canonical = "22222222-2222-4222-8222-222222222222";
    let alias = "11111111-1111-4111-8111-111111111111";
    let unrelated = "33333333-3333-4333-8333-333333333333";
    let expected =
        coverart::asset_destination(&state.data_dir, EntityKind::Artist, canonical).unwrap();
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap();
    for (source_id, local_id, confidence, decision, visible) in [
        (canonical, None, Confidence::Identified, None, true),
        (canonical, Some(alias), Confidence::Identified, None, true),
        (
            canonical,
            Some(unrelated),
            Confidence::Identified,
            None,
            false,
        ),
        (canonical, None, Confidence::matched(95), None, false),
        (
            canonical,
            None,
            Confidence::matched(95),
            Some(ReviewDecision::Accepted),
            true,
        ),
        (
            canonical,
            None,
            Confidence::Identified,
            Some(ReviewDecision::Rejected),
            false,
        ),
        ("../../private", None, Confidence::Identified, None, false),
    ] {
        catalog.artists[0].mbid = local_id.map(str::to_string);
        let mut sources = Sources::default();
        sources.set(SourceRecord {
            key: entity.key.clone(),
            source: aede_core::sources::MUSICBRAINZ.into(),
            source_id: Some(source_id.into()),
            confidence,
            fetched_at: 1,
            facts: Facts::Artist(ArtistFacts {
                lookup_mbid: Some(alias.into()),
                ..Default::default()
            }),
        });
        if let Some(decision) = decision {
            sources.set_review(SourceReview {
                entity: entity.clone(),
                source: aede_core::sources::MUSICBRAINZ.into(),
                source_id: Some(source_id.into()),
                decision,
                reviewed_at: 2,
            });
        }
        let sources =
            aede_core::sources::from_json(&aede_core::sources::to_json(&sources)).unwrap();
        let folders = artist_folders(&catalog, &sources, 0, &state.data_dir);
        assert_eq!(
            folders.contains(&expected),
            visible,
            "source={source_id}, local={local_id:?}, confidence={confidence:?}, decision={decision:?}"
        );
        assert!(
            folders
                .iter()
                .all(|folder| folder.starts_with(state.data_dir.join("assets/artists")))
        );
    }
}

#[test]
fn artwork_selection_rejects_paths_and_wrong_entity_kinds() {
    let state = sample_state();
    let guard = state.catalog.blocking_read();
    let catalog = guard.as_ref().unwrap();
    for (reference, kind) in [
        ("/private/cover.jpg".into(), "cover"),
        (reference(catalog, EntityKind::Artist, 0).unwrap(), "cover"),
        (
            reference(catalog, EntityKind::Release, 0).unwrap(),
            "../background",
        ),
    ] {
        assert!(
            select(
                catalog,
                &state.data_dir,
                &Parameters {
                    reference,
                    kind: kind.into(),
                    size: None
                }
            )
            .is_err()
        );
    }
}
#[test]
fn artist_assets_never_escape_through_a_tagged_identifier_or_watched_root() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.artists[0].mbid = Some("../../private".into());
    catalog.releases[0].folder = "/music/album".into();
    catalog.roots = vec!["/music".into()];
    assert!(artist_folder(&catalog, 0, &state.data_dir).is_none());
    catalog.artists[0].mbid = Some("12345678-1234-1234-1234-123456789abc".into());
    assert_eq!(
        artist_folder(&catalog, 0, &state.data_dir),
        Some(
            state
                .data_dir
                .join("assets/artists/12345678-1234-1234-1234-123456789abc")
        )
    );
}

#[test]
fn artist_logo_uses_the_same_confined_asset_and_presence_contract_as_portraits() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    let root = std::env::temp_dir().join(format!("aede-logo-presentation-{}", std::process::id()));
    let artist_folder = root.join("Artist");
    std::fs::create_dir_all(artist_folder.join("Album")).unwrap();
    catalog.roots = vec![root.to_str().unwrap().into()];
    catalog.releases[0].folder = artist_folder.join("Album").to_str().unwrap().into();
    let logo = artist_folder.join(coverart::image_name(Kind::Logo, "png", 0, 1));
    std::fs::write(&logo, b"\x89PNG\r\n\x1a\n").unwrap();
    let params = Parameters {
        reference: reference(&catalog, EntityKind::Artist, 0).unwrap(),
        kind: "logo".into(),
        size: Some(512),
    };
    let source = select(&catalog, &state.data_dir, &params).expect("catalogue artist logo");
    assert_eq!(source.path, logo);
    std::fs::remove_file(logo).unwrap();
    assert!(select(&catalog, &state.data_dir, &params).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_artwork_rechecks_anonymous_activation_and_revoked_sessions_before_publication() {
    use crate::accounts_test_support::{ADMIN_TOKEN, Fixture};
    use tower::ServiceExt;

    async fn worker_started(state: &ApiState) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while state.inspection_slots.available_permits() != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let folder = fixture.0.data_dir.join("Album");
        std::fs::create_dir(&folder).unwrap();
        let png = include_bytes!("../../aede-core/tests/fixtures/images/rgba.png");
        let cover = folder.join("cover.png");
        std::fs::write(&cover, png).unwrap();
        let wanted = {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            catalog.releases[0].folder = folder.to_string_lossy().into_owned();
            catalog.releases[0].cover_path = Some(cover.to_string_lossy().into_owned());
            reference(catalog, EntityKind::Release, 0).unwrap()
        };
        let protected = [
            "catalog.json",
            "sources.json",
            "user.json",
            "conclusions.json",
        ]
        .map(|name| fixture.0.data_dir.join(name));
        let before = protected.each_ref().map(|path| std::fs::read(path).ok());
        let accounts = fixture.accounts();
        let accounts_path = aede_core::accounts::accounts_path(&fixture.0.data_dir);
        let accounts_bytes = std::fs::read(&accounts_path).unwrap();
        std::fs::remove_file(&accounts_path).unwrap();

        // Use the complete Host/authentication/router path without opening a socket.
        let address: SocketAddr = "127.0.0.1:17897".parse().unwrap();
        let path = format!(
            "/api/v1/artwork?ref={}&kind=cover",
            crate::test_support::encoded(&wanted)
        );
        let app = crate::router(fixture.0.clone(), address);
        let request = |token: Option<&str>| {
            let mut builder = Request::builder()
                .uri(&path)
                .header(header::HOST, address.to_string());
            if let Some(token) = token {
                builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            builder.body(axum::body::Body::empty()).unwrap()
        };
        let anonymous = app.clone().oneshot(request(None)).await.unwrap();
        assert_eq!(anonymous.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(anonymous.into_body(), 100_000)
                .await
                .unwrap()
                .as_ref(),
            png
        );

        // Admit anonymously, then activate account mode while the real worker waits.
        let gate = fixture.0.catalog.write().await;
        let waiting = tokio::spawn(app.clone().oneshot(request(None)));
        worker_started(&fixture.0).await;
        aede_core::accounts::save(&accounts, &accounts_path).unwrap();
        assert!(auth::load_accounts(&fixture.0).unwrap().is_some());
        drop(gate);
        let refused = waiting.await.unwrap().unwrap();
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(refused.into_body(), 100_000).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["error"]["code"], "unauthorized");

        let principal =
            auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap())
                .unwrap()
                .0;
        let signed = app
            .clone()
            .oneshot(request(Some(auth::session_token(&principal))))
            .await
            .unwrap();
        assert_eq!(signed.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(signed.into_body(), 100_000)
                .await
                .unwrap()
                .as_ref(),
            png
        );
        let gate = fixture.0.catalog.write().await;
        let waiting = tokio::spawn(
            app.clone()
                .oneshot(request(Some(auth::session_token(&principal)))),
        );
        worker_started(&fixture.0).await;
        auth::end_session(&fixture.0, &principal).unwrap();
        drop(gate);
        assert_eq!(
            waiting.await.unwrap().unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let administrative = app.oneshot(request(Some(ADMIN_TOKEN))).await.unwrap();
        assert_eq!(administrative.status(), StatusCode::OK);
        drop(administrative);
        assert_eq!(std::fs::read(cover).unwrap(), png);
        assert_eq!(std::fs::read(accounts_path).unwrap(), accounts_bytes);
        assert_eq!(
            protected.each_ref().map(|path| std::fs::read(path).ok()),
            before
        );
    });
}
