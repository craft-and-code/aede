//! Device-clocked local output, with ffplay as a portable fallback.

use std::error::Error;
use std::io::{self, Write};
#[cfg(unix)]
use std::process::Command;

use aede_core::playback::format::PcmStreamFormat;
use aede_core::playback::output::OutputSession;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
mod native {
    use std::io::{self, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
    use std::time::Duration;

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream, StreamConfig};

    use super::PcmStreamFormat;

    const QUEUED_BLOCKS: usize = 64;

    /// The callback takes complete sample blocks without allocating or locking.
    /// A bounded channel keeps decoding ahead of the device without unbounded
    /// memory growth. The producer never blocks inside Write, so pause and
    /// transport keys remain available while the queue is full.
    pub(in crate::commands::play) struct NativeOutput {
        format: PcmStreamFormat,
        sender: SyncSender<Vec<f32>>,
        stream: Stream,
        submitted: Arc<AtomicU64>,
        consumed: Arc<AtomicU64>,
        failed: Arc<AtomicBool>,
        started: bool,
        closed: bool,
    }

    impl NativeOutput {
        pub(super) fn open(format: PcmStreamFormat) -> Result<Self, String> {
            let device = cpal::default_host()
                .default_output_device()
                .ok_or("no default audio output device")?;
            let supported = device
                .supported_output_configs()
                .map_err(|error| format!("cannot query audio device: {error}"))?
                .find(|config| {
                    config.sample_format() == SampleFormat::F32
                        && config.channels() == format.channels()
                        && config.min_sample_rate() <= format.sample_rate()
                        && config.max_sample_rate() >= format.sample_rate()
                })
                .ok_or_else(|| {
                    format!(
                        "device has no floating-point {} Hz/{} channel output",
                        format.sample_rate(),
                        format.channels()
                    )
                })?;
            let config: StreamConfig = supported.with_sample_rate(format.sample_rate()).into();
            let (sender, receiver) = mpsc::sync_channel(QUEUED_BLOCKS);
            let submitted = Arc::new(AtomicU64::new(0));
            let consumed = Arc::new(AtomicU64::new(0));
            let failed = Arc::new(AtomicBool::new(false));
            let consumed_callback = Arc::clone(&consumed);
            let failed_callback = Arc::clone(&failed);
            let mut pending = PendingSamples::new(receiver);
            let stream = device
                .build_output_stream(
                    config,
                    move |output: &mut [f32], _| {
                        pending.render(output, &consumed_callback);
                    },
                    move |error| {
                        failed_callback.store(true, Ordering::Release);
                        eprintln!("audio device error: {error}");
                    },
                    None,
                )
                .map_err(|error| format!("cannot open audio device: {error}"))?;
            Ok(Self {
                format,
                sender,
                stream,
                submitted,
                consumed,
                failed,
                started: false,
                closed: false,
            })
        }

        pub(super) fn format(&self) -> PcmStreamFormat {
            self.format
        }

        pub(super) fn drained(&self) -> Result<bool, String> {
            if self.failed.load(Ordering::Acquire) {
                return Err("audio device stopped while playing".into());
            }
            Ok(self.submitted.load(Ordering::Acquire) == self.consumed.load(Ordering::Acquire))
        }

        pub(super) fn pause(&self) -> Result<(), String> {
            self.stream
                .pause()
                .map_err(|error| format!("cannot pause audio device: {error}"))
        }

        pub(super) fn resume(&self) -> Result<(), String> {
            self.stream
                .play()
                .map_err(|error| format!("cannot resume audio device: {error}"))
        }

        pub(super) fn close_input(&mut self) {
            self.closed = true;
        }

        pub(super) fn finish(&mut self) -> Result<(), String> {
            self.closed = true;
            while !self.drained()? {
                std::thread::sleep(Duration::from_millis(5));
            }
            // CPAL has handed these frames to the host, which may still have
            // one hardware buffer in flight. Keep the stream alive for it.
            if self.started {
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        }
    }

    impl Write for NativeOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.closed || self.failed.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "audio output is closed",
                ));
            }
            if !bytes.len().is_multiple_of(4) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unaligned PCM samples",
                ));
            }
            let samples = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|sample| f32::from_le_bytes(*sample))
                .collect::<Vec<_>>();
            match self.sender.try_send(samples) {
                Ok(()) => {
                    self.submitted
                        .fetch_add((bytes.len() / 4) as u64, Ordering::Release);
                    if !self.started {
                        self.stream.play().map_err(io::Error::other)?;
                        self.started = true;
                    }
                    Ok(bytes.len())
                }
                Err(TrySendError::Full(_)) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                Err(TrySendError::Disconnected(_)) => {
                    Err(io::Error::from(io::ErrorKind::BrokenPipe))
                }
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    pub(super) struct PendingSamples {
        receiver: Receiver<Vec<f32>>,
        block: Vec<f32>,
        offset: usize,
    }

    impl PendingSamples {
        pub(super) fn new(receiver: Receiver<Vec<f32>>) -> Self {
            Self {
                receiver,
                block: Vec::new(),
                offset: 0,
            }
        }

        pub(super) fn render(&mut self, output: &mut [f32], consumed: &AtomicU64) {
            let mut played = 0;
            for sample in output {
                if self.offset == self.block.len() {
                    match self.receiver.try_recv() {
                        Ok(block) => {
                            self.block = block;
                            self.offset = 0;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                            *sample = 0.0;
                            continue;
                        }
                    }
                }
                if let Some(value) = self.block.get(self.offset) {
                    *sample = *value;
                    self.offset += 1;
                    played += 1;
                } else {
                    *sample = 0.0;
                }
            }
            consumed.fetch_add(played, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum BackendChoice {
    Auto,
    Native,
    Ffplay,
}

pub(super) enum LocalOutput {
    Unopened {
        choice: BackendChoice,
    },
    Ffplay(OutputSession),
    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", target_env = "gnu")
    ))]
    Native {
        output: native::NativeOutput,
        required: bool,
    },
}

impl LocalOutput {
    pub(super) fn new() -> Result<Self, Box<dyn Error>> {
        let choice = match std::env::var("AEDE_AUDIO_BACKEND") {
            Ok(value) if value == "ffplay" => BackendChoice::Ffplay,
            Ok(value) if value == "native" => BackendChoice::Native,
            Ok(value) => {
                return Err(
                    format!("AEDE_AUDIO_BACKEND must be native or ffplay, got {value:?}").into(),
                );
            }
            Err(std::env::VarError::NotPresent) => BackendChoice::Auto,
            Err(error) => return Err(error.into()),
        };
        Ok(Self::Unopened { choice })
    }

    pub(super) fn prepare(&mut self, format: PcmStreamFormat) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => {
                output.prepare(format)?;
                return Ok(());
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } if output.format() == format => return Ok(()),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, required } => {
                output.finish()?;
                *self = Self::Unopened {
                    choice: if *required {
                        BackendChoice::Native
                    } else {
                        BackendChoice::Auto
                    },
                };
            }
            Self::Unopened { .. } => {}
        }
        let choice = match self {
            Self::Unopened { choice } => *choice,
            _ => BackendChoice::Auto,
        };
        if !matches!(choice, BackendChoice::Ffplay) {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            match native::NativeOutput::open(format) {
                Ok(output) => {
                    *self = Self::Native {
                        output,
                        required: matches!(choice, BackendChoice::Native),
                    };
                    return Ok(());
                }
                Err(error) if matches!(choice, BackendChoice::Native) => return Err(error.into()),
                Err(error) => eprintln!("native audio unavailable ({error}); trying ffplay"),
            }
            #[cfg(not(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            )))]
            if matches!(choice, BackendChoice::Native) {
                return Err("native audio output is unavailable on this build".into());
            }
        }
        let mut output = OutputSession::new();
        output.prepare(format)?;
        *self = Self::Ffplay(output);
        Ok(())
    }

    pub(super) fn close_input(&mut self) {
        match self {
            Self::Unopened { .. } => {}
            Self::Ffplay(output) => output.close_input(),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.close_input(),
        }
    }

    pub(super) fn drained(&mut self) -> Result<bool, Box<dyn Error>> {
        match self {
            Self::Unopened { .. } => Ok(true),
            Self::Ffplay(output) => Ok(output.poll_finished()?),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.drained()?),
        }
    }

    pub(super) fn stopped_early(&mut self) -> Result<bool, Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => Ok(output.poll_finished()?),
            _ => Ok(false),
        }
    }

    pub(super) fn finish(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Unopened { .. } => Ok(()),
            Self::Ffplay(output) => Ok(output.finish()?),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.finish()?),
        }
    }

    pub(super) fn abort(&mut self) -> Result<(), Box<dyn Error>> {
        let previous = std::mem::replace(
            self,
            Self::Unopened {
                choice: BackendChoice::Auto,
            },
        );
        match previous {
            Self::Ffplay(mut output) => {
                output.abort()?;
                *self = Self::Ffplay(OutputSession::new());
            }
            Self::Unopened { choice } => *self = Self::Unopened { choice },
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { required, .. } => {
                *self = Self::Unopened {
                    choice: if required {
                        BackendChoice::Native
                    } else {
                        BackendChoice::Auto
                    },
                };
            }
        }
        Ok(())
    }

    pub(super) fn pause(&self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => signal_player(output.process_id(), "STOP"),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.pause()?),
            Self::Unopened { .. } => Ok(()),
        }
    }

    pub(super) fn resume(&self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => signal_player(output.process_id(), "CONT"),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.resume()?),
            Self::Unopened { .. } => Ok(()),
        }
    }
}

impl Write for LocalOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Unopened { .. } => Err(io::Error::from(io::ErrorKind::BrokenPipe)),
            Self::Ffplay(output) => output.write(bytes),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Unopened { .. } => Ok(()),
            Self::Ffplay(output) => output.flush(),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.flush(),
        }
    }
}

#[cfg(unix)]
fn signal_player(process_id: Option<u32>, signal: &str) -> Result<(), Box<dyn Error>> {
    let process_id = process_id.ok_or("audio output has no process")?;
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(process_id.to_string())
        .status()?;
    if !status.success() {
        return Err(format!("cannot {signal} ffplay process: kill exited with {status}").into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn signal_player(_process_id: Option<u32>, _signal: &str) -> Result<(), Box<dyn Error>> {
    Err("ffplay terminal controls are unavailable on this platform".into())
}

#[cfg(test)]
#[path = "play_output_tests.rs"]
mod tests;
