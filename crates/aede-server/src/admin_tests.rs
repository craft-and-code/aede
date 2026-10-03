use super::*;
use axum::body::Body;

#[test]
fn cancelled_synchronous_scan_waiter_keeps_activity_until_worker_failure() {
    crate::test_support::test_runtime().block_on(async {
        let mut state = crate::test_support::sample_state();
        let dir = state.data_dir.clone();
        let (started, received) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let released = std::sync::Mutex::new(released);
        state.admin = Some(Admin {
            token: "0123456789abcdef0123456789abcdef".into(),
            data_dir: dir.clone(),
            job: Arc::new(|_, _| Err("unexpected job".into())),
            scan: Arc::new(move |_| {
                started.send(()).unwrap();
                released
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Err("deliberate scan failure".into())
            }),
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/admin/v1/scan")
            .header("authorization", "Bearer 0123456789abcdef0123456789abcdef")
            .body(Body::empty())
            .unwrap();
        let waiter = tokio::spawn(admin_scan(State(state.clone()), request));
        tokio::task::spawn_blocking(move || received.recv_timeout(Duration::from_secs(3)).unwrap())
            .await
            .unwrap();
        let active_before_abort = state.scan_activity.is_running();
        waiter.abort();
        let _ = waiter.await;
        let active_after_abort = state.scan_activity.is_running();
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while state.scan_activity.is_running() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // The activity guard may drop just before the file lock during failure.
        let cleanup = dir.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = StoreLock::acquire(&cleanup).unwrap();
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        assert!(active_before_abort);
        assert!(
            active_after_abort,
            "aborting the waiter cannot stop its blocking scan"
        );
    });
}
