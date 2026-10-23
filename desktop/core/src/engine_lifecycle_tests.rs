use super::*;
#[path = "engine_lifecycle_test_support.rs"]
mod support;
use support::*;

fn forced_stop(http: bool, disconnect: bool) {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let folder = music(&fixture.0);
    host.configure(available_port(), vec![folder.display().to_string()])
        .unwrap();
    host.start().unwrap();
    let before = catalog(&fixture.0);
    let lock = writer_lock(&fixture.0);
    let parent = server_pid(&fixture.0);
    let mut client = if http {
        host.scan().unwrap();
        None
    } else {
        Some(Client::scan(&fixture.0, &folder))
    };
    let worker = Worker(child_of(parent));
    if http {
        let rows = processes();
        let engine_group = rows.iter().find(|row| row.0 == parent).unwrap().2;
        let worker_group = rows.iter().find(|row| row.0 == worker.0).unwrap().2;
        assert_ne!(
            engine_group, worker_group,
            "HTTP worker must retain its own cancellation group"
        );
    }
    if disconnect {
        drop(client.take());
    }
    assert_eq!(
        catalog(&fixture.0),
        before,
        "writer rendezvous must precede publication"
    );
    let stopped = host.stop().unwrap();
    assert!(!stopped.running);
    let worker_survived = exists(worker.0);
    let at_stop = catalog(&fixture.0);
    drop(lock);
    wait_until("old owned worker did not terminate", || !exists(worker.0));
    let after = catalog(&fixture.0);
    host.start().unwrap();
    let restarted = catalog(&fixture.0);
    host.stop().unwrap();
    assert_eq!(at_stop, before);
    assert_eq!(
        after, before,
        "AE-037: old owned worker published after successful Stop"
    );
    assert_eq!(
        restarted, before,
        "restart must not observe an old engine's late writer"
    );
    assert!(
        !worker_survived,
        "AE-037: successful forced Stop left its owned worker alive"
    );
}

#[test]
fn forced_stop_prevents_delegated_json_scan_publication_after_return_and_restart() {
    forced_stop(false, false);
}
#[test]
fn forced_stop_prevents_disconnected_delegated_json_scan_publication() {
    forced_stop(false, true);
}
#[test]
fn forced_stop_terminates_http_scan_in_its_distinct_process_group() {
    forced_stop(true, false);
}

#[test]
fn completed_work_survives_stop_and_restart() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let folder = music(&fixture.0);
    host.configure(available_port(), vec![folder.display().to_string()])
        .unwrap();
    host.start().unwrap();
    completed_scan(&mut host);
    let before = catalog(&fixture.0);
    assert_eq!(
        serde_json::from_slice::<Value>(&before).unwrap()["file"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    host.stop().unwrap();
    host.start().unwrap();
    assert_eq!(catalog(&fixture.0), before);
    host.stop().unwrap();
}

#[path = "engine_lifecycle_control_tests.rs"]
mod control;

#[path = "engine_network_lifecycle_tests.rs"]
mod network;
