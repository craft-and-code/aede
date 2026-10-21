pub(crate) struct Wave(pub(crate) std::path::PathBuf);
impl Wave {
    pub(crate) fn new(samples: &[f32], channels: u16) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self::with_stamp(samples, channels, unique)
    }
    fn with_stamp(samples: &[f32], channels: u16, unique: u128) -> Self {
        use std::io::Write;
        // Timestamp resolution can coincide across test threads; reserve the path atomically.
        let (path, mut file) = (0..128)
            .find_map(|attempt| {
                let path = std::env::temp_dir().join(format!(
                    "aede-audio-{}-{unique}-{attempt}.wav",
                    std::process::id()
                ));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(file) => Some((path, file)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => panic!("cannot create audio fixture: {error}"),
                }
            })
            .expect("audio fixture namespace exhausted");
        let size = samples.len() as u32 * 4;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(3_u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(48_000_u32.to_le_bytes());
        bytes.extend((48_000_u32 * channels as u32 * 4).to_le_bytes());
        bytes.extend((channels * 4).to_le_bytes());
        bytes.extend(32_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(size.to_le_bytes());
        for sample in samples {
            bytes.extend(sample.to_le_bytes());
        }
        file.write_all(&bytes).unwrap();
        Self(path)
    }
}
impl Drop for Wave {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
#[path = "test_support_tests.rs"]
mod tests;
