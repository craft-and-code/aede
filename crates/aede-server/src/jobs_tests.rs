use super::*;
use crate::Admin;
use std::io::{Read, Write};
use std::sync::atomic::AtomicUsize;

const TOKEN: &str = "a_private_test_token_of_at_least_32_characters";

fn state(
    run: impl Fn(JobRequest, Arc<AtomicBool>) -> Result<JobOutput, String> + Send + Sync + 'static,
) -> ApiState {
    let mut state = crate::test_support::sample_state();
    state.admin = Some(Admin {
        token: TOKEN.into(),
        data_dir: std::env::temp_dir().join(format!("aede_job_no_catalog_{}", std::process::id())),
        scan: Arc::new(|_| Ok(())),
        job: Arc::new(run),
    });
    state
}

fn request(method: &str, uri: &str, body: impl Into<Body>) -> Request {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(body.into())
        .unwrap()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn accepted_id(response: Response) -> u64 {
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = to_bytes(response.into_body(), MAX_BODY).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = value["task_id"].as_u64().unwrap();
    assert_eq!(value["status_url"], format!("/api/admin/v1/tasks/{id}"));
    id
}

#[test]
fn every_job_route_requires_the_token_and_refuses_browser_origins() {
    runtime().block_on(async {
        let called = Arc::new(AtomicUsize::new(0));
        let count = called.clone();
        let state = state(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok(JobOutput::default())
        });
        for browser in [false, true] {
            for (method, path) in [
                ("POST", "/api/admin/v1/scan"),
                ("POST", "/api/admin/v1/fetch"),
                ("GET", "/api/admin/v1/tasks/1"),
                ("POST", "/api/admin/v1/tasks/1/cancel"),
            ] {
                let mut req = request(method, path, "{}");
                if browser {
                    req.headers_mut()
                        .insert("origin", "http://localhost".parse().unwrap());
                } else {
                    req.headers_mut().remove("authorization");
                }
                let error = match path {
                    "/api/admin/v1/scan" => scan_route(State(state.clone()), req).await.err(),
                    "/api/admin/v1/fetch" => fetch_route(State(state.clone()), req).await.err(),
                    "/api/admin/v1/tasks/1" => {
                        task_route(State(state.clone()), Path("1".into()), req)
                            .await
                            .err()
                    }
                    _ => cancel_route(State(state.clone()), Path("1".into()), req)
                        .await
                        .err(),
                }
                .expect("unauthorized route");
                assert_eq!(error.status, StatusCode::UNAUTHORIZED);
            }
        }
        assert_eq!(called.load(Ordering::Relaxed), 0);
    });
}

#[test]
fn invalid_json_options_are_rejected_before_any_job_starts() {
    runtime().block_on(async {
        let called = Arc::new(AtomicUsize::new(0));
        let count = called.clone();
        let state = state(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok(JobOutput::default())
        });
        for payload in [
            "",
            "[]",
            "null",
            "{",
            r#"{"command":"reset"}"#,
            r#"{"data":"/tmp/other"}"#,
            r#"{"full":1}"#,
            r#"{"full":true,"full":false}"#,
            r#"{"size":"500"}"#,
            r#"{"covers":true,"size":"999"}"#,
            r#"{"banners":true}"#,
            r#"{"no_logo":true}"#,
            r#"{"fanart":true,"logos":true,"no_logo":true}"#,
            r#"{"targets":[""]}"#,
        ] {
            let error = fetch_route(
                State(state.clone()),
                request("POST", "/api/admin/v1/fetch", payload),
            )
            .await
            .expect_err(payload);
            assert_eq!(error.status, StatusCode::BAD_REQUEST, "{payload}");
        }
        for payload in [
            r#"{"threads":65}"#,
            r#"{"replace":true}"#,
            r#"{"folders":["relative/path"]}"#,
            r#"{"data":"/tmp/other"}"#,
        ] {
            let error = scan_route(
                State(state.clone()),
                request("POST", "/api/admin/v1/scan", payload),
            )
            .await
            .expect_err(payload);
            assert_eq!(error.status, StatusCode::BAD_REQUEST, "{payload}");
        }
        let oversized = " ".repeat(MAX_BODY + 1);
        let error = fetch_route(
            State(state.clone()),
            request("POST", "/api/admin/v1/fetch", oversized),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.code, "invalid_body");
        assert_eq!(called.load(Ordering::Relaxed), 0);
        assert!(state.jobs.data.lock().unwrap().tasks.is_empty());
    });
}

#[test]
fn parameterized_scans_and_fetches_use_the_typed_requests() {
    runtime().block_on(async {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let state = state(move |request, _| { received.lock().unwrap().push(request); Ok(JobOutput::default()) });
        let scan = scan_route(State(state.clone()), request("POST", "/api/admin/v1/scan", r#"{"folders":["/music/Other"],"replace":true,"full":true,"threads":2,"include_hidden":true,"follow_symlinks":true}"#)).await.unwrap_or_else(|_| panic!("accepted scan"));
        let scan_id = accepted_id(scan).await;
        let fetch = fetch_route(State(state.clone()), request("POST", "/api/admin/v1/fetch", r#"{"targets":["AC/DC","--data=/other"],"covers":true,"size":"1200","lang":"fr,en","yes":true,"dry_run":true}"#)).await.unwrap_or_else(|_| panic!("accepted fetch"));
        let fetch_id = accepted_id(fetch).await;
        wait_for_jobs(&state).await;
        assert_ne!(scan_id, fetch_id);
        let requests = requests.lock().unwrap();
        assert!(requests.iter().any(|job| matches!(job, JobRequest::Scan(scan) if scan.replace && scan.full && scan.include_hidden && scan.follow_symlinks && scan.threads == Some(2) && scan.folders == ["/music/Other"])));
        assert!(requests.iter().any(|job| matches!(job, JobRequest::Fetch(fetch) if fetch.covers && fetch.yes && fetch.dry_run && fetch.targets == ["AC/DC", "--data=/other"] && fetch.size.as_deref() == Some("1200") && fetch.lang.as_deref() == Some("fr,en"))));
        assert_eq!(state.jobs.get(scan_id).unwrap_or_else(|_| panic!("scan status")).status, Status::Completed);
    });
}

#[test]
fn an_empty_json_scan_object_starts_an_asynchronous_watched_root_scan() {
    runtime().block_on(async {
        let state = state(|job, _| {
            let JobRequest::Scan(scan) = job else {
                panic!("expected scan")
            };
            assert!(scan.folders.is_empty());
            assert!(!scan.replace);
            Ok(JobOutput::default())
        });
        let response = scan_route(
            State(state.clone()),
            request("POST", "/api/admin/v1/scan", "{}"),
        )
        .await
        .unwrap_or_else(|_| panic!("async watched scan"));
        let id = accepted_id(response).await;
        wait_for_jobs(&state).await;
        assert_eq!(
            state
                .jobs
                .get(id)
                .unwrap_or_else(|_| panic!("task status"))
                .status,
            Status::Completed
        );
    });
}

#[test]
fn dropping_the_http_response_does_not_cancel_accepted_work() {
    runtime().block_on(async {
        let released = Arc::new(AtomicBool::new(false));
        let proceed = released.clone();
        let state = state(move |_, _| {
            while !proceed.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(JobOutput {
                stdout: "completed independently".into(),
                ..Default::default()
            })
        });
        let response = fetch_route(
            State(state.clone()),
            request("POST", "/api/admin/v1/fetch", "{}"),
        )
        .await
        .unwrap_or_else(|_| panic!("accepted fetch"));
        drop(response);
        released.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(2), wait_for_jobs(&state))
            .await
            .unwrap();
        let task = state
            .jobs
            .get(1)
            .unwrap_or_else(|_| panic!("retained task"));
        assert_eq!(task.status, Status::Completed);
        assert_eq!(task.result.unwrap().stdout, "completed independently");
    });
}

#[test]
fn authenticated_cancellation_is_visible_until_a_terminal_result() {
    runtime().block_on(async {
        let state = state(move |_, cancelled| {
            while !cancelled.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(JobOutput {
                exit_code: 130,
                stdout: "previous answers remain saved".into(),
                ..Default::default()
            })
        });
        let response = fetch_route(
            State(state.clone()),
            request("POST", "/api/admin/v1/fetch", "{}"),
        )
        .await
        .unwrap_or_else(|_| panic!("accepted fetch"));
        let id = accepted_id(response).await;
        let response = cancel_route(
            State(state.clone()),
            Path(id.to_string()),
            request("POST", &format!("/api/admin/v1/tasks/{id}/cancel"), ""),
        )
        .await
        .unwrap_or_else(|_| panic!("cancellable task"));
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        tokio::time::timeout(Duration::from_secs(2), wait_for_jobs(&state))
            .await
            .unwrap();
        let Json(task) = task_route(
            State(state.clone()),
            Path(id.to_string()),
            request("GET", &format!("/api/admin/v1/tasks/{id}"), ""),
        )
        .await
        .unwrap_or_else(|_| panic!("task status"));
        assert_eq!(task.status, Status::Cancelled);
        assert!(task.cancel_requested);
        assert_eq!(task.result.unwrap().exit_code, 130);
        let error = cancel_route(
            State(state),
            Path(id.to_string()),
            request("POST", "/api/admin/v1/tasks/1/cancel", ""),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.status, StatusCode::CONFLICT);
    });
}

#[test]
fn callback_failure_and_panic_finish_the_task_and_release_capacity() {
    runtime().block_on(async {
        let state = state(|request, _| match request {
            JobRequest::Scan(_) => Err("mock failure".into()),
            JobRequest::Fetch(_) => panic!("mock worker panic"),
        });
        let mut events = state.events.subscribe();
        let _ = enqueue(state.clone(), JobRequest::Scan(ScanRequest::default()))
            .unwrap_or_else(|_| panic!("queued"));
        let _ = enqueue(state.clone(), JobRequest::Fetch(FetchRequest::default()))
            .unwrap_or_else(|_| panic!("queued"));
        tokio::time::timeout(Duration::from_secs(2), wait_for_jobs(&state))
            .await
            .unwrap();
        for id in [1, 2] {
            assert_eq!(
                state
                    .jobs
                    .get(id)
                    .unwrap_or_else(|_| panic!("failed task"))
                    .status,
                Status::Failed
            );
        }
        assert_eq!(
            state
                .jobs
                .get(1)
                .unwrap_or_else(|_| panic!("failure detail"))
                .error
                .unwrap()
                .message,
            "mock failure"
        );
        let mut failures = 0;
        while failures < 2 {
            if let CatalogEvent::TaskFailed { message, .. } =
                tokio::time::timeout(Duration::from_secs(1), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
            {
                assert!(!message.contains("mock failure"));
                assert!(!message.contains("mock worker panic"));
                failures += 1;
            }
        }
        assert_eq!(state.jobs.data.lock().unwrap().active, 0);
    });
}

#[test]
fn task_capacity_and_retention_are_bounded_and_running_tasks_are_kept() {
    let registry = JobRegistry::default();
    for id in 1..=MAX_ACTIVE as u64 {
        registry
            .insert(id, "scan")
            .unwrap_or_else(|_| panic!("available capacity"));
    }
    assert_eq!(
        registry.insert(100, "scan").err().unwrap().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    for id in 2..=MAX_ACTIVE as u64 {
        registry
            .finish(id, Ok(JobOutput::default()))
            .unwrap_or_else(|_| panic!("finished"));
    }
    for id in (MAX_ACTIVE + 1) as u64..=100 {
        registry
            .insert(id, "scan")
            .unwrap_or_else(|_| panic!("available capacity"));
        registry
            .finish(id, Ok(JobOutput::default()))
            .unwrap_or_else(|_| panic!("finished"));
    }
    assert_eq!(registry.data.lock().unwrap().tasks.len(), MAX_RETAINED);
    assert_eq!(
        registry
            .get(1)
            .unwrap_or_else(|_| panic!("running task retained"))
            .status,
        Status::Queued
    );
    assert_eq!(registry.get(2).err().unwrap().code, "task_not_found");
    registry
        .finish(
            1,
            Ok(JobOutput {
                stdout: "é".repeat(MAX_OUTPUT),
                ..Default::default()
            }),
        )
        .unwrap_or_else(|_| panic!("bounded output"));
    let output = registry
        .get(1)
        .unwrap_or_else(|_| panic!("result"))
        .result
        .unwrap();
    assert!(output.stdout.len() <= MAX_OUTPUT);
    assert!(output.output_truncated);
}

fn http_request(
    address: std::net::SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    authenticated: bool,
) -> (u16, serde_json::Value) {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let auth = if authenticated {
        format!("Authorization: Bearer {TOKEN}\r\n")
    } else {
        String::new()
    };
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n{auth}\r\n{body}", body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (header, body) = response.split_once("\r\n\r\n").unwrap();
    let status = header.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, serde_json::from_str(body).unwrap())
}

#[test]
fn job_routes_are_opt_in_and_round_trip_through_the_http_router() {
    runtime().block_on(async {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(
                listener,
                crate::router(crate::test_support::sample_state(), address),
            )
            .into_future(),
        );
        for (method, path, payload) in [
            ("POST", "/api/admin/v1/fetch", "{}"),
            ("GET", "/api/admin/v1/tasks/1", ""),
            ("POST", "/api/admin/v1/tasks/1/cancel", ""),
        ] {
            let (status, body) = http_request(address, method, path, payload, true);
            assert_eq!(status, 404);
            assert_eq!(body["error"]["code"], "not_found");
        }
        server.abort();
        let state = state(|_, _| {
            Ok(JobOutput {
                stdout: "dry-run result".into(),
                ..Default::default()
            })
        });
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(listener, crate::router(state.clone(), address)).into_future(),
        );
        for path in ["/api/admin/v1/scan", "/api/admin/v1/fetch"] {
            let (status, _) = http_request(address, "POST", path, "{}", false);
            assert_eq!(status, 401);
            let (status, body) = http_request(address, "POST", path, "{}", true);
            assert_eq!(status, 202);
            let status_url = body["status_url"].as_str().unwrap();
            wait_for_jobs(&state).await;
            assert_eq!(http_request(address, "GET", status_url, "", false).0, 401);
            let (status, result) = http_request(address, "GET", status_url, "", true);
            assert_eq!(status, 200);
            assert_eq!(result["status"], "completed");
            assert_eq!(result["result"]["stdout"], "dry-run result");
        }
        server.abort();
    });
}
