use super::*;
fn available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn engine() -> PathBuf {
    PathBuf::from(
        std::env::var("AEDE_DESKTOP_TEST_BINARY")
            .expect("Set the exact bundled binary for engine-tests"),
    )
}
#[test]
fn real_https_server_authenticates_exports_only_public_identity_and_stops() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let port = available_port();
    host.configure(port, vec![]).unwrap();
    assert!(host.start().unwrap().running);
    let profile: Value = serde_json::from_str(&host.connection_profile().unwrap()).unwrap();
    assert_eq!(profile["address"], format!("https://127.0.0.1:{port}"));
    assert!(profile["certificate"].as_array().unwrap().len() > 100);
    assert!(profile.get("password").is_none());
    assert!(profile.get("private_key").is_none());
    let first_password = host.password().unwrap();
    assert!(!host.stop().unwrap().running);
    assert!(host.start().unwrap().running);
    assert_eq!(host.password().unwrap(), first_password);
    host.stop().unwrap();
}
#[test]
fn busy_port_refuses_start_without_adopting_another_server() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    host.configure(listener.local_addr().unwrap().port(), vec![])
        .unwrap();
    assert!(host.start().is_err());
    assert!(!host.status().unwrap().running);
}
#[test]
fn expired_administration_session_reauthenticates_without_restarting_server() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    host.start().unwrap();
    host.process.as_mut().unwrap().token = Some("0".repeat(64));
    assert!(host.request("GET", "/api/v1/status", None).is_ok());
    assert!(host.status().unwrap().running);
}
#[test]
fn explicit_scan_publishes_audio_and_reports_completion() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let folder = fixture.0.join("music");
    fs::create_dir(&folder).unwrap();
    let samples = 4410u32;
    let length = samples * 2;
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + length).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(44100u32.to_le_bytes());
    wav.extend(88200u32.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(length.to_le_bytes());
    wav.resize(44 + length as usize, 0);
    fs::write(folder.join("test.wav"), wav).unwrap();
    host.configure(available_port(), vec![folder.display().to_string()])
        .unwrap();
    host.start().unwrap();
    let accepted = host.scan().unwrap();
    let id = accepted["task_id"].as_u64().unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => {}
            "completed" => break,
            other => panic!("unexpected task state: {other}: {task}"),
        };
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let tracks = host.request("GET", "/api/v1/tracks", None).unwrap();
    assert_eq!(tracks["items"].as_array().unwrap().len(), 1);
    let diagnostics = serde_json::to_value(host.diagnostics().unwrap()).unwrap();
    assert_eq!(diagnostics["catalogLoaded"], true);
    assert_eq!(diagnostics["library"]["tracks"], 1);
}

#[cfg(feature = "phemios-acceptance")]
#[test]
fn phemios_imports_the_exported_profile_and_authenticates_the_real_server() {
    use std::io::Write;
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    host.configure(available_port(), vec![]).unwrap();
    host.start().unwrap();
    let profile = fixture.0.join("connection.json");
    fs::write(&profile, host.connection_profile().unwrap()).unwrap();
    let probe = std::env::var_os("AEDE_DESKTOP_PHEMIOS_PROBE")
        .expect("Set the built Phémios server_profile_probe example");
    let mut child = std::process::Command::new(probe)
        .arg(profile)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(host.password().unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Phémios probe refused the local server: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["authenticated"], true);
    assert_eq!(result["server"], host.status().unwrap().address.unwrap());
}

#[test]
fn running_server_accepts_folder_edits_but_refuses_port_changes() {
    let fixture = Fixture::new();
    let mut host = Host::new(fixture.0.clone(), engine()).unwrap();
    let port = available_port();
    host.configure(port, vec![]).unwrap();
    host.start().unwrap();
    let roots = vec![fixture.0.join("new-music").display().to_string()];
    let status = host.configure(port, roots.clone()).unwrap();
    assert!(status.running);
    assert_eq!(status.roots[0].path, roots[0]);
    let before = fs::read(fixture.0.join("server.json")).unwrap();
    assert!(
        host.configure(if port == 65535 { port - 1 } else { port + 1 }, vec![])
            .is_err()
    );
    assert_eq!(fs::read(fixture.0.join("server.json")).unwrap(), before);
    let config = read(&fixture.0).unwrap();
    let agent = crate::http::agent(config.identity.as_ref().unwrap());
    let root = agent
        .get(format!("https://127.0.0.1:{port}/"))
        .call()
        .unwrap();
    assert_eq!(root.status().as_u16(), 200);
    assert!(
        root.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    let denied = agent
        .get(format!("https://127.0.0.1:{port}/api/v1/status?key=secret"))
        .call()
        .unwrap();
    assert_eq!(denied.status().as_u16(), 401);
    let diagnostics = serde_json::to_value(host.diagnostics().unwrap()).unwrap();
    assert_eq!(diagnostics["apiResponding"], true);
    assert_eq!(diagnostics["catalogLoaded"], true);
    assert_eq!(diagnostics["library"]["tracks"], 0);
    // Log delivery is asynchronous; wait for the actual HTTP summary, not just a lifecycle entry.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let snapshot = serde_json::to_string(&host.logs(0).unwrap()).unwrap();
        assert!(!snapshot.contains(&host.password().unwrap()));
        assert!(!snapshot.contains("key=secret"));
        if snapshot.contains("/api/v1/status") && snapshot.contains("401") {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "missing HTTP summary: {snapshot}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    host.stop().unwrap();
    assert!(
        !serde_json::to_value(host.logs(0).unwrap()).unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        serde_json::to_value(host.diagnostics().unwrap()).unwrap()["running"],
        false
    );
}

#[path = "proxy_tests.rs"]
mod proxy;

#[path = "selection_engine_tests.rs"]
mod selection;

#[cfg(unix)]
#[path = "engine_lifecycle_tests.rs"]
mod lifecycle;
