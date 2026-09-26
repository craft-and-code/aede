use super::*;
use crate::test_support::{encoded, json_response, request, sample_state};
use aede_core::model::{IntegrityRecord, ScannedFile};
use aede_core::tags::RawTags;

struct Fixture {
    dir: PathBuf,
    state: ApiState,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "aede_inspection_{}_{}_{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("test")
                .replace(':', "_"),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut state = sample_state();
        state.data_dir = dir.clone();
        let catalog = test_catalog();
        store::save(&catalog, &store::catalog_path(&dir)).unwrap();
        state.catalog = Arc::new(RwLock::new(Some(catalog)));
        Self { dir, state }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

fn test_catalog() -> Catalog {
    let scanned = |path: &str, fields: &[(&str, &str)]| {
        let mut tags = RawTags::default();
        for (name, value) in fields {
            tags.insert(name, *value);
        }
        tags.properties.codec = "flac".into();
        tags.properties.lossless = true;
        tags.properties.duration_ms = Some(1000);
        ScannedFile {
            path: path.into(),
            size: 42,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    };
    let mut catalog = aede_core::model::build(
        vec![
            scanned(
                "/music/root/01.flac",
                &[
                    ("title", "Écho"),
                    ("artist", "Écho"),
                    ("album", "The Echo Sessions"),
                    ("albumartist", "Écho"),
                    ("genre", "Jazz"),
                    ("date", "2000"),
                    ("composer", "Writer"),
                    ("comment", "Echo vinyl rip"),
                ],
            ),
            scanned(
                "/music/root/02.flac",
                &[
                    ("title", "Echoes"),
                    ("artist", "Other"),
                    ("album", "Ocean"),
                    ("genre", "Rock"),
                    ("date", "2001"),
                ],
            ),
            scanned("/music/rooted/03 missing.flac", &[]),
        ],
        vec!["/music/root".into()],
        42,
        &[],
    );
    catalog.excluded.push("/music/root/private".into());
    catalog
}

fn with_server(test: impl FnOnce(SocketAddr, &Fixture)) {
    let fixture = Fixture::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let app = routes().with_state(fixture.state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        test(address, &fixture);
        server.abort();
        let _ = server.await;
    });
}

fn error_response(address: SocketAddr, path: &str, status: u16, code: &str) {
    let response = request(address, path);
    assert!(
        response.starts_with(&format!("HTTP/1.1 {status}")),
        "{response}"
    );
    let body: serde_json::Value =
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["error"]["code"], code, "{body}");
}

#[test]
fn inspection_statistics_roots_and_roles_are_structured_and_paginated() {
    with_server(|address, _| {
        let stats = json_response(address, "/api/v1/stats?limit=1");
        assert_eq!(stats["files"], 3);
        assert_eq!(stats["tracks"], 3);
        assert_eq!(stats["bytes"], 126);
        assert_eq!(stats["duration_ms"], 3000);
        assert_eq!(stats["by_codec"]["items"][0]["label"], "FLAC");
        assert_eq!(stats["by_codec"]["items"][0]["count"], 3);
        assert_eq!(stats["by_codec"]["limit"], 1);
        assert!(stats["roles"]["total"].as_u64().unwrap() >= 3);
        assert_eq!(stats["roles"]["items"].as_array().unwrap().len(), 1);
        let roots = json_response(address, "/api/v1/roots?limit=1");
        assert_eq!(roots["total"], 3);
        assert_eq!(roots["items"][0]["path"], "/music/root");
        assert_eq!(roots["items"][0]["tracks"], 2);
        assert_eq!(roots["totals"]["tracks"], 3);
        assert_eq!(roots["totals"]["bytes"], 126);
        let excluded = json_response(address, "/api/v1/roots?limit=1&offset=1");
        assert_eq!(excluded["items"][0]["status"], "excluded");
        let unwatched = json_response(address, "/api/v1/roots?offset=2");
        assert_eq!(unwatched["items"][0]["status"], "unwatched");
        assert!(unwatched["items"][0]["path"].is_null());
        assert_eq!(
            unwatched["items"][0]["tracks"], 1,
            "rooted is not below root"
        );
        let roles = json_response(address, "/api/v1/roles?limit=1&offset=1");
        assert_eq!(roles["items"].as_array().unwrap().len(), 1);
        assert_eq!(roles["items"][0]["role"], "composer");
        assert_eq!(roles["items"][0]["artists"], 1);
        assert_eq!(roles["items"][0]["credits"], 1);
    });
}

#[test]
fn inspection_search_preserves_core_ranking_and_separate_comment_provenance() {
    with_server(|address, fixture| {
        // Public inspection must not open this file, even if it cannot parse it.
        std::fs::write(
            fixture.dir.join("user.json"),
            "SECRET private notes, not JSON",
        )
        .unwrap();
        let names = json_response(address, "/api/v1/search?q=echo");
        let catalog = test_catalog();
        let expected = catalog.search("echo", usize::MAX);
        assert_eq!(names["total"], expected.len());
        for (found, expected) in names["items"].as_array().unwrap().iter().zip(expected) {
            assert_eq!(found["name"], expected.name);
            assert_eq!(found["kind"], expected.kind.as_str());
            assert_eq!(found["found_in"], "name");
            assert_eq!(
                found["reference"],
                reference(&catalog, expected.kind, expected.id).unwrap()
            );
        }
        let comments = json_response(address, "/api/v1/search?q=echo&comments=true");
        assert_eq!(
            comments["total"].as_u64().unwrap(),
            names["total"].as_u64().unwrap() + 1
        );
        let comment = comments["items"].as_array().unwrap().last().unwrap();
        assert_eq!(comment["found_in"], "comment");
        assert_eq!(comment["context"], "Echo vinyl rip");
        let page = json_response(
            address,
            "/api/v1/search?q=echo&comments=true&limit=1&offset=1",
        );
        assert_eq!(page["items"][0], comments["items"][1]);
        assert_eq!(page["total"], comments["total"]);
        assert_eq!(
            json_response(address, "/api/v1/search?q=SECRET")["total"],
            0
        );
    });
}

#[test]
fn inspection_query_uses_public_core_semantics_and_refuses_personal_fields() {
    with_server(|address, fixture| {
        std::fs::write(
            fixture.dir.join("user.json"),
            "SECRET private notes, not JSON",
        )
        .unwrap();
        let expression = "genre:jazz year:1990..2000 composer:writer";
        let selected = json_response(
            address,
            &format!("/api/v1/query?q={}&sort=title-", encoded(expression)),
        );
        assert_eq!(selected["total"], 1);
        assert_eq!(selected["items"][0]["title"], "Écho");
        let ranked = json_response(address, "/api/v1/query?q=title:echo&sort=title-&limit=1");
        assert_eq!(ranked["total"], 2);
        assert_eq!(ranked["items"][0]["title"], "Echoes");
        for private in [
            "note:SECRET",
            "-note",
            "album.rating:4",
            "artist.loved",
            "tag:rare",
            "played:0",
            "title:echo OR (NOT artist.note:x)",
            "lyrics:train",
        ] {
            error_response(
                address,
                &format!("/api/v1/query?q={}", encoded(private)),
                400,
                "invalid_query",
            );
        }
        for sort in ["rating", "played-", "wrong"] {
            error_response(
                address,
                &format!("/api/v1/query?q=title:echo&sort={sort}"),
                400,
                "invalid_query",
            );
        }
        error_response(
            address,
            "/api/v1/query?q=genre:unknown",
            400,
            "invalid_query",
        );
    });
}

#[test]
fn inspection_doctor_reloads_conclusions_and_reports_source_errors_without_private_data() {
    with_server(|address, fixture| {
        std::fs::write(fixture.dir.join("user.json"), "private invalid document").unwrap();
        let first = json_response(address, "/api/v1/doctor?severity=error&limit=1");
        assert_eq!(first["unverified_files"], 3);
        assert!(first["total"].as_u64().unwrap() >= 2);
        assert_eq!(first["items"].as_array().unwrap().len(), 1);
        assert_eq!(first["items"][0]["severity"], "error");
        let mut current = test_catalog();
        current.files[0].integrity = Some(IntegrityRecord {
            verdict: aede_core::audit::integrity::Verdict::Damaged {
                detail: "bad checksum".into(),
            },
            method: "flac-frame-crc".into(),
            checked_at: 7,
        });
        let conclusions = aede_core::conclusions::Conclusions::from_catalog(&current);
        aede_core::conclusions::save(
            &conclusions,
            &aede_core::conclusions::conclusions_path(&fixture.dir),
        )
        .unwrap();
        let second = json_response(address, "/api/v1/doctor?severity=error");
        assert_eq!(second["unverified_files"], 2);
        assert!(
            second["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "damaged_audio")
        );
        assert_eq!(second["summary"]["warning"], 0);
        let guard = StoreLock::acquire(&fixture.dir).unwrap();
        error_response(address, "/api/v1/doctor", 409, "store_busy");
        drop(guard);
        std::fs::write(
            sources::sources_path(&fixture.dir),
            "broken source document",
        )
        .unwrap();
        for path in [
            "/api/v1/doctor",
            "/api/v1/stats",
            "/api/v1/query?q=title:echo",
        ] {
            error_response(address, path, 500, "sources_unavailable");
        }
        assert!(
            json_response(address, "/api/v1/search?q=echo")["total"]
                .as_u64()
                .unwrap()
                > 0
        );
    });
}

#[test]
fn inspection_routes_reject_unknown_duplicate_and_unsupported_parameters() {
    with_server(|address, _| {
        for path in [
            "/api/v1/search",
            "/api/v1/search?q=",
            "/api/v1/search?q=echo&q=other",
            "/api/v1/search?q=echo&notes=true",
            "/api/v1/search?q=echo&lyrics=true",
            "/api/v1/search?q=echo&comments=yes",
            "/api/v1/search?q=echo&sort=name",
            "/api/v1/query?q=title:echo&comments=true",
            "/api/v1/doctor?severity=severe",
            "/api/v1/stats?q=echo",
            "/api/v1/roots?remove=true",
            "/api/v1/roles?sort=name",
        ] {
            error_response(address, path, 400, "invalid_query");
        }
        for path in [
            "/api/v1/search?q=echo&limit=0",
            "/api/v1/stats?offset=-1",
            "/api/v1/roots?limit=201",
        ] {
            error_response(address, path, 400, "invalid_pagination");
        }
        error_response(
            address,
            &format!("/api/v1/search?q={}", "a".repeat(MAX_TEXT_BYTES + 1)),
            400,
            "invalid_query",
        );
        error_response(
            address,
            &format!(
                "/api/v1/query?q={}title:echo",
                "-".repeat(MAX_QUERY_PARTS + 1)
            ),
            400,
            "invalid_query",
        );
    });
}

#[test]
fn inspection_worker_capacity_is_rejected_without_starting_extra_work() {
    with_server(|address, fixture| {
        let permits = fixture.state.inspection_slots.try_acquire_many(2).unwrap();
        error_response(address, "/api/v1/search?q=echo", 429, "inspection_busy");
        drop(permits);
        assert!(
            json_response(address, "/api/v1/search?q=echo")["total"]
                .as_u64()
                .unwrap()
                > 0
        );
    });
}

#[test]
fn inspection_countries_keep_source_iso_codes_and_derived_initials_distinct() {
    with_server(|address, fixture| {
        let empty = json_response(address, "/api/v1/countries");
        assert_eq!(empty["total"], 0);
        assert_eq!(empty["coverage"]["artists_asked"], 0);
        assert_eq!(empty["coverage"]["artists_with_area"], 0);
        assert_eq!(
            empty["coverage"]["artists_without_area"],
            empty["coverage"]["artists_total"]
        );
        let catalog = test_catalog();
        let mut held = sources::Sources::default();
        for (name, area, code) in [
            ("Écho", Some("United Kingdom"), Some("GB")),
            ("Other", Some("County Antrim"), None),
            ("Writer", None, None),
        ] {
            let artist = catalog
                .artists
                .iter()
                .find(|artist| artist.name == name)
                .unwrap();
            held.set(sources::SourceRecord {
                key: EntityRef::of(&catalog, EntityKind::Artist, artist.id)
                    .unwrap()
                    .key,
                source: sources::MUSICBRAINZ.into(),
                source_id: None,
                fetched_at: 1,
                confidence: sources::Confidence::Identified,
                facts: sources::Facts::Artist(sources::ArtistFacts {
                    area: area.map(str::to_string),
                    country_code: code.map(str::to_string),
                    ..Default::default()
                }),
            });
        }
        // Diagnosis must use the same external evidence as the CLI, rather
        // than reporting only the missing tags from the local catalog.
        held.set(sources::SourceRecord {
            key: EntityRef::of(&catalog, EntityKind::Release, 0).unwrap().key,
            source: sources::MUSICBRAINZ.into(),
            source_id: None,
            fetched_at: 1,
            confidence: sources::Confidence::Identified,
            facts: sources::Facts::Release(sources::ReleaseFacts {
                first_released: Some("1990".into()),
                ..Default::default()
            }),
        });
        sources::save(&held, &sources::sources_path(&fixture.dir)).unwrap();
        let page = json_response(address, "/api/v1/countries?limit=1");
        assert_eq!(page["total"], 2);
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["items"][0]["name"], "County Antrim");
        assert!(page["items"][0]["iso_code"].is_null());
        assert_eq!(page["items"][0]["derived_initials"], "ca");
        assert_eq!(page["coverage"]["artists_asked"], 3);
        assert_eq!(page["coverage"]["artists_with_area"], 2);
        assert_eq!(page["coverage"]["artists_without_area"], 1);
        assert_eq!(page["coverage"]["places_without_iso_code"], 1);
        let uk = json_response(address, "/api/v1/countries?q=UK");
        assert_eq!(uk["matched_by"], "exact");
        assert_eq!(uk["items"][0]["iso_code"], "gb");
        assert_eq!(uk["items"][0]["derived_initials"], "uk");
        assert_eq!(uk["items"][0]["tracks"], 1);
        assert_eq!(uk["items"][0]["bytes"], 42);
        assert_eq!(
            json_response(address, "/api/v1/countries?q=kingdom")["matched_by"],
            "partial"
        );
        error_response(
            address,
            "/api/v1/countries?q=Atlantis",
            400,
            "invalid_query",
        );
        let diagnosis = json_response(address, "/api/v1/doctor?limit=200");
        assert!(
            diagnosis["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["type"] == "source_disagrees")
        );
        std::fs::write(sources::sources_path(&fixture.dir), "broken").unwrap();
        error_response(address, "/api/v1/countries", 500, "sources_unavailable");
    });
}

#[test]
fn inspection_years_order_and_coverage_distinguish_undated_and_orphaned_tracks() {
    with_server(|address, _| {
        let first = json_response(address, "/api/v1/years?limit=1");
        assert_eq!(first["total"], 2);
        assert_eq!(first["items"][0]["year"], 2000);
        assert_eq!(first["items"][0]["albums"], 1);
        assert_eq!(first["items"][0]["duration_ms"], 1000);
        assert_eq!(first["albums_without_year"], 0);
        assert_eq!(first["tracks_without_dated_album"], 1);
        let second = json_response(address, "/api/v1/years?limit=1&offset=1");
        assert_eq!(second["items"][0]["year"], 2001);
        for path in [
            "/api/v1/years?sort=year",
            "/api/v1/years?q=2000",
            "/api/v1/countries?sort=name",
        ] {
            error_response(address, path, 400, "invalid_query");
        }
    });
    let mut catalog = test_catalog();
    catalog.releases[0].year = None;
    let years = year_rows(
        &catalog,
        Window {
            offset: 0,
            limit: 50,
        },
    );
    assert_eq!(years.albums_without_year, 1);
    assert_eq!(years.tracks_without_dated_album, 2);
}

#[test]
fn inspection_diagnostic_file_previews_are_explicitly_bounded() {
    let mut catalog = test_catalog();
    let first = catalog.files[0].clone();
    // An identical fingerprint groups copies even when their names differ.
    let fingerprint = aede_core::fingerprint::Fingerprint {
        data: "same".into(),
        seconds: 1,
    };
    for number in 0..ISSUE_FILE_PREVIEW + 2 {
        let mut file = first.clone();
        file.id = catalog.files.len() as Id;
        file.path = format!("/copies/{number}.flac");
        file.fingerprint = Some(fingerprint.clone());
        catalog.files.push(file);
    }
    let found = diagnosis(
        &catalog,
        &sources::Sources::default(),
        None,
        Window {
            offset: 0,
            limit: 200,
        },
    );
    let same = found
        .page
        .items
        .iter()
        .find(|item| item.kind == "same_audio")
        .unwrap();
    assert_eq!(same.files.len(), ISSUE_FILE_PREVIEW);
    assert_eq!(same.file_count, ISSUE_FILE_PREVIEW + 2);
    assert!(same.files_truncated);
}
