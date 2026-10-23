//! Real-process rendezvous and isolated writer contention for lifecycle regressions.
use super::*;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn wait_until(message: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !ready() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn processes() -> Vec<(u32, u32, u32, String)> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid=,command="])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((
                fields.next()?.parse().ok()?,
                fields.next()?.parse().ok()?,
                fields.next()?.parse().ok()?,
                fields.collect::<Vec<_>>().join(" "),
            ))
        })
        .collect()
}

pub(super) fn exists(pid: u32) -> bool {
    processes().iter().any(|row| row.0 == pid)
}

pub(super) fn child_of(parent: u32) -> u32 {
    let mut child = None;
    wait_until("owned scan child was not accepted", || {
        child = processes()
            .iter()
            .find(|row| row.1 == parent && row.3.contains("scan"))
            .map(|row| row.0);
        child.is_some()
    });
    child.unwrap()
}

pub(super) fn server_pid(root: &std::path::Path) -> u32 {
    let needle = format!("--data={}", root.join("engine").display());
    processes()
        .iter()
        .find(|row| {
            row.1 == std::process::id() && row.3.contains(&needle) && row.3.contains("serve")
        })
        .unwrap()
        .0
}

pub(super) fn music(root: &std::path::Path) -> PathBuf {
    let folder = root.join("music");
    fs::create_dir_all(&folder).unwrap();
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
    fs::write(folder.join("test.wav"), wav).unwrap();
    folder
}

pub(super) fn writer_lock(root: &std::path::Path) -> fs::File {
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("engine/.aede.lock"))
        .unwrap();
    lock.lock().unwrap();
    lock
}

pub(super) struct Client(pub(super) Child);
impl Client {
    pub(super) fn scan(root: &std::path::Path, folder: &std::path::Path) -> Self {
        Self(
            Command::new(engine())
                .arg(format!("--data={}", root.join("engine").display()))
                .args(["scan", "--json", "--threads=1"])
                .arg(folder)
                .env_remove("AEDE_DESKTOP_LEASE")
                .env_remove("AEDE_DELEGATED_CHILD")
                .env_remove("AEDE_DELEGATED_DATA_DIR")
                .env_remove("AEDE_UNAVAILABLE_ROOTS")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) struct Worker(pub(super) u32);
impl Drop for Worker {
    fn drop(&mut self) {
        if exists(self.0) {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &self.0.to_string()])
                .status();
        }
    }
}

pub(super) fn catalog(root: &std::path::Path) -> Vec<u8> {
    fs::read(root.join("engine/catalog.json")).unwrap()
}

pub(super) fn completed_scan(host: &mut Host) {
    let id = host.scan().unwrap()["task_id"].as_u64().unwrap();
    wait_until("scan did not complete", || {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => false,
            "completed" => true,
            state => panic!("scan ended as {state}: {task}"),
        }
    });
}
