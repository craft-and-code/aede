use aede_desktop_core::Host;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
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
            "aede-legacy-selection-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    pub(super) fn host(&self) -> Host {
        let binary = std::env::var_os("AEDE_DESKTOP_TEST_BINARY")
            .expect("Set the exact bundled binary for engine-tests");
        Host::new(self.0.join("profile"), PathBuf::from(binary)).unwrap()
    }

    pub(super) fn profile(&self) -> PathBuf {
        self.0.join("profile").join("server.json")
    }

    pub(super) fn configuration(&self) -> Value {
        serde_json::from_slice(&fs::read(self.profile()).unwrap()).unwrap()
    }

    pub(super) fn catalog(&self) -> Value {
        serde_json::from_slice(
            &fs::read(self.0.join("profile").join("engine").join("catalog.json")).unwrap(),
        )
        .unwrap()
    }

    pub(super) fn seed_legacy_profile(&self) -> Vec<String> {
        drop(self.host());
        let music = self.0.join("music");
        fs::create_dir(&music).unwrap();
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
        fs::write(music.join("test.wav"), wav).unwrap();

        let mut roots = vec![music.to_str().unwrap().to_owned()];
        for index in 0..7 {
            let mut path = self.0.join(format!("absent-volume-{index}"));
            for component in 0..17 {
                path = path.join(format!("{component}-{}", "a".repeat(120)));
            }
            roots.push(path.to_str().unwrap().to_owned());
        }
        let mut configuration = self.configuration();
        configuration["port"] = json!(
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        );
        configuration["roots"] = json!(roots);
        let bytes = serde_json::to_vec(&configuration).unwrap();
        assert!(bytes.len() < 128 * 1024);
        assert!(roots.iter().all(|root| root.len() <= 4096));
        fs::write(self.profile(), bytes).unwrap();
        roots
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn completed_scan(host: &mut Host) {
    let admitted = host.scan().unwrap();
    let id = admitted["task_id"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let task = host.task(id, false).unwrap();
        match task["status"].as_str().unwrap() {
            "queued" | "running" => {}
            "completed" => {
                assert_eq!(task["result"]["exit_code"], 0, "{task}");
                return;
            }
            _ => panic!("scan did not complete: {task}"),
        }
        assert!(Instant::now() < deadline, "scan timed out: {task}");
        std::thread::sleep(Duration::from_millis(25));
    }
}
