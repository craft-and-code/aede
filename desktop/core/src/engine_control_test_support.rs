//! Lease-only clients remain compatible with additive controlled-engine readiness.
use super::*;

pub(super) struct LeaseEngine {
    pub(super) process: Client,
    pub(super) stdin: std::process::ChildStdin,
    pub(super) stdout: BufReader<std::process::ChildStdout>,
    pub(super) ready: Value,
}

impl LeaseEngine {
    pub(super) fn start(root: &std::path::Path, tls: bool, token: &str) -> Self {
        let identity = read(root).unwrap().identity.unwrap();
        let mut process = Client(
            Command::new(engine())
                .arg(format!("--data={}", root.join("engine").display()))
                .args(["serve", "--port=0"])
                .env("AEDE_DESKTOP_LEASE", "1")
                .env("AEDE_ADMIN_TOKEN", token)
                .env_remove("AEDE_DELEGATED_CHILD")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let mut stdin = process.0.stdin.take().unwrap();
        let mut request = json!({"username":"aede-desktop", "password":identity.password});
        if tls {
            request["tls"] = json!({"certificate":identity.certificate_pem, "private_key":identity.private_key_pem});
        }
        writeln!(stdin, "{request}").unwrap();
        let mut stdout = BufReader::new(process.0.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let ready = serde_json::from_str(&line).unwrap();
        Self {
            process,
            stdin,
            stdout,
            ready,
        }
    }

    pub(super) fn stop(mut self) {
        writeln!(self.stdin, "{{\"desktopCommand\":\"shutdown\"}}").unwrap();
        wait_until("controlled engine did not exit", || {
            self.process.0.try_wait().unwrap().is_some()
        });
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&line).unwrap(),
            json!({"desktopStopped":1})
        );
    }
}
