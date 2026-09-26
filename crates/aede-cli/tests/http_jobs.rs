//! Real HTTP administration runs the CLI writer without external requests.

#![cfg(unix)]

use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use aede_core::json::{self, Json};

const TOKEN: &str = "local_http_job_test_secret_32_characters";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("aede_http_jobs_{}_{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Server {
    child: Child,
    address: std::net::SocketAddr,
    output: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn start(data: &Workspace) -> Self {
        let mut child = command(data)
            .args(["serve", "--port=0"])
            .env("AEDE_ADMIN_TOKEN", TOKEN)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let address = line
            .trim()
            .strip_prefix("Aède API: http://")
            .and_then(|line| line.strip_suffix("/api/v1/status"))
            .expect("server readiness")
            .parse()
            .unwrap();
        let output = std::thread::spawn(move || {
            let _ = std::io::copy(&mut stdout, &mut std::io::sink());
        });
        Self {
            child,
            address,
            output: Some(output),
        }
    }

    fn request(&self, method: &str, path: &str, body: &str) -> (u16, Json) {
        let mut stream = std::net::TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(stream, "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", self.address, body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, json::parse(body).unwrap())
    }

    fn submit(&self, route: &str, body: &str) -> String {
        let (status, task) = self.request("POST", route, body);
        assert_eq!(status, 202, "{}", task.to_string_compact());
        task.field_str("status_url").unwrap()
    }

    fn completed(&self, route: &str) -> Json {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (status, task) = self.request("GET", route, "");
            assert_eq!(status, 200);
            if matches!(
                task.field_str("status").as_deref(),
                Some("completed" | "failed" | "cancelled")
            ) {
                return task;
            }
            assert!(
                Instant::now() < deadline,
                "task timed out: {}",
                task.to_string_compact()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status();
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(output) = self.output.take() {
            let _ = output.join();
        }
    }
}

fn command(data: &Workspace) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
    command
        .env("AEDE_HOME", &data.0)
        .env("NO_COLOR", "1")
        .env_remove("AEDE_DELEGATED_CHILD")
        .env_remove("AEDE_DELEGATED_DATA_DIR")
        .env_remove("AEDE_ACOUSTID_KEY")
        .env_remove("AEDE_FANARTTV_KEY");
    command
}

#[test]
fn http_jobs_run_real_scans_and_dry_fetches_and_cancel_a_waiting_writer() {
    let data = Workspace::new();
    let library = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures");
    let initial = command(&data).arg("scan").arg(library).output().unwrap();
    assert!(
        initial.status.success(),
        "{}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let server = Server::start(&data);
    let route = server.submit("/api/admin/v1/scan", r#"{"full":true,"threads":1}"#);
    let task = server.completed(&route);
    assert_eq!(
        task.field_str("status").as_deref(),
        Some("completed"),
        "{}",
        task.to_string_compact()
    );
    assert_eq!(task.get("result").unwrap().field_u64("exit_code"), Some(0));

    #[cfg(feature = "fetch")]
    {
        let route = server.submit("/api/admin/v1/fetch", r#"{"dry_run":true}"#);
        let task = server.completed(&route);
        assert_eq!(
            task.field_str("status").as_deref(),
            Some("completed"),
            "{}",
            task.to_string_compact()
        );
        let result = task.get("result").unwrap();
        assert_eq!(result.field_u64("exit_code"), Some(0));
        assert!(
            result
                .field_str("stdout")
                .unwrap()
                .contains("nothing was asked")
        );
    }

    let before = std::fs::read(data.0.join("catalog.json")).unwrap();
    let held = aede_core::store_lock::StoreLock::acquire(&data.0).unwrap();
    let route = server.submit("/api/admin/v1/scan", r#"{"full":true}"#);
    let (status, _) = server.request("POST", &format!("{route}/cancel"), "");
    assert_eq!(status, 202);
    let task = server.completed(&route);
    assert_eq!(
        task.field_str("status").as_deref(),
        Some("cancelled"),
        "{}",
        task.to_string_compact()
    );
    assert_eq!(
        task.get("result").unwrap().field_u64("exit_code"),
        Some(130)
    );
    assert_eq!(std::fs::read(data.0.join("catalog.json")).unwrap(), before);
    drop(held);
}
