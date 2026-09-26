use super::*;
use crate::test_support::{encoded, json_response, request, sample_state};
use aede_core::model::{
    Artist, AudioFile, Genre, GenreLink, Label, Recording, Release, ReleaseGroup, Track, Work,
};
use aede_core::sources::{ArtistFacts, ReviewDecision, SourceRecord, SourceReview};

async fn fixture() -> ApiState {
    let mut state = sample_state();
    state.data_dir = std::env::temp_dir().join(format!(
        "aede_navigation_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut guard = state.catalog.write().await;
        let catalog = guard.as_mut().unwrap();
        catalog.artists[0].key = text::normalize(&catalog.artists[0].name);
        catalog.artists[0].mbid = Some("artist-1".into());
        catalog.artists[0].aliases = vec!["accadacca".into()];
        catalog.artists.push(Artist {
            id: 1,
            name: "AC/DC Tribute".into(),
            sort_name: "AC/DC Tribute".into(),
            key: text::normalize("AC/DC Tribute"),
            ..Default::default()
        });
        catalog.releases[0].key = "back in black".into();
        catalog.releases[0].year = Some(1980);
        catalog.releases[0].folder = "/music/album".into();
        catalog.labels[0].key = "atlantic".into();
        catalog.genres[0].key = "rock".into();
        catalog.works[0].key = "hells bells".into();
        catalog.recordings[0].key = "hells bells".into();
        catalog.release_groups[0].key = "back in black".into();
        catalog.labels.push(Label {
            id: 1,
            name: "Roadrunner".into(),
            key: "roadrunner".into(),
            ..Default::default()
        });
        catalog.genres.push(Genre {
            id: 1,
            name: "Hard Rock".into(),
            key: "hard rock".into(),
        });
        catalog.works.push(Work {
            id: 1,
            title: "Hells Bells Live".into(),
            key: "hells bells live".into(),
            mbid: "work-2".into(),
            recording_ids: vec![2],
        });
        catalog.release_groups.push(ReleaseGroup {
            id: 1,
            title: "Black Album".into(),
            key: "black album".into(),
            mbid: "group-2".into(),
            release_ids: vec![1],
        });
        for (id, title, track_title, year, artist, label) in [
            (1, "Black Album", "Other Song", 1991, 1, 1),
            (2, "Back in Black Live", "Hells Bells Live", 1990, 0, 0),
        ] {
            catalog.files.push(AudioFile {
                id,
                path: format!("/music/album-{id}/01.flac"),
                size: 100,
                ..Default::default()
            });
            catalog.tracks.push(Track {
                id,
                file_id: id,
                release_id: Some(id),
                recording_id: id,
                title: track_title.into(),
                ..Default::default()
            });
            catalog.recordings.push(Recording {
                id,
                title: track_title.into(),
                key: text::normalize(track_title),
                mbid: Some(format!("recording-{id}")),
                track_ids: vec![id],
                work_ids: if id == 2 { vec![1] } else { vec![] },
                ..Default::default()
            });
            catalog.releases.push(Release {
                id,
                title: title.into(),
                key: text::normalize(title),
                album_artist_id: Some(artist),
                year: Some(year),
                label_ids: vec![label],
                track_ids: vec![id],
                release_group_id: Some(if id == 1 { 1 } else { 0 }),
                folder: format!("/music/album-{id}"),
                ..Default::default()
            });
            catalog.genre_links.push(GenreLink {
                genre_id: 1,
                entity_kind: EntityKind::Track,
                entity_id: id,
            });
        }
    }
    state
}

async fn start(state: ApiState) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, crate::router(state, address)).await;
    });
    (address, server)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn response_json(response: &str) -> serde_json::Value {
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn singular_navigation_prefers_exact_names_and_returns_usable_ambiguity_candidates() {
    runtime().block_on(async {
        let (address, server) = start(fixture().await).await;
        for (route, name, kind) in [
            ("album", "Back in Black", "release"),
            ("track", "Hells Bells", "track"),
            ("artist", "AC/DC", "artist"),
            ("genre", "Rock", "genre"),
            ("label", "Atlantic", "label"),
            ("work", "Hells Bells", "work"),
            ("release-group", "Back in Black", "release_group"),
            ("recording", "Hells Bells", "recording"),
        ] {
            let detail = json_response(address, &format!("/api/v1/{route}?name={}", encoded(name)));
            assert_eq!(detail["kind"], kind, "{route}");
            let by_reference = json_response(
                address,
                &format!(
                    "/api/v1/{route}?ref={}",
                    encoded(detail["reference"].as_str().unwrap())
                ),
            );
            assert_eq!(detail, by_reference, "{route}");
            let legacy = json_response(
                address,
                &format!(
                    "/api/v1/entities?ref={}",
                    encoded(detail["reference"].as_str().unwrap())
                ),
            );
            for (key, value) in legacy.as_object().unwrap() {
                assert_eq!(&detail[key], value, "{route}.{key}");
            }
        }
        let alias = json_response(address, "/api/v1/artist?name=accadacca");
        assert_eq!(alias["name"], "AC/DC");
        let ambiguous = request(address, "/api/v1/album?name=Black");
        assert!(ambiguous.starts_with("HTTP/1.1 409"), "{ambiguous}");
        let error = response_json(&ambiguous);
        assert_eq!(error["error"]["code"], "ambiguous_entity");
        let candidates = error["error"]["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 3);
        for candidate in candidates {
            let selected = json_response(
                address,
                &format!(
                    "/api/v1/album?ref={}",
                    encoded(candidate["reference"].as_str().unwrap())
                ),
            );
            assert_eq!(selected["title"], candidate["name"]);
        }
        for route in [
            "album",
            "track",
            "artist",
            "genre",
            "label",
            "work",
            "release-group",
            "recording",
            "from",
        ] {
            let missing = request(address, &format!("/api/v1/{route}?name=not-in-library"));
            assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        }
        server.abort();
    });
}

#[test]
fn catalog_lists_compose_cli_filters_before_sorting_and_pagination() {
    runtime().block_on(async {
        let (address, server) = start(fixture().await).await;
        let albums = json_response(address, "/api/v1/albums?artist=AC%2FDC&genre=rock&label=Atlantic&sort=year&order=desc&limit=1&offset=1");
        assert_eq!(albums["total"], 2);
        assert_eq!(albums["offset"], 1);
        assert_eq!(albums["limit"], 1);
        assert_eq!(albums["items"][0]["title"], "Back in Black");
        let exact_artist = json_response(address, "/api/v1/albums?artist=artist%3Aac%20dc&sort=year");
        assert_eq!(exact_artist["total"], 2);
        let year = json_response(address, "/api/v1/albums?year=1991&name=Black");
        assert_eq!(year["total"], 1);
        assert_eq!(year["items"][0]["title"], "Black Album");
        assert_eq!(json_response(address, "/api/v1/albums?offset=99")["items"], serde_json::json!([]));
        assert_eq!(json_response(address, "/api/v1/albums")["items"], json_response(address, "/api/v1/releases")["items"]);
        for route in ["genres", "labels", "works", "release-groups"] {
            let page = json_response(address, &format!("/api/v1/{route}?sort=name&order=desc&limit=1"));
            assert_eq!(page["total"], 2, "{route}");
            assert_eq!(page["items"].as_array().unwrap().len(), 1);
            assert_eq!(page["scanned_at"], 1_700_000_000);
        }
        let genre = json_response(address, "/api/v1/genres?q=hard");
        assert_eq!(genre["items"][0]["track_count"], 2);
        assert_eq!(genre["total"], 1);
        let work = json_response(address, "/api/v1/works?mbid=work-2");
        assert_eq!(work["items"][0]["title"], "Hells Bells Live");
        assert_eq!(work["total"], 1);
        server.abort();
    });
}

#[test]
fn navigation_rejects_unsupported_duplicate_and_incompatible_options() {
    runtime().block_on(async {
        let (address, server) = start(fixture().await).await;
        for path in [
            "/api/v1/album",
            "/api/v1/album?name=",
            "/api/v1/album?name=Black&ref=release%3Ax",
            "/api/v1/album?name=Black&name=Black",
            "/api/v1/album?ref=artist%3Aac%2Fdc",
            "/api/v1/artist?name=AC%2FDC&fetch=true",
            "/api/v1/albums?name=Black&q=Black",
            "/api/v1/albums?sort=size",
            "/api/v1/albums?year=-1",
            "/api/v1/albums?year=4294967296",
            "/api/v1/albums?limit=0",
            "/api/v1/albums?offset=%2B1",
            "/api/v1/albums?genre=",
            "/api/v1/genres?artist=AC%2FDC",
            "/api/v1/genres?mbid=x",
            "/api/v1/labels?sort=year",
            "/api/v1/works?limit=201",
            "/api/v1/release-groups?unknown=true",
        ] {
            let response = request(address, path);
            assert!(response.starts_with("HTTP/1.1 400"), "{path}: {response}");
        }
        let missing = request(address, "/api/v1/albums?genre=not-a-genre");
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        server.abort();
    });
}

#[test]
fn artist_origin_is_explicit_attributed_local_and_never_guessed() {
    runtime().block_on(async {
        let state = fixture().await;
        let dir = state.data_dir.clone();
        let (address, server) = start(state).await;
        let absent = json_response(address, "/api/v1/from?name=AC%2FDC");
        assert_eq!(absent["origin"]["status"], "unknown");
        assert!(absent["origin"]["country_code"].is_null());
        assert!(
            absent["origin"]["message"]
                .as_str()
                .unwrap()
                .contains("No MusicBrainz")
        );
        assert!(
            !dir.exists(),
            "a GET with missing information must not fetch or persist anything"
        );
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: text::normalize("AC/DC"),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("artist-1".into()),
            fetched_at: 123,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts::default()),
        });
        sources::save(&held, &sources::sources_path(&dir)).unwrap();
        let unknown = json_response(address, "/api/v1/artist?name=AC%2FDC");
        assert_eq!(unknown["origin"]["status"], "unknown");
        assert_eq!(unknown["origin"]["attribution"]["source"], "musicbrainz");
        assert!(
            unknown["origin"]["message"]
                .as_str()
                .unwrap()
                .contains("supplied no country")
        );
        held.records[0].facts = Facts::Artist(ArtistFacts {
            area: Some("Australia".into()),
            country_code: Some("AU".into()),
            ..Default::default()
        });
        sources::save(&held, &sources::sources_path(&dir)).unwrap();
        std::fs::write(
            dir.join("user.json"),
            "private annotations must not be read or exposed",
        )
        .unwrap();
        let before = std::fs::read(sources::sources_path(&dir)).unwrap();
        let known = json_response(address, "/api/v1/artist?name=AC%2FDC");
        assert_eq!(known["origin"]["status"], "known");
        assert_eq!(known["origin"]["country_code"], "AU");
        assert_eq!(known["origin"]["area"], "Australia");
        assert_eq!(known["origin"]["attribution"]["fetched_at"], 123);
        assert_eq!(known["origin"]["attribution"]["source_id"], "artist-1");
        assert_eq!(std::fs::read(sources::sources_path(&dir)).unwrap(), before);
        assert!(!known.to_string().contains("private annotations"));
        held.records[0].confidence = Confidence::Matched(95);
        sources::save(&held, &sources::sources_path(&dir)).unwrap();
        let untrusted = json_response(address, "/api/v1/from?name=AC%2FDC");
        assert_eq!(untrusted["origin"]["status"], "unknown");
        assert!(untrusted["origin"]["country_code"].is_null());
        assert_eq!(untrusted["origin"]["attribution"]["trusted"], false);
        held.set_review(SourceReview {
            entity: held.records[0].entity(),
            source: sources::MUSICBRAINZ.into(),
            source_id: held.records[0].source_id.clone(),
            decision: ReviewDecision::Accepted,
            reviewed_at: 124,
        });
        sources::save(&held, &sources::sources_path(&dir)).unwrap();
        let accepted = json_response(address, "/api/v1/from?name=AC%2FDC");
        assert_eq!(accepted["origin"]["status"], "known");
        assert_eq!(accepted["origin"]["attribution"]["confidence"], "matched");
        assert_eq!(accepted["origin"]["attribution"]["match_score"], 95);
        assert_eq!(accepted["origin"]["attribution"]["trusted"], true);
        held.reviews.clear();
        held.records[0].source_id = Some("a-different-artist".into());
        held.records[0].confidence = Confidence::Identified;
        sources::save(&held, &sources::sources_path(&dir)).unwrap();
        let conflicting = json_response(address, "/api/v1/from?name=AC%2FDC");
        assert_eq!(conflicting["origin"]["status"], "unknown");
        assert!(conflicting["origin"]["area"].is_null());
        assert_eq!(conflicting["origin"]["attribution"]["trusted"], false);
        std::fs::write(sources::sources_path(&dir), "corrupt").unwrap();
        let unreadable = request(address, "/api/v1/from?name=AC%2FDC");
        assert!(unreadable.starts_with("HTTP/1.1 500"), "{unreadable}");
        assert_eq!(
            response_json(&unreadable)["error"]["code"],
            "sources_unavailable"
        );
        server.abort();
        std::fs::remove_file(sources::sources_path(&dir)).unwrap();
        std::fs::remove_file(dir.join("user.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    });
}

#[test]
fn navigation_shares_the_inspection_budget_and_keeps_status_available() {
    runtime().block_on(async {
        let state = fixture().await;
        let slots = state.inspection_slots.clone();
        let permits = slots.try_acquire_many(2).unwrap();
        let (address, server) = start(state).await;
        for path in [
            "/api/v1/albums",
            "/api/v1/artist?name=AC%2FDC",
            "/api/v1/from?name=AC%2FDC",
            "/api/v1/genres",
        ] {
            let response = request(address, path);
            assert!(response.starts_with("HTTP/1.1 429"), "{response}");
            assert_eq!(response_json(&response)["error"]["code"], "inspection_busy");
        }
        assert_eq!(json_response(address, "/api/v1/status")["status"], "ok");
        drop(permits);
        assert_eq!(json_response(address, "/api/v1/albums")["total"], 3);
        server.abort();
    });
}
