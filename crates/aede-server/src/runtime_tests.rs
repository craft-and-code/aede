use super::*;
use std::sync::mpsc;

#[test]
fn runtime_shutdown_is_bounded_while_a_blocking_worker_is_stalled() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (entered_at_worker, entered) = mpsc::channel();
    let (release, released_to_worker) = mpsc::channel();
    let (finished_by_worker, finished) = mpsc::channel();
    runtime.spawn_blocking(move || {
        entered_at_worker.send(()).unwrap();
        released_to_worker.recv().unwrap();
        finished_by_worker.send(()).unwrap();
    });
    entered.recv_timeout(Duration::from_secs(1)).unwrap();

    let started = Instant::now();
    shutdown_runtime(runtime, Duration::from_millis(50));
    assert!(started.elapsed() < Duration::from_secs(1));

    // `shutdown_timeout` does not cancel blocking work. Release this fixture
    // and observe its exit so it cannot survive into another test.
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(1)).unwrap();
}
