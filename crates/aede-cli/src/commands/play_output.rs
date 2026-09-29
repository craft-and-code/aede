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

    use aede_dsp::TpdfQuantizer;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream, StreamConfig};

    use super::PcmStreamFormat;

    const QUEUED_BLOCKS: usize = 64;

    fn format_rank(format: SampleFormat) -> Option<u8> {
        match format {
            SampleFormat::F32 => Some(0),
            SampleFormat::F64 => Some(1),
            SampleFormat::I32 => Some(2),
            SampleFormat::I24 => Some(3),
            SampleFormat::I16 => Some(4),
            SampleFormat::U32 => Some(5),
            SampleFormat::U24 => Some(6),
            SampleFormat::U16 => Some(7),
            SampleFormat::I8 => Some(8),
            SampleFormat::U8 => Some(9),
            _ => None,
        }
    }

    pub(super) fn select_config(
        source: u32,
        ranges: &[(SampleFormat, u32, u32)],
    ) -> Option<(usize, u32)> {
        ranges
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, (format, minimum, maximum))| {
                (minimum <= maximum)
                    .then(|| {
                        format_rank(format)
                            .map(|rank| (index, source.clamp(minimum, maximum), rank))
                    })
                    .flatten()
            })
            .min_by_key(|(_, rate, rank)| (*rank, source.abs_diff(*rate), u32::MAX - *rate))
            .map(|(index, rate, _)| (index, rate))
    }

    fn matching_output(
        input: PcmStreamFormat,
    ) -> Result<(cpal::Device, cpal::SupportedStreamConfig, PcmStreamFormat), String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("no default audio output device")?;
        let mut configs = device
            .supported_output_configs()
            .map_err(|error| format!("cannot query audio device: {error}"))?
            .filter(|config| config.channels() == input.channels())
            .collect::<Vec<_>>();
        let ranges = configs
            .iter()
            .map(|config| {
                (
                    config.sample_format(),
                    config.min_sample_rate(),
                    config.max_sample_rate(),
                )
            })
            .collect::<Vec<_>>();
        let (index, rate) = select_config(input.sample_rate(), &ranges).ok_or_else(|| {
            format!(
                "device has no supported {}-channel PCM output",
                input.channels()
            )
        })?;
        let format = PcmStreamFormat::with_layout(rate, input.layout())
            .map_err(|error| error.to_string())?;
        Ok((
            device,
            configs.swap_remove(index).with_sample_rate(rate),
            format,
        ))
    }

    pub(super) fn negotiated_format(input: PcmStreamFormat) -> Result<PcmStreamFormat, String> {
        matching_output(input).map(|(_, _, format)| format)
    }

    pub(super) fn decode_f32le(bytes: &[u8]) -> io::Result<Vec<f32>> {
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
        if samples
            .iter()
            .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "audio output requires finite PCM within full scale",
            ));
        }
        Ok(samples)
    }

    /// The callback takes complete sample blocks without allocating or locking.
    /// A bounded channel keeps decoding ahead of the device without unbounded
    /// memory growth. The producer never blocks inside Write, so pause and
    /// transport keys remain available while the queue is full.
    pub(in crate::commands::play) struct NativeOutput {
        format: PcmStreamFormat,
        sample_format: SampleFormat,
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
            let (device, supported, format) = matching_output(format)?;
            let sample_format = supported.sample_format();
            let config: StreamConfig = supported.into();
            let (sender, receiver) = mpsc::sync_channel(QUEUED_BLOCKS);
            let submitted = Arc::new(AtomicU64::new(0));
            let consumed = Arc::new(AtomicU64::new(0));
            let failed = Arc::new(AtomicBool::new(false));
            let consumed_callback = Arc::clone(&consumed);
            let failed_callback = Arc::clone(&failed);
            let mut pending = PendingSamples::new(receiver);
            macro_rules! build_stream {
                ($sample:ty, $silence:expr, $convert:expr) => {{
                    let mut convert = $convert;
                    device.build_output_stream(
                        config,
                        move |output: &mut [$sample], _| {
                            pending.render_mapped(
                                output,
                                &consumed_callback,
                                $silence,
                                &mut convert,
                            );
                        },
                        move |error| {
                            failed_callback.store(true, Ordering::Release);
                            eprintln!("audio device error: {error}");
                        },
                        None,
                    )
                }};
            }
            let stream = match sample_format {
                SampleFormat::F32 => build_stream!(f32, 0.0, |sample| sample),
                SampleFormat::F64 => build_stream!(f64, 0.0, |sample| f64::from(sample)),
                SampleFormat::I8 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(i8, 0, move |sample| quantizer.i8(sample))
                }
                SampleFormat::I16 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(i16, 0, move |sample| quantizer.i16(sample))
                }
                SampleFormat::I24 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(cpal::I24, cpal::I24::from(0_i32), move |sample| {
                        cpal::I24::from(quantizer.i24(sample))
                    })
                }
                SampleFormat::I32 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(i32, 0, move |sample| quantizer.i32(sample))
                }
                SampleFormat::U8 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(u8, 1 << 7, move |sample| quantizer.u8(sample))
                }
                SampleFormat::U16 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(u16, 1 << 15, move |sample| quantizer.u16(sample))
                }
                SampleFormat::U24 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(cpal::U24, cpal::U24::from(8_388_608_i32), move |sample| {
                        cpal::U24::from(quantizer.u24(sample) as i32)
                    })
                }
                SampleFormat::U32 => {
                    let mut quantizer = TpdfQuantizer::new();
                    build_stream!(u32, 1 << 31, move |sample| quantizer.u32(sample))
                }
                _ => return Err(format!("unsupported output sample format: {sample_format}")),
            }
            .map_err(|error| format!("cannot open audio device: {error}"))?;
            Ok(Self {
                format,
                sample_format,
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

        pub(super) fn integer_description(&self) -> Option<&'static str> {
            match self.sample_format {
                SampleFormat::I8 => Some("signed 8-bit PCM"),
                SampleFormat::I16 => Some("signed 16-bit PCM"),
                SampleFormat::I24 => Some("signed 24-bit PCM"),
                SampleFormat::I32 => Some("signed 32-bit PCM"),
                SampleFormat::U8 => Some("unsigned 8-bit PCM"),
                SampleFormat::U16 => Some("unsigned 16-bit PCM"),
                SampleFormat::U24 => Some("unsigned 24-bit PCM"),
                SampleFormat::U32 => Some("unsigned 32-bit PCM"),
                _ => None,
            }
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
            let samples = decode_f32le(bytes)?;
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

        pub(super) fn render_mapped<T: Copy>(
            &mut self,
            output: &mut [T],
            consumed: &AtomicU64,
            silence: T,
            mut convert: impl FnMut(f32) -> T,
        ) {
            let mut played = 0;
            for sample in output {
                if self.offset == self.block.len() {
                    match self.receiver.try_recv() {
                        Ok(block) => {
                            self.block = block;
                            self.offset = 0;
                        }
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                            *sample = silence;
                            continue;
                        }
                    }
                }
                if let Some(value) = self.block.get(self.offset) {
                    *sample = convert(*value);
                    self.offset += 1;
                    played += 1;
                } else {
                    *sample = silence;
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
    pub(super) fn integer_description(&self) -> Option<&'static str> {
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.integer_description(),
            _ => None,
        }
    }

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

    pub(super) fn prepare(
        &mut self,
        format: PcmStreamFormat,
    ) -> Result<PcmStreamFormat, Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => {
                output.prepare(format)?;
                return Ok(format);
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. }
                if native::negotiated_format(format)
                    .is_ok_and(|chosen| chosen == output.format()) =>
            {
                return Ok(output.format());
            }
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
                    let selected = output.format();
                    *self = Self::Native {
                        output,
                        required: matches!(choice, BackendChoice::Native),
                    };
                    return Ok(selected);
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
        Ok(format)
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
