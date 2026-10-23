//! Dedicated test-only media fixtures, independent of device implementations.

use super::*;
use std::sync::atomic::AtomicU64;

pub(super) struct Fixture {
    pub folder: PathBuf,
    pub path: PathBuf,
}
impl Fixture {
    pub fn raw_path(&self, parts: &[&str]) -> PathBuf {
        // Preserve the traversal under test on Windows, including verbatim prefixes.
        let mut path = self.folder.as_os_str().to_owned();
        for part in parts {
            path.push(std::path::MAIN_SEPARATOR_STR);
            path.push(part);
        }
        PathBuf::from(path)
    }

    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let folder = std::env::temp_dir().join(format!(
            "aede_device_media_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Example.flac");
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../aede-core/tests/playback_fixtures/flac/playback-stereo.flac"),
            &path,
        )
        .unwrap();
        Self {
            folder: fs::canonicalize(folder).unwrap(),
            path: fs::canonicalize(path).unwrap(),
        }
    }
    pub fn sources(&self) -> Vec<Source> {
        prepare(vec![self.path.clone()]).unwrap()
    }
    pub fn selected(&self) -> SelectedSource {
        let metadata = fs::metadata(&self.path).unwrap();
        let expected = aede_core::user::PlaybackSource::from_metadata(&metadata).unwrap();
        SelectedSource::from_expected(self.path.clone(), &expected).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.folder);
    }
}

pub(super) async fn get(url: &str, method: &str, extra: &str) -> Vec<u8> {
    let (address, path) = url
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap();
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!("{method} /{path} HTTP/1.1\r\nHost: {address}\r\n{extra}\r\n").as_bytes(),
        )
        .await
        .unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut bytes))
        .await
        .unwrap()
        .unwrap();
    bytes
}
