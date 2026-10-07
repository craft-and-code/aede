//! Device-clocked local output, with ffplay as a portable fallback.

use std::error::Error;
use std::io::{self, Write};
#[cfg(unix)]
use std::process::Command;

use aede_core::playback::format::PcmStreamFormat;
use aede_core::playback::output::OutputSession;

#[derive(Clone, Copy)]
pub(super) struct DrainProgress {
    pub(super) drained: bool,
    pub(super) consumed_frames: Option<u64>,
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[path = "play_audio_queue.rs"]
mod queue;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[path = "play_device_status.rs"]
mod status;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
mod native {
    use std::io::{self, Write};
    use std::sync::Arc;
    use std::time::Duration;

    use aede_dsp::TpdfQuantizer;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, Stream, StreamConfig};

    use super::PcmStreamFormat;
    use super::queue::{PcmProducer, pcm_queue};
    use super::status::DeviceStatus;

    // Bound decode lead by time rather than decoder-dependent block sizes.
    const QUEUE_MILLISECONDS: u64 = 500;

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

    /// Device output backed by a preallocated single-producer/single-consumer
    /// PCM ring. Device management and reporting stay on the driver thread.
    pub(in crate::commands::play) struct NativeOutput {
        format: PcmStreamFormat,
        sample_format: SampleFormat,
        stream: Stream,
        producer: PcmProducer,
        status: Arc<DeviceStatus>,
        started: bool,
        closed: bool,
        reported: bool,
    }

    impl NativeOutput {
        pub(super) fn open(format: PcmStreamFormat) -> Result<Self, String> {
            let (device, supported, format) = matching_output(format)?;
            let sample_format = supported.sample_format();
            let config: StreamConfig = supported.into();
            let capacity_frames =
                (u64::from(format.sample_rate()) * QUEUE_MILLISECONDS).div_ceil(1000) as usize;
            let (producer, mut pending) =
                pcm_queue(usize::from(format.channels()), capacity_frames, false)
                    .map_err(|error| format!("cannot prepare audio queue: {error}"))?;
            let status = Arc::new(DeviceStatus::default());
            let status_callback = Arc::clone(&status);
            macro_rules! build_stream {
                ($sample:ty, $silence:expr, $convert:expr) => {{
                    let mut convert = $convert;
                    device.build_output_stream(
                        config,
                        move |output: &mut [$sample], _| {
                            pending.render_mapped(output, $silence, &mut convert);
                        },
                        move |error| {
                            status_callback.record(error.kind());
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
                stream,
                producer,
                status,
                started: false,
                closed: false,
                reported: false,
            })
        }

        pub(super) fn format(&self) -> PcmStreamFormat {
            self.format
        }

        pub(super) fn can_reuse(&self, format: PcmStreamFormat) -> bool {
            !self.closed && self.format == format
        }

        pub(super) fn input_closed(&self) -> bool {
            self.closed
        }

        pub(super) fn drain_progress(&self) -> Result<super::DrainProgress, String> {
            self.status.check()?;
            Ok(super::DrainProgress {
                drained: self.producer.drained(),
                consumed_frames: Some(self.producer.snapshot().consumed_frames),
            })
        }

        pub(super) fn consumed_frames(&self) -> u64 {
            self.producer.snapshot().consumed_frames
        }

        pub(super) fn host_tail(&self) -> Duration {
            if self.started {
                Duration::from_millis(100)
            } else {
                Duration::ZERO
            }
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

        pub(super) fn stage_description(&self) -> &'static str {
            match self.sample_format {
                SampleFormat::F32 => "native f32",
                SampleFormat::F64 => "native f64",
                _ => "native integer with TPDF dither",
            }
        }

        pub(super) fn check(&self) -> Result<(), String> {
            self.status.check()
        }

        pub(super) fn pause(&self) -> Result<(), String> {
            self.status.check()?;
            self.stream
                .pause()
                .map_err(|error| format!("cannot pause audio device: {error}"))
        }

        pub(super) fn resume(&self) -> Result<(), String> {
            self.status.check()?;
            self.stream
                .play()
                .map_err(|error| format!("cannot resume audio device: {error}"))
        }

        pub(super) fn close_input(&mut self) {
            self.closed = true;
            self.producer.close_input();
        }

        pub(super) fn finish(&mut self) -> Result<(), String> {
            self.close_input();
            if !self.drain_progress()?.drained {
                return Err("native output must be drained before finalization".into());
            }
            // The driver has also kept the host-buffer allowance alive while
            // polling controls. Finalization must not introduce another wait.
            self.report_diagnostics();
            Ok(())
        }

        fn report_diagnostics(&mut self) {
            if self.reported || !self.started {
                return;
            }
            self.reported = true;
            report_queue(&self.producer, &self.status);
        }

        pub(super) fn abort(self) {
            let Self {
                stream,
                producer,
                status,
                started,
                reported,
                ..
            } = self;
            // Stop the device before terminal I/O: reporting must not prolong
            // delivery of the queued track after a transport change.
            drop(stream);
            if started && !reported {
                report_queue(&producer, &status);
            }
        }

        fn start_if_needed(&mut self, nonempty: bool) -> io::Result<()> {
            self.status.check().map_err(io::Error::other)?;
            if self.closed {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "audio output is closed",
                ));
            }
            // Start before publication so a failed stream start cannot hide
            // accepted frames from the caller's progress accounting.
            if !self.started && nonempty {
                self.stream.play().map_err(io::Error::other)?;
                self.started = true;
            }
            Ok(())
        }

        pub(super) fn write_samples(&mut self, samples: &[f32]) -> io::Result<usize> {
            self.start_if_needed(!samples.is_empty())?;
            self.producer.write_samples(samples)
        }
    }

    fn report_queue(producer: &PcmProducer, status: &DeviceStatus) {
        let queue = producer.snapshot();
        println!(
            "Native output: {} consumed frames; queue {}/{} frames; {} missing frames in {} callbacks; {} host xruns",
            queue.consumed_frames,
            queue.queued_frames,
            queue.capacity_frames,
            queue.underrun_frames,
            queue.underrun_callbacks,
            status.snapshot_xruns(),
        );
        if queue.underrun_frames > 0 {
            eprintln!(
                "Warning: the native PCM queue ran short while playing; output used digital silence"
            );
        }
    }

    impl Write for NativeOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.start_if_needed(!bytes.is_empty())?;
            self.producer.write_f32le(bytes)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
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
    pub(super) fn write_pcm(
        &mut self,
        samples: &[f32],
        f32le: &[u8],
        byte_offset: usize,
    ) -> io::Result<usize> {
        if byte_offset > f32le.len() || byte_offset > samples.len().saturating_mul(4) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PCM offset exceeds block",
            ));
        }
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => {
                if !byte_offset.is_multiple_of(4) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "native PCM offset is not sample aligned",
                    ));
                }
                let remaining = samples.get(byte_offset / 4..).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "native PCM offset exceeds block",
                    )
                })?;
                output.write_samples(remaining).map(|count| count * 4)
            }
            _ => self.write(&f32le[byte_offset..]),
        }
    }

    pub(super) fn stage_description(&self) -> &'static str {
        match self {
            Self::Ffplay(_) => "ffplay f32le",
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.stage_description(),
            Self::Unopened { .. } => "unopened",
        }
    }

    pub(super) fn consumed_frames(&self) -> Option<u64> {
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Some(output.consumed_frames()),
            _ => None,
        }
    }

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
                if output.format().is_some_and(|held| held != format) && !output.poll_finished()? {
                    return Err("ffplay output must be drained before a format change".into());
                }
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
                    .is_ok_and(|chosen| output.can_reuse(chosen)) =>
            {
                return Ok(output.format());
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, required } => {
                if !output.input_closed() {
                    return Err("native output must be drained before a format change".into());
                }
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

    pub(super) fn needs_reopen(&mut self, format: PcmStreamFormat) -> Result<bool, Box<dyn Error>> {
        match self {
            Self::Unopened { .. } => Ok(false),
            Self::Ffplay(output) => {
                output.poll_finished()?;
                Ok(output.format().is_some_and(|held| held != format))
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => {
                Ok(native::negotiated_format(format)
                    .map_or(true, |chosen| !output.can_reuse(chosen)))
            }
        }
    }

    pub(super) fn drain_progress(&mut self) -> Result<DrainProgress, Box<dyn Error>> {
        match self {
            Self::Unopened { .. } => Ok(DrainProgress {
                drained: true,
                consumed_frames: None,
            }),
            Self::Ffplay(output) => Ok(DrainProgress {
                drained: output.poll_finished()?,
                consumed_frames: None,
            }),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.drain_progress()?),
        }
    }

    pub(super) fn host_tail(&self) -> std::time::Duration {
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output.host_tail(),
            _ => std::time::Duration::ZERO,
        }
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

    pub(super) fn stopped_early(&mut self) -> Result<bool, Box<dyn Error>> {
        match self {
            Self::Ffplay(output) => Ok(output.poll_finished()?),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => {
                output.check()?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Finalize an output already drained by the control-aware driver. Native
    /// callers must also have served `host_tail`; this method never waits.
    pub(super) fn finish(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Unopened { .. } => Ok(()),
            Self::Ffplay(output) => {
                if !output.poll_finished()? {
                    return Err("ffplay output must be drained before finalization".into());
                }
                Ok(output.finish()?)
            }
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
            Self::Native { required, output } => {
                output.abort();
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
