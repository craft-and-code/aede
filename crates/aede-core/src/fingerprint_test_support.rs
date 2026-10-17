use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct AudioFolder(PathBuf);

impl AudioFolder {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede-fingerprint-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("an exclusive test folder");
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for AudioFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl AudioFolder {
    pub(super) fn tone(&self, name: &str, seconds: u32, changed_tail: bool) -> PathBuf {
        use std::io::Write;
        const RATE: u32 = 11_025;
        let path = self.path().join(name);
        let mut output = std::io::BufWriter::new(std::fs::File::create(&path).expect("a WAV"));
        let bytes = seconds * RATE * 2;
        output.write_all(b"RIFF").expect("header");
        output.write_all(&(bytes + 36).to_le_bytes()).expect("size");
        output.write_all(b"WAVEfmt ").expect("format");
        output.write_all(&16u32.to_le_bytes()).expect("PCM header");
        output.write_all(&1u16.to_le_bytes()).expect("PCM");
        output.write_all(&1u16.to_le_bytes()).expect("mono");
        output.write_all(&RATE.to_le_bytes()).expect("rate");
        output
            .write_all(&(RATE * 2).to_le_bytes())
            .expect("byte rate");
        output.write_all(&2u16.to_le_bytes()).expect("block size");
        output
            .write_all(&16u16.to_le_bytes())
            .expect("sample width");
        output.write_all(b"data").expect("data");
        output.write_all(&bytes.to_le_bytes()).expect("data size");
        for sample in 0..seconds * RATE {
            let second = sample / RATE;
            let (frequency, harmonic) = if changed_tail && second >= 120 {
                (
                    [205.0, 370.0, 490.0, 710.0, 965.0][((second - 120) / 3 % 5) as usize],
                    1.3,
                )
            } else {
                (
                    [220.0, 440.0, 330.0, 880.0, 550.0][(second / 7 % 5) as usize],
                    1.505,
                )
            };
            let phase = std::f64::consts::TAU * frequency * f64::from(sample) / f64::from(RATE);
            let value = (14_000.0 * phase.sin() + 4_000.0 * (phase * harmonic).sin()) as i16;
            output.write_all(&value.to_le_bytes()).expect("sample");
        }
        output.flush().expect("the complete WAV");
        path
    }
}
