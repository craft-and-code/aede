use aede_desktop_core::Host;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub(super) struct Fixture(pub(super) PathBuf);

impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aede-unavailable-roots-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    pub(super) fn host(&self) -> Host {
        Host::new(self.0.join("profile"), engine()).unwrap()
    }

    pub(super) fn catalog(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.0.join("profile").join("engine").join("catalog.json")).unwrap(),
        )
        .unwrap()
    }

    pub(super) fn command(&self, target: &Path) -> Command {
        let mut command = Command::new(engine());
        command
            .arg("scan")
            .arg("--replace")
            .arg("--dry-run")
            .arg("--json")
            .arg("--threads=1")
            .arg("--data")
            .arg(self.0.join("cli-data"))
            .arg("--")
            .arg(target)
            .env("NO_COLOR", "1");
        for name in [
            "AEDE_DESKTOP_LEASE",
            "AEDE_DELEGATED_CHILD",
            "AEDE_DELEGATED_DATA_DIR",
            "AEDE_UNAVAILABLE_ROOTS",
            "AEDE_ADMIN_TOKEN",
        ] {
            command.env_remove(name);
        }
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn engine() -> PathBuf {
    PathBuf::from(
        std::env::var_os("AEDE_DESKTOP_TEST_BINARY")
            .expect("Set the exact bundled binary for engine-tests"),
    )
}

pub(super) fn available_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub(super) fn write_wav(path: &Path) {
    let length = 4410u32 * 2;
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
    fs::write(path, wav).unwrap();
}

pub(super) fn completed_scan(host: &mut Host) -> Value {
    let accepted = host.scan().unwrap();
    let id = accepted["task_id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => {}
            "completed" => {
                assert_eq!(task["result"]["exit_code"], 0, "{task}");
                return task;
            }
            _ => panic!("scan did not complete: {task}"),
        }
        assert!(Instant::now() < deadline, "scan timed out: {task}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn assert_catalog(host: &mut Host, catalog: &Value, expected: &[PathBuf]) {
    let paths: BTreeSet<PathBuf> = catalog["file"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| PathBuf::from(file["path"].as_str().unwrap()))
        .collect();
    assert_eq!(paths, expected.iter().cloned().collect());
    assert_eq!(catalog["track"].as_array().unwrap().len(), expected.len());
    let diagnostics = serde_json::to_value(host.diagnostics().unwrap()).unwrap();
    assert_eq!(diagnostics["catalogLoaded"], true, "{diagnostics}");
    assert_eq!(diagnostics["apiResponding"], true, "{diagnostics}");
    assert_eq!(diagnostics["library"]["tracks"], expected.len());
    assert_eq!(diagnostics["library"]["files"], expected.len());
}
