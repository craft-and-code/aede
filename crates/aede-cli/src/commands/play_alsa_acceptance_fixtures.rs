//! Verified silent sources shared by native lifecycle checks and offline tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::playback::decoder::IntegerFileDecoder;
use aede_core::playback::format::IntegerPcmFormat;

pub(super) struct SilentWav {
    root: PathBuf,
    pub(super) path: PathBuf,
    pub(super) bytes: Vec<u8>,
    pub(super) frames: usize,
}

impl SilentWav {
    pub(super) fn new(format: IntegerPcmFormat) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aede_alsa_acceptance_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).expect("create isolated ALSA acceptance fixture");
        let path = root.join("silence.wav");
        let frames = format.sample_rate() / 5;
        let frame_bytes = u32::from(format.channels()) * (format.bits_per_sample() / 8);
        let data_bytes = frames * frame_bytes;
        let mut bytes = Vec::with_capacity(44 + data_bytes as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&format.channels().to_le_bytes());
        bytes.extend_from_slice(&format.sample_rate().to_le_bytes());
        bytes.extend_from_slice(&(format.sample_rate() * frame_bytes).to_le_bytes());
        bytes.extend_from_slice(&(frame_bytes as u16).to_le_bytes());
        bytes.extend_from_slice(&(format.bits_per_sample() as u16).to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_bytes.to_le_bytes());
        bytes.resize(44 + data_bytes as usize, 0);
        std::fs::write(&path, &bytes).expect("write native silent PCM WAV");
        Self {
            root,
            path,
            bytes,
            frames: frames as usize,
        }
    }

    pub(super) fn verified_samples(&self, format: IntegerPcmFormat) -> Vec<i32> {
        let mut decoder = IntegerFileDecoder::open(&self.path).expect("decode native WAV");
        assert_eq!(decoder.format(), format);
        let mut samples = Vec::new();
        let mut block = vec![0; 1024 * usize::from(format.channels())];
        loop {
            let frames = decoder
                .read_frames(&mut block)
                .expect("decode complete frames");
            if frames == 0 {
                break;
            }
            let sample_count = frames * usize::from(format.channels());
            assert!(sample_count <= block.len());
            assert!(block[..sample_count].iter().all(|&sample| sample == 0));
            samples.extend_from_slice(&block[..sample_count]);
        }
        assert_eq!(samples.len(), self.frames * usize::from(format.channels()));
        samples
    }
}

impl Drop for SilentWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
#[path = "play_alsa_acceptance_fixtures_tests.rs"]
mod tests;
