//! Local-only external PCM fallback shared by playback and spectral measurement.
use super::Error;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread::JoinHandle;

/// FFmpeg handles Opus, AAC and ALAC when the native decoder cannot open them.
/// Stream data and errors are kept separate, so a failed decode never looks
/// like an ordinary end of file.
pub(crate) struct FfmpegStream {
    child: Child,
    stdout: ChildStdout,
    stderr: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    pending_bytes: Vec<u8>,
    frame_bytes: usize,
    done: bool,
}

impl FfmpegStream {
    pub(super) fn open(path: &Path, sample_rate: u32, channels: u16) -> Result<Self, Error> {
        let ffmpeg = crate::ffmpeg::find()
            .ok_or_else(|| Error::External(crate::ffmpeg::missing("playback")))?;
        Self::open_with_program(path, sample_rate, channels, &ffmpeg)
    }

    fn open_with_program(
        path: &Path,
        sample_rate: u32,
        channels: u16,
        ffmpeg: &str,
    ) -> Result<Self, Error> {
        if sample_rate == 0 || channels == 0 {
            return Err(Error::InvalidFormat);
        }
        let args = vec![
            "-f".into(),
            "f32le".into(),
            "-acodec".into(),
            "pcm_f32le".into(),
            "-ar".into(),
            sample_rate.to_string(),
            "-ac".into(),
            channels.to_string(),
        ];
        Self::spawn(path, ffmpeg, &args, channels)
    }

    /// Decode without rate/channel overrides; a bounded WAV header carries the true output format.
    pub(crate) fn measurement(path: &Path, ffmpeg: &str) -> Result<(Self, u32, u16), Error> {
        let args = [
            "-f",
            "wav",
            "-rf64",
            "never",
            "-acodec",
            "pcm_f32le",
            "-map_metadata",
            "-1",
        ]
        .map(String::from);
        let mut stream = Self::spawn(path, ffmpeg, &args, 1)?;
        let (rate, channels) = aede_audio::pcm_wave::read_format(&mut stream.stdout)
            .map_err(|error| Error::External(error.to_string()))?;
        stream.frame_bytes = usize::from(channels) * 4;
        Ok((stream, rate, channels))
    }
    fn spawn(path: &Path, ffmpeg: &str, args: &[String], channels: u16) -> Result<Self, Error> {
        // FFmpeg parses relative colon-prefixed names as URLs. A canonical
        // local path and restricted protocols keep fallback decoding local.
        let path =
            std::fs::canonicalize(path).map_err(|error| Error::External(error.to_string()))?;
        let metadata =
            std::fs::metadata(&path).map_err(|error| Error::External(error.to_string()))?;
        if !metadata.is_file() {
            return Err(Error::External(
                "playback requires a local regular file".into(),
            ));
        }
        let mut child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file,pipe",
                "-i",
            ])
            .arg(&path)
            .args(["-map", "0:a:0", "-vn", "-sn", "-dn"])
            .args(args)
            .arg("pipe:1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| Error::External(error.to_string()))?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::External("ffmpeg did not open its PCM pipe".into()));
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::External("ffmpeg did not open its error pipe".into()));
        };
        let stderr = match std::thread::Builder::new()
            .name("aede-ffmpeg-errors".into())
            .spawn(move || crate::ffmpeg::diagnostic_tail(stderr))
        {
            Ok(handle) => handle,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::External(error.to_string()));
            }
        };
        Ok(Self {
            child,
            stdout,
            stderr: Some(stderr),
            pending_bytes: Vec::new(),
            frame_bytes: usize::from(channels) * 4,
            done: false,
        })
    }

    pub(crate) fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, Error> {
        let mut buffer = [0; 16_384];
        loop {
            let complete = self.pending_bytes.len() / self.frame_bytes * self.frame_bytes;
            if complete > 0 {
                let bytes = self.pending_bytes.drain(..complete).collect::<Vec<_>>();
                let chunk = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes))
                    .collect();
                return Ok(Some(chunk));
            }
            let count = self
                .stdout
                .read(&mut buffer)
                .map_err(|error| Error::External(error.to_string()))?;
            if count == 0 {
                let status = self
                    .child
                    .wait()
                    .map_err(|error| Error::External(error.to_string()))?;
                self.done = true;
                let tail = match self.stderr.take() {
                    Some(handle) => handle
                        .join()
                        .map_err(|_| Error::External("ffmpeg error reader stopped".into()))?
                        .map_err(|error| Error::External(error.to_string()))?,
                    None => Vec::new(),
                };
                if !status.success() {
                    let message = String::from_utf8_lossy(&tail);
                    return Err(Error::External(format!(
                        "ffmpeg exited with {status}: {}",
                        message.trim()
                    )));
                }
                if !self.pending_bytes.is_empty() {
                    return Err(Error::IncompleteFrame);
                }
                return Ok(None);
            }
            self.pending_bytes.extend_from_slice(&buffer[..count]);
        }
    }
}

impl Drop for FfmpegStream {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(handle) = self.stderr.take() {
            let _ = handle.join();
        }
    }
}
