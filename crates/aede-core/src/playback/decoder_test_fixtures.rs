//! Isolated inputs for the public float-decoder regressions.

use std::path::{Path, PathBuf};

pub(super) struct TemporarySource {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporarySource {
    pub(super) fn new(bytes: &[u8]) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for attempt in 0..100 {
            let directory = std::env::temp_dir().join(format!(
                "aede_float_decoder_{}_{nonce}_{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    let source = Self {
                        path: directory.join("source.flac"),
                        directory,
                    };
                    std::fs::write(&source.path, bytes).unwrap();
                    return source;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("isolated decoder fixture: {error}"),
            }
        }
        panic!("could not reserve an isolated decoder fixture directory");
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporarySource {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(super) fn prefixed_surround_flac(optional_picture: bool) -> Vec<u8> {
    let original = include_bytes!("../../tests/playback_fixtures/flac/playback-5_1.flac");
    assert_eq!(&original[..8], b"fLaC\0\0\0\x22");
    let mut audio_start = 4;
    loop {
        let header = &original[audio_start..audio_start + 4];
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        audio_start += 4 + length;
        if header[0] & 0x80 != 0 {
            break;
        }
    }
    let prefix = b"harmless leading bytes\0\x11\x22\x33";
    assert_eq!(prefix.len(), 26);
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(&original[..42]);
    bytes[prefix.len() + 4] = if optional_picture { 0 } else { 0x80 };
    if optional_picture {
        // The declared eight-byte body is present; its unused nested MIME
        // length is deliberately inconsistent and must never affect playback.
        bytes.extend_from_slice(&[0x86, 0, 0, 8]);
        bytes.extend_from_slice(&3_u32.to_be_bytes());
        bytes.extend_from_slice(&65_537_u32.to_be_bytes());
    }
    bytes.extend_from_slice(&original[audio_start..]);
    bytes
}

#[cfg(unix)]
pub(super) fn wait_for_child(
    child: std::process::Child,
    timeout: std::time::Duration,
) -> Option<std::process::ExitStatus> {
    struct ReapedChild(std::process::Child);
    impl Drop for ReapedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    // The guard also kills/reaps on an assertion or unexpected wait error.
    let mut worker = ReapedChild(child);
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = worker.0.try_wait().unwrap() {
            return Some(status);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum FlacDigest {
    Present,
    Wrong,
    Missing,
}

pub(super) fn verification_surround_flac(
    leading_id3: bool,
    optional_picture: bool,
    digest: FlacDigest,
    declared_frames: u64,
) -> Vec<u8> {
    let mut bytes = prefixed_surround_flac(optional_picture);
    let streaminfo = bytes.windows(4).position(|magic| magic == b"fLaC").unwrap();
    let packed = u64::from_be_bytes(bytes[streaminfo + 18..streaminfo + 26].try_into().unwrap());
    let frame_mask = (1_u64 << 36) - 1;
    assert_eq!(packed & frame_mask, 120, "real fixture source frames");
    assert!(declared_frames <= frame_mask);
    bytes[streaminfo + 18..streaminfo + 26]
        .copy_from_slice(&((packed & !frame_mask) | declared_frames).to_be_bytes());
    assert!(
        bytes[streaminfo + 26..streaminfo + 42]
            .iter()
            .any(|&byte| byte != 0)
    );
    match digest {
        FlacDigest::Present => {}
        FlacDigest::Wrong => bytes[streaminfo + 26] ^= 1,
        FlacDigest::Missing => bytes[streaminfo + 26..streaminfo + 42].fill(0),
    }
    if leading_id3 {
        let mut prefixed = b"ID3\x04\0\0\0\0\0\0".repeat(3);
        prefixed.extend_from_slice(&bytes[streaminfo..]);
        bytes = prefixed;
    }
    bytes
}
