//! Behavioural tests for authenticated personal-data routes.

use super::*;
use axum::body::Body;
use std::io::{Read, Write};
use std::sync::Arc;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";
static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

fn state() -> ApiState {
    let mut state = crate::test_support::sample_state();
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
    state.data_dir = std::env::temp_dir().join(format!(
        "aede_personal_api_{}_{}_{}",
        std::process::id(),
        nonce,
        sequence
    ));
    state.admin = Some(Admin {
        token: TOKEN.into(),
        data_dir: state.data_dir.clone(),
        scan: Arc::new(|_| Ok(())),
        job: Arc::new(|_, _| Ok(JobOutput::default())),
    });
    store::save_catalog_only(
        state.catalog.try_read().unwrap().as_ref().unwrap(),
        &store::catalog_path(&state.data_dir),
    )
    .unwrap();
    state
}

// Publish a new track without refreshing the server's old in-memory catalog.
async fn add_track_on_disk(state: &ApiState) -> EntityRef {
    let mut catalog = state.catalog.read().await.clone().unwrap();
    let mut file = catalog.files[0].clone();
    file.id = 1;
    file.path = "/music/new-album/01.flac".into();
    file.size = 99;
    catalog.files.push(file);
    let mut track = catalog.tracks[0].clone();
    track.id = 1;
    track.file_id = 1;
    track.release_id = None;
    track.recording_id = 1;
    catalog.tracks.push(track);
    let mut recording = catalog.recordings[0].clone();
    recording.id = 1;
    recording.mbid = Some("new-recording".into());
    recording.track_ids = vec![1];
    recording.work_ids.clear();
    catalog.recordings.push(recording);
    catalog.scanned_at += 1;
    let reference = EntityRef::of(&catalog, EntityKind::Track, 1).unwrap();
    let _guard = StoreLock::acquire(&state.data_dir).unwrap();
    store::save_catalog_only(&catalog, &store::catalog_path(&state.data_dir)).unwrap();
    let mut data = UserData::default();
    data.entry(LOCAL_USER, &reference, 1).note = Some("belongs to the new track".into());
    user::save(&data, &user::user_path(&state.data_dir)).unwrap();
    reference
}

#[test]
fn every_personal_write_preserves_notes_when_the_cached_catalog_is_stale() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let state = state();
        let old_track = track(&state).await;
        for operation in ["collection", "annotation", "history", "delete"] {
            let added = add_track_on_disk(&state).await;
            if operation == "delete" {
                let path = user::user_path(&state.data_dir);
                let mut data = user::load(&path).unwrap().unwrap();
                data.save_collection(LOCAL_USER, "saved", "loved", 1);
                user::save(&data, &path).unwrap();
            }
            match operation {
                "collection" => {
                    let _ = save_collection(State(state.clone()), Ok(Query(NameQuery { name: "saved".into() })),
                        request("/api/admin/v1/collection?name=saved", r#"{"expression":"loved"}"#))
                        .await.unwrap();
                }
                "annotation" => {
                    let _ = update_annotation(State(state.clone()), Ok(Query(RefQuery { reference: old_track.clone() })),
                        request("/api/admin/v1/annotation", r#"{"rating":4}"#)).await.unwrap();
                }
                "history" => {
                    let _ = record_history(State(state.clone()), request("/api/admin/v1/history",
                        &format!(r#"{{"track":"{old_track}","at":100,"ms_played":10,"completed":true}}"#)))
                        .await.unwrap();
                }
                _ => {
                    delete_collection(State(state.clone()), Ok(Query(NameQuery { name: "saved".into() })),
                        request("/api/admin/v1/collection?name=saved", "")).await.unwrap();
                }
            }
            let saved = user::load(&user::user_path(&state.data_dir)).unwrap().unwrap();
            assert_eq!(saved.find(LOCAL_USER, &added).and_then(|item| item.note.as_deref()),
                Some("belongs to the new track"), "{operation} must not reattach a note to the old track");
        }
        cleanup(&state);
    });
}

#[test]
fn personal_targets_are_resolved_against_the_current_disk_catalog() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let state = state();
        let added = add_track_on_disk(&state).await;
        let reference = added.to_token();
        let old_track = track(&state).await;
        let read = annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: old_track,
            })),
            request("/api/admin/v1/annotation", ""),
        )
        .await
        .unwrap();
        assert!(
            read.0.annotation.is_none(),
            "a stale read must not display another track's note"
        );
        let changed = update_annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: reference.clone(),
            })),
            request("/api/admin/v1/annotation", r#"{"rating":5}"#),
        )
        .await
        .unwrap();
        assert_eq!(
            changed.0.annotation.unwrap().note.as_deref(),
            Some("belongs to the new track")
        );
        let _ = record_history(
            State(state.clone()),
            request(
                "/api/admin/v1/history",
                &format!(r#"{{"track":"{reference}","ms_played":10,"completed":true}}"#),
            ),
        )
        .await
        .unwrap();
        // A stale cached entity must also stop being writable once removed on disk.
        store::save_catalog_only(&Catalog::default(), &store::catalog_path(&state.data_dir))
            .unwrap();
        let before = std::fs::read(user::user_path(&state.data_dir)).unwrap();
        let error = update_annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: track(&state).await,
            })),
            request("/api/admin/v1/annotation", r#"{"loved":true}"#),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "entity_not_found");
        let error = record_history(
            State(state.clone()),
            request(
                "/api/admin/v1/history",
                &format!(
                    r#"{{"track":"{}","ms_played":10,"completed":true}}"#,
                    track(&state).await
                ),
            ),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.code, "entity_not_found");
        assert_eq!(
            std::fs::read(user::user_path(&state.data_dir)).unwrap(),
            before
        );
        cleanup(&state);
    });
}

#[test]
fn personal_updates_refuse_unreadable_catalogs_and_busy_stores_without_changing_notes() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let state = state();
        add_track_on_disk(&state).await;
        let path = user::user_path(&state.data_dir);
        let before = std::fs::read(&path).unwrap();
        let guard = StoreLock::acquire(&state.data_dir).unwrap();
        let error = save_collection(
            State(state.clone()),
            Ok(Query(NameQuery {
                name: "saved".into(),
            })),
            request("/api/admin/v1/collection", r#"{"expression":"loved"}"#),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.code, "store_busy");
        drop(guard);
        let catalog_path = store::catalog_path(&state.data_dir);
        std::fs::write(&catalog_path, "invalid JSON").unwrap();
        let error = save_collection(
            State(state.clone()),
            Ok(Query(NameQuery {
                name: "saved".into(),
            })),
            request("/api/admin/v1/collection", r#"{"expression":"loved"}"#),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(catalog_path).unwrap();
        let error = save_collection(
            State(state.clone()),
            Ok(Query(NameQuery {
                name: "saved".into(),
            })),
            request("/api/admin/v1/collection", r#"{"expression":"loved"}"#),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        cleanup(&state);
    });
}

#[test]
fn personal_history_pages_follow_event_time_including_legacy_out_of_order_events() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let state = state();
        let reference = track(&state).await;
        for at in [200, 100, 300] {
            let _ = record_history(
                State(state.clone()),
                request(
                    "/api/admin/v1/history",
                    &format!(
                        r#"{{"track":"{reference}","at":{at},"ms_played":10,"completed":true}}"#
                    ),
                ),
            )
            .await
            .unwrap();
        }
        // Also simulate an existing file saved by the old, arrival-ordered API.
        let path = user::user_path(&state.data_dir);
        let mut saved = user::load(&path).unwrap().unwrap();
        saved.plays.sort_by_key(|play| std::cmp::Reverse(play.at));
        user::save(&saved, &path).unwrap();
        let response = history(
            State(state.clone()),
            Ok(Query(PageQuery {
                offset: Some("0".into()),
                limit: Some("2".into()),
            })),
            request("/api/admin/v1/history", ""),
        )
        .await
        .unwrap();
        assert_eq!(response.0.page.total, 3);
        assert_eq!(
            response.0.page.items[0].at, 300,
            "sorting must happen before pagination"
        );
        assert_eq!(response.0.page.items[0].play_count, 3);
        assert_eq!(response.0.page.items[0].last_played, 300);
        cleanup(&state);
    });
}

fn request(uri: &str, body: &str) -> Request {
    Request::builder()
        .uri(uri)
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn unauthenticated(uri: &str) -> Request {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

async fn release(state: &ApiState) -> String {
    let catalog = state.catalog.read().await;
    EntityRef::of(catalog.as_ref().unwrap(), EntityKind::Release, 0)
        .unwrap()
        .to_token()
}

async fn track(state: &ApiState) -> String {
    let catalog = state.catalog.read().await;
    EntityRef::of(catalog.as_ref().unwrap(), EntityKind::Track, 0)
        .unwrap()
        .to_token()
}

fn cleanup(state: &ApiState) {
    let _ = std::fs::remove_dir_all(&state.data_dir);
}

fn http_request(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> (u16, serde_json::Value) {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

#[test]
fn annotation_patch_updates_one_local_owner_and_explicit_null_removes_fields() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let state = state();
        let reference = release(&state).await;
        let response = update_annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: reference.clone(),
            })),
            request(
                "/api/admin/v1/annotation",
                r#"{"loved":true,"rating":5,"note":"great remaster","tags":["vinyl","rare"]}"#,
            ),
        )
        .await
        .unwrap();
        let saved = response.0.annotation.unwrap();
        assert!(saved.loved);
        assert_eq!(saved.rating, Some(5));
        assert_eq!(saved.tags, ["rare", "vinyl"]);
        let response = update_annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: reference.clone(),
            })),
            request(
                "/api/admin/v1/annotation",
                r#"{"loved":false,"rating":null,"note":null,"tags":null}"#,
            ),
        )
        .await
        .unwrap();
        assert!(response.0.annotation.is_none());
        let data = user::load(&user::user_path(&state.data_dir))
            .unwrap()
            .unwrap();
        assert!(
            data.find(LOCAL_USER, &EntityRef::parse_token(&reference).unwrap())
                .is_none()
        );
        cleanup(&state);
    });
}

#[test]
fn personal_writes_validate_before_touching_the_store_and_require_the_token() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let state = state();
        let reference = release(&state).await;
        let error = update_annotation(
            State(state.clone()),
            Ok(Query(RefQuery { reference })),
            request("/api/admin/v1/annotation?ref=release:x", r#"{"rating":6}"#),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "invalid_parameters");
        let error = annotation(
            State(state.clone()),
            Ok(Query(RefQuery {
                reference: "release:x".into(),
            })),
            unauthenticated("/api/admin/v1/annotation?ref=release:x"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "unauthorized");
        assert!(!user::user_path(&state.data_dir).exists());
        cleanup(&state);
    });
}

#[test]
fn history_and_collections_are_owned_by_the_local_profile() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let state = state();
        let track = track(&state).await;
        let history_response = record_history(
            State(state.clone()),
            request(
                "/api/admin/v1/history",
                &format!(
                    r#"{{"track":"{}","at":100,"ms_played":1234,"completed":true}}"#,
                    track
                ),
            ),
        )
        .await
        .unwrap();
        assert_eq!(history_response.0, StatusCode::CREATED);
        assert_eq!(history_response.1.0.play_count, 1);
        let collection = save_collection(
            State(state.clone()),
            Ok(Query(NameQuery {
                name: "Metal".into(),
            })),
            request(
                "/api/admin/v1/collection?name=Metal",
                r#"{"expression":"genre:metal loved"}"#,
            ),
        )
        .await
        .unwrap();
        assert_eq!(collection.0.name, "Metal");
        let listed = collections(
            State(state.clone()),
            Ok(Query(PageQuery::default())),
            request("/api/admin/v1/collections", ""),
        )
        .await
        .unwrap();
        assert_eq!(listed.0.page.total, 1);
        let deleted = delete_collection(
            State(state.clone()),
            Ok(Query(NameQuery {
                name: "metal".into(),
            })),
            request("/api/admin/v1/collection?name=metal", ""),
        )
        .await
        .unwrap();
        assert_eq!(deleted, StatusCode::NO_CONTENT);
        cleanup(&state);
    });
}

#[test]
fn personal_routes_enforce_authentication_and_strict_query_contracts() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let state = state();
        let reference = release(&state).await;
        let data_dir = state.data_dir.clone();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, crate::router(state, address)).await;
        });
        let encoded = crate::test_support::encoded(&reference);
        let token = format!("Authorization: Bearer {TOKEN}\r\n");

        let (status, body) = tokio::task::spawn_blocking({
            let token = token.clone();
            let encoded = encoded.clone();
            move || {
                http_request(
                    address,
                    "PUT",
                    &format!("/api/admin/v1/annotation?ref={encoded}"),
                    &token,
                    r#"{"loved":true}"#,
                )
            }
        })
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body["annotation"]["loved"], true);

        let (status, body) = tokio::task::spawn_blocking({
            let token = token.clone();
            let encoded = encoded.clone();
            move || {
                http_request(
                    address,
                    "GET",
                    &format!("/api/admin/v1/annotation?ref={encoded}&extra=true"),
                    &token,
                    "",
                )
            }
        })
        .await
        .unwrap();
        assert_eq!(status, 400);
        assert_eq!(body["error"]["code"], "invalid_query");

        let (status, body) = tokio::task::spawn_blocking(move || {
            http_request(address, "GET", "/api/admin/v1/collections", "", "")
        })
        .await
        .unwrap();
        assert_eq!(status, 401);
        assert_eq!(body["error"]["code"], "unauthorized");

        let origin = format!("Authorization: Bearer {TOKEN}\r\nOrigin: http://{address}\r\n");
        let (status, body) = tokio::task::spawn_blocking(move || {
            http_request(address, "GET", "/api/admin/v1/collections", &origin, "")
        })
        .await
        .unwrap();
        assert_eq!(status, 401);
        assert_eq!(body["error"]["code"], "unauthorized");

        let (status, body) = tokio::task::spawn_blocking(move || {
            http_request(
                address,
                "POST",
                "/api/admin/v1/history?unexpected=true",
                &token,
                r#"{"track":"track:0","ms_played":1,"completed":true}"#,
            )
        })
        .await
        .unwrap();
        assert_eq!(status, 400);
        assert_eq!(body["error"]["code"], "invalid_query");

        server.abort();
        let _ = std::fs::remove_dir_all(data_dir);
    });
}
