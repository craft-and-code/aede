#[cfg(unix)]
use super::*;

#[cfg(unix)]
fn test_server(control: bool, mut command: Command) -> Server {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let lease = child.stdin.take();
    let (_sender, stopped) = std::sync::mpsc::channel();
    Server {
        child,
        lease,
        control,
        stopping: false,
        stopped,
        confirmed: false,
        address: String::new(),
        token: None,
        agent: ureq::Agent::config_builder().proxy(None).build().into(),
    }
}

#[cfg(unix)]
#[test]
fn legacy_engine_without_control_capability_is_never_force_killed() {
    let mut command = Command::new("sleep");
    command.arg("20");
    let mut server = test_server(false, command);
    let result = server.stop();
    let still_owned = server.running().unwrap();
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    server.stop().unwrap();
    assert!(result.is_err());
    assert!(
        still_owned,
        "legacy drain timeout must retain the live owned engine"
    );
}

#[cfg(unix)]
#[test]
fn engine_exit_without_worker_acknowledgement_does_not_confirm_stop() {
    let mut server = test_server(true, Command::new("true"));
    server.child.wait().unwrap();
    assert!(server.stop().is_err());
    assert!(!server.confirmed);
}
