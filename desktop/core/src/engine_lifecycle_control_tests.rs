use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn owner_eof_drains_accepted_disconnected_scan_without_cancelling_it() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let folder = music(&fixture.0);
    let port = available_port();
    host.configure(port, vec![folder.display().to_string()])
        .unwrap();
    host.start().unwrap();
    host.stop().unwrap();
    let configuration = read(&fixture.0).unwrap();
    let identity = configuration.identity.unwrap();
    let mut engine = Client(
        Command::new(engine())
            .arg(format!("--data={}", fixture.0.join("engine").display()))
            .args(["serve", &format!("--port={port}")])
            .env("AEDE_DESKTOP_LEASE", "1")
            .env_remove("AEDE_ADMIN_TOKEN")
            .env_remove("AEDE_DELEGATED_CHILD")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut lease = engine.0.stdin.take().unwrap();
    writeln!(
        lease,
        "{}",
        json!({"username":"aede-desktop", "password":identity.password,
        "tls":{"certificate":identity.certificate_pem,"private_key":identity.private_key_pem}})
    )
    .unwrap();
    let mut output = BufReader::new(engine.0.stdout.take().unwrap());
    let mut ready = String::new();
    output.read_line(&mut ready).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&ready).unwrap()["desktopProtocol"],
        2
    );
    let before = catalog(&fixture.0);
    let lock = writer_lock(&fixture.0);
    let client = Client::scan(&fixture.0, &folder);
    let worker = Worker(child_of(engine.0.id()));
    drop(client);
    drop(lease);
    wait_until("EOF did not stop new admission", || {
        std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
    });
    assert!(
        engine.0.try_wait().unwrap().is_none(),
        "EOF must keep the engine owning accepted work"
    );
    assert!(
        processes()
            .iter()
            .any(|row| row.0 == worker.0 && row.1 == engine.0.id())
    );
    assert_eq!(catalog(&fixture.0), before);
    drop(lock);
    wait_until("graceful EOF did not finish accepted work", || {
        engine.0.try_wait().unwrap().is_some()
    });
    assert!(!exists(worker.0));
    let after: Value = serde_json::from_slice(&catalog(&fixture.0)).unwrap();
    assert_eq!(after["file"].as_array().unwrap().len(), 1);
}

#[test]
fn failed_stop_keeps_ownership_and_can_retry_without_starting_a_second_engine() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let folder = music(&fixture.0);
    host.configure(available_port(), vec![folder.display().to_string()])
        .unwrap();
    host.start().unwrap();
    let lock = writer_lock(&fixture.0);
    let _client = Client::scan(&fixture.0, &folder);
    let parent = server_pid(&fixture.0);
    let worker = Worker(child_of(parent));
    let engine_cleanup = Worker(parent);
    assert!(
        Command::new("/bin/kill")
            .args(["-STOP", &parent.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let start = Instant::now();
    let result = host.stop();
    let retained = host.process.is_some();
    let running = host.start().unwrap().running;
    let same_engine = server_pid(&fixture.0) == parent;
    assert!(
        Command::new("/bin/kill")
            .args(["-CONT", &parent.to_string()])
            .status()
            .unwrap()
            .success()
    );
    host.stop().unwrap();
    drop(lock);
    assert!(
        result.is_err(),
        "unconfirmed forced stop must report failure"
    );
    assert!(
        start.elapsed() < Duration::from_secs(14),
        "stop failure must be bounded"
    );
    assert!(
        retained && running && same_engine,
        "failed stop must retain the same owned engine"
    );
    assert!(!exists(worker.0));
    drop(engine_cleanup);
}

#[test]
fn forced_stop_does_not_terminate_an_unrelated_process_group() {
    let mut unrelated = Command::new("sleep");
    use std::os::unix::process::CommandExt;
    unrelated.arg("30").process_group(0);
    let mut unrelated = Client(unrelated.spawn().unwrap());
    forced_stop(false, false);
    assert!(unrelated.0.try_wait().unwrap().is_none());
}

#[path = "engine_control_test_support.rs"]
mod lease_support;

#[test]
fn controlled_protocol_one_preserves_explicit_admin_token_while_https_refuses_it() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    host.start().unwrap();
    host.stop().unwrap();
    let token = "controlled-engine-test-token-1234567890";
    for tls in [false, true] {
        let lease = lease_support::LeaseEngine::start(&fixture.0, tls, token);
        assert_eq!(lease.ready["desktopControl"], 1);
        assert_eq!(lease.ready["desktopProtocol"], if tls { 2 } else { 1 });
        let identity = read(&fixture.0).unwrap().identity.unwrap();
        let agent = crate::http::agent(&identity);
        let address = lease.ready["address"].as_str().unwrap();
        let response = agent
            .get(format!("{address}/api/admin/v1/tasks"))
            .header("Authorization", &format!("Bearer {token}"))
            .call()
            .unwrap();
        assert_eq!(response.status().as_u16(), if tls { 401 } else { 200 });
        let anonymous = agent
            .get(format!("{address}/api/admin/v1/tasks"))
            .call()
            .unwrap();
        assert_eq!(anonymous.status().as_u16(), 401);
        lease.stop();
    }
}
