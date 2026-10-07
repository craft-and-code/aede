//! Device-clocked local output, with ffplay as a portable fallback.

use std::error::Error;
use std::io::{self, Write};
#[cfg(unix)]
use std::process::Command;

use aede_core::playback::exact_output::ExactOutputFormat;
use aede_core::playback::format::{IntegerPcmFormat, PcmStreamFormat};
use aede_core::playback::output::OutputSession;

pub(super) fn list_devices() -> Result<(), Box<dyn Error>> {
    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", target_env = "gnu")
    ))]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let mut found = Vec::new();
        for host_id in cpal::available_hosts() {
            let host = cpal::host_from_id(host_id)?;
            for device in host.output_devices()? {
                let id = device.id()?.to_string();
                let name = device.description()?.name().to_owned();
                found.push((id, name));
            }
        }
        found.sort();
        if found.is_empty() {
            println!("No native output devices found.");
        } else {
            println!("Native output device IDs (listing does not establish strict eligibility):");
            for (id, name) in found {
                println!("{}  {}", crate::ui::literal(&id), crate::ui::literal(&name));
            }
        }
        println!(
            "Bit-perfect currently requires a validated direct ALSA hw: route on Linux GNU; CoreAudio and WASAPI strict routes are refused."
        );
        Ok(())
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", target_env = "gnu")
    )))]
    {
        Err("native output device discovery is unavailable on this build; ordinary playback uses ffplay".into())
    }
}

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
#[path = "play_exact_route.rs"]
mod route;

#[cfg(any(
    all(target_os = "linux", target_env = "gnu"),
    all(test, any(target_os = "macos", target_os = "windows"))
))]
#[path = "play_alsa_output.rs"]
mod alsa;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[path = "play_sample_conversion.rs"]
mod conversion;

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
mod native {
    use std::cell::Cell;
    use std::io::{self, Write};
    use std::sync::Arc;
    use std::time::Duration;

    use cpal::traits::{DeviceTrait, StreamTrait};
    use cpal::{SampleFormat, Stream, StreamConfig};
    use std::sync::atomic::AtomicU64;

    use super::conversion::SampleConversion;
    use super::queue::{PcmProducer, QueueFailure, QueueSnapshot, pcm_queue};
    use super::route::{NativeRequest, resolve_output, strict_plan};
    use super::status::DeviceStatus;
    use super::{ExactOutputFormat, IntegerPcmFormat, PcmStreamFormat};

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

    pub(super) fn select_config_without_effects(
        source: u32,
        ranges: &[(SampleFormat, u32, u32)],
    ) -> Option<(usize, u32)> {
        ranges
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, (format, minimum, maximum))| {
                if minimum > maximum {
                    return None;
                }
                let rate = source.clamp(minimum, maximum);
                format_rank(format).map(|rank| (index, rate, rank))
            })
            .min_by_key(|(_, rate, rank)| (source.abs_diff(*rate), *rank, u32::MAX - *rate))
            .map(|(index, rate, _)| (index, rate))
    }

    fn matching_output(
        input: PcmStreamFormat,
        request: &NativeRequest,
    ) -> Result<
        (
            cpal::Device,
            cpal::SupportedStreamConfig,
            PcmStreamFormat,
            cpal::DeviceId,
        ),
        String,
    > {
        let selected = resolve_output(request)?;
        let id = selected.id;
        let device = selected.device;
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
        let chosen = if request.without_effects {
            select_config_without_effects(input.sample_rate(), &ranges)
        } else {
            select_config(input.sample_rate(), &ranges)
        };
        let (index, rate) = chosen.ok_or_else(|| {
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
            id,
        ))
    }

    pub(super) fn negotiated_format(
        input: PcmStreamFormat,
        request: &NativeRequest,
    ) -> Result<PcmStreamFormat, String> {
        matching_output(input, request).map(|(_, _, format, _)| format)
    }

    enum Producer {
        Float(PcmProducer<f32>),
        Integer(PcmProducer<i32>),
    }

    impl Producer {
        fn snapshot(&self) -> QueueSnapshot {
            match self {
                Self::Float(p) => p.snapshot(),
                Self::Integer(p) => p.snapshot(),
            }
        }
        fn drained(&self) -> bool {
            match self {
                Self::Float(p) => p.drained(),
                Self::Integer(p) => p.drained(),
            }
        }
        fn close_input(&self) {
            match self {
                Self::Float(p) => p.close_input(),
                Self::Integer(p) => p.close_input(),
            }
        }
        fn failure_handle(&self) -> QueueFailure {
            match self {
                Self::Float(p) => p.failure_handle(),
                Self::Integer(p) => p.failure_handle(),
            }
        }
    }

    #[derive(Default)]
    pub(super) struct StartState {
        started: Cell<bool>,
        paused: Cell<bool>,
    }

    impl StartState {
        pub(super) fn ensure(
            &self,
            ready: bool,
            play: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<()> {
            if ready && !self.started.get() && !self.paused.get() {
                play()?;
                self.started.set(true);
            }
            Ok(())
        }
        pub(super) fn pause(&self, pause: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
            if self.started.get() {
                pause()?;
            }
            self.paused.set(true);
            Ok(())
        }
        pub(super) fn resume(&self, play: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
            if self.started.get() {
                play()?;
            }
            self.paused.set(false);
            Ok(())
        }
    }

    /// Device output backed by a preallocated single-producer/single-consumer
    /// PCM ring. Device management and reporting stay on the driver thread.
    pub(in crate::commands::play) struct NativeOutput {
        format: PcmStreamFormat,
        sample_format: SampleFormat,
        stream: Stream,
        producer: Producer,
        status: Arc<DeviceStatus>,
        start: StartState,
        source: Option<IntegerPcmFormat>,
        exact_format: Option<ExactOutputFormat>,
        request: NativeRequest,
        device_id: cpal::DeviceId,
        closed: bool,
        reported: bool,
        quantized_samples: Arc<AtomicU64>,
    }

    impl NativeOutput {
        pub(super) fn open(
            format: PcmStreamFormat,
            request: NativeRequest,
        ) -> Result<Self, String> {
            let (device, supported, format, device_id) = matching_output(format, &request)?;
            let sample_format = supported.sample_format();
            let config: StreamConfig = supported.into();
            let capacity_frames =
                (u64::from(format.sample_rate()) * QUEUE_MILLISECONDS).div_ceil(1000) as usize;
            let (producer, mut pending) =
                pcm_queue(usize::from(format.channels()), capacity_frames, false)
                    .map_err(|error| format!("cannot prepare audio queue: {error}"))?;
            let status = Arc::new(DeviceStatus::default());
            let status_callback = Arc::clone(&status);
            let quantized_samples = Arc::new(AtomicU64::new(0));
            let without_effects = request.without_effects;
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
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(i8, 0, move |sample| quantizer.i8(sample))
                }
                SampleFormat::I16 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(i16, 0, move |sample| quantizer.i16(sample))
                }
                SampleFormat::I24 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(cpal::I24, cpal::I24::from(0_i32), move |sample| {
                        cpal::I24::from(quantizer.i24(sample))
                    })
                }
                SampleFormat::I32 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(i32, 0, move |sample| quantizer.i32(sample))
                }
                SampleFormat::U8 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(u8, 1 << 7, move |sample| quantizer.u8(sample))
                }
                SampleFormat::U16 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(u16, 1 << 15, move |sample| quantizer.u16(sample))
                }
                SampleFormat::U24 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(cpal::U24, cpal::U24::from(8_388_608_i32), move |sample| {
                        cpal::U24::from(quantizer.u24(sample) as i32)
                    })
                }
                SampleFormat::U32 => {
                    let mut quantizer =
                        SampleConversion::new(without_effects, Arc::clone(&quantized_samples));
                    build_stream!(u32, 1 << 31, move |sample| quantizer.u32(sample))
                }
                _ => return Err(format!("unsupported output sample format: {sample_format}")),
            }
            .map_err(|error| format!("cannot open audio device: {error}"))?;
            Ok(Self {
                format,
                sample_format,
                stream,
                producer: Producer::Float(producer),
                status,
                start: StartState::default(),
                source: None,
                exact_format: None,
                request,
                device_id,
                closed: false,
                reported: false,
                quantized_samples,
            })
        }

        pub(super) fn open_exact(
            source: IntegerPcmFormat,
            request: NativeRequest,
        ) -> Result<Self, String> {
            let selected = resolve_output(&request)?;
            let plan = strict_plan(source, &selected)?;
            let sample_format = plan.config.sample_format();
            let config: StreamConfig = plan.config.into();
            let format = PcmStreamFormat::with_layout(source.sample_rate(), source.layout())
                .map_err(|error| error.to_string())?;
            let capacity_frames =
                (u64::from(source.sample_rate()) * QUEUE_MILLISECONDS).div_ceil(1000) as usize;
            let (producer, mut pending) =
                pcm_queue::<i32>(usize::from(source.channels()), capacity_frames, true)
                    .map_err(|error| format!("cannot prepare strict audio queue: {error}"))?;
            let status = Arc::new(DeviceStatus::with_policy(true));
            let status_callback = Arc::clone(&status);
            let status_render = Arc::clone(&status);
            let failure = producer.failure_handle();
            let failure_render = failure.clone();
            let mapping = ExactMapping(source.bits_per_sample());
            macro_rules! build_exact_stream {
                ($sample:ty, $silence:expr, $convert:expr) => {{
                    let mut convert = $convert;
                    selected.device.build_output_stream(
                        config,
                        move |output: &mut [$sample], _| {
                            if status_render.is_failed() {
                                failure_render.fail();
                            }
                            pending.render_mapped(output, $silence, &mut convert);
                        },
                        move |error| {
                            status_callback.record(error.kind());
                            failure.fail();
                        },
                        None,
                    )
                }};
            }
            let stream = match sample_format {
                SampleFormat::I16 => build_exact_stream!(i16, 0, move |s| mapping.i16(s)),
                SampleFormat::I24 => {
                    build_exact_stream!(cpal::I24, cpal::I24::from(0), move |s| mapping.i24(s))
                }
                SampleFormat::I32 => build_exact_stream!(i32, 0, move |s| mapping.i32(s)),
                SampleFormat::F32 => build_exact_stream!(f32, 0.0, move |s| mapping.f32(s)),
                SampleFormat::F64 => build_exact_stream!(f64, 0.0, move |s| mapping.f64(s)),
                _ => {
                    return Err(format!(
                        "unsupported exact native representation: {sample_format}"
                    ));
                }
            }
            .map_err(|error| format!("cannot open strict audio device: {error}"))?;
            Ok(Self {
                format,
                sample_format,
                stream,
                producer: Producer::Integer(producer),
                status,
                start: StartState::default(),
                source: Some(source),
                exact_format: Some(plan.output),
                request,
                device_id: selected.id,
                closed: false,
                reported: false,
                quantized_samples: Arc::new(AtomicU64::new(0)),
            })
        }

        pub(super) fn request(&self) -> &NativeRequest {
            &self.request
        }

        pub(super) fn exact_needs_reopen(&self, source: IntegerPcmFormat) -> Result<bool, String> {
            self.check()?;
            let selected = resolve_output(&self.request)?;
            if selected.id != self.device_id {
                return Err("selected native device ID changed".to_owned());
            }
            let plan = strict_plan(source, &selected)?;
            Ok(
                self.closed
                    || self.source != Some(source)
                    || self.exact_format != Some(plan.output),
            )
        }

        pub(super) fn exact_format(&self) -> Result<ExactOutputFormat, String> {
            self.exact_format
                .ok_or_else(|| "native output is not strict integer PCM".to_owned())
        }

        pub(super) fn format(&self) -> PcmStreamFormat {
            self.format
        }

        pub(super) fn can_reuse(&self, format: PcmStreamFormat) -> bool {
            !self.closed && self.source.is_none() && self.format == format
        }

        pub(super) fn input_closed(&self) -> bool {
            self.closed
        }

        pub(super) fn drain_progress(&self) -> Result<super::DrainProgress, String> {
            self.check()?;
            if self.source.is_some() && self.closed {
                self.ensure_started()?;
            }
            Ok(super::DrainProgress {
                drained: self.producer.drained(),
                consumed_frames: Some(self.producer.snapshot().consumed_frames),
            })
        }

        pub(super) fn consumed_frames(&self) -> u64 {
            self.producer.snapshot().consumed_frames
        }

        pub(super) fn host_tail(&self) -> Duration {
            if self.start.started.get() {
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
            if self.source.is_some() {
                return "native exact PCM; no DSP or dither";
            }
            match self.sample_format {
                SampleFormat::F32 => "native f32",
                SampleFormat::F64 => "native f64",
                _ if self.request.without_effects => {
                    "native integer; exact mapping when representable, otherwise TPDF quantization"
                }
                _ => "native integer with TPDF dither",
            }
        }

        pub(super) fn check(&self) -> Result<(), String> {
            self.status.check()?;
            if self.producer.snapshot().failed {
                return Err("strict audio queue failed; source playback is incomplete".to_owned());
            }
            Ok(())
        }

        pub(super) fn pause(&self) -> Result<(), String> {
            self.check()?;
            self.start
                .pause(|| self.stream.pause().map_err(io::Error::other))
                .map_err(|error| {
                    self.fail_exact();
                    format!("cannot pause audio device: {error}")
                })
        }

        pub(super) fn resume(&self) -> Result<(), String> {
            self.check()?;
            self.start
                .resume(|| self.stream.play().map_err(io::Error::other))
                .map_err(|error| {
                    self.fail_exact();
                    format!("cannot resume audio device: {error}")
                })
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
            if self.reported || !self.start.started.get() {
                return;
            }
            self.reported = true;
            report_queue(&self.producer, &self.status, &self.quantized_samples);
        }

        pub(super) fn abort(self) {
            let Self {
                stream,
                producer,
                status,
                start,
                reported,
                quantized_samples,
                ..
            } = self;
            // Stop the device before terminal I/O: reporting must not prolong
            // delivery of the queued track after a transport change.
            drop(stream);
            if start.started.get() && !reported {
                report_queue(&producer, &status, &quantized_samples);
            }
        }

        fn start_if_needed(&mut self, nonempty: bool) -> io::Result<()> {
            self.check().map_err(io::Error::other)?;
            if self.closed {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "audio output is closed",
                ));
            }
            // Start before publication so a failed stream start cannot hide
            // accepted frames from the caller's progress accounting.
            self.start
                .ensure(nonempty, || self.stream.play().map_err(io::Error::other))?;
            Ok(())
        }

        pub(super) fn write_samples(&mut self, samples: &[f32]) -> io::Result<usize> {
            if self.source.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "strict output refuses floating-point source PCM",
                ));
            }
            self.start_if_needed(!samples.is_empty())?;
            match &mut self.producer {
                Producer::Float(p) => p.write_samples(samples),
                Producer::Integer(_) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "strict output refuses floating-point source PCM",
                )),
            }
        }

        fn fail_exact(&self) {
            if self.source.is_some() {
                self.producer.failure_handle().fail();
            }
        }

        pub(super) fn ensure_started(&self) -> Result<(), String> {
            self.check()?;
            let ready = self.producer.snapshot().queued_frames > 0;
            self.start
                .ensure(ready, || self.stream.play().map_err(io::Error::other))
                .map_err(|error| {
                    self.fail_exact();
                    format!("cannot start audio device: {error}")
                })
        }

        pub(super) fn write_exact(&mut self, samples: &[i32]) -> io::Result<usize> {
            self.check().map_err(io::Error::other)?;
            if self.closed {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "strict audio output is closed",
                ));
            }
            let source = self.source.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "output is not strict integer PCM",
                )
            })?;
            let occupancy = self.producer.snapshot();
            if occupancy.queued_frames == occupancy.capacity_frames {
                self.ensure_started().map_err(io::Error::other)?;
            }
            match &mut self.producer {
                Producer::Integer(p) => p.write_integer(samples, source),
                Producer::Float(_) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "output is not strict integer PCM",
                )),
            }
        }
    }

    #[derive(Clone, Copy)]
    pub(super) struct ExactMapping(pub(super) u32);

    impl ExactMapping {
        pub(super) fn i16(self, sample: i32) -> i16 {
            sample as i16
        }
        pub(super) fn i24(self, sample: i32) -> cpal::I24 {
            cpal::I24::from(sample << (24 - self.0))
        }
        pub(super) fn i32(self, sample: i32) -> i32 {
            sample << (32 - self.0)
        }
        pub(super) fn f32(self, sample: i32) -> f32 {
            sample as f32 / (1_u32 << (self.0 - 1)) as f32
        }
        pub(super) fn f64(self, sample: i32) -> f64 {
            f64::from(sample) / f64::from(1_u32 << (self.0 - 1))
        }
    }

    fn report_queue(producer: &Producer, status: &DeviceStatus, quantized: &AtomicU64) {
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
        let count = quantized.load(std::sync::atomic::Ordering::Relaxed);
        if count > 0 {
            println!("Native conversion: {count} source samples required integer quantization");
        }
        if queue.underrun_frames > 0 {
            eprintln!(
                "Warning: the native PCM queue ran short while playing; output used digital silence"
            );
        }
    }

    impl Write for NativeOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.source.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "strict output refuses untyped PCM bytes",
                ));
            }
            self.start_if_needed(!bytes.is_empty())?;
            match &mut self.producer {
                Producer::Float(p) => p.write_f32le(bytes),
                Producer::Integer(_) => Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "strict output refuses untyped PCM bytes",
                )),
            }
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

#[derive(Clone, Debug, Default)]
pub(super) struct NativeSelection {
    pub(super) device: Option<String>,
    pub(super) strict: bool,
    pub(super) without_effects: bool,
}

impl NativeSelection {
    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", target_env = "gnu")
    ))]
    fn request(&self) -> Result<route::NativeRequest, Box<dyn Error>> {
        Ok(route::NativeRequest {
            device: self.device.as_deref().map(str::parse).transpose()?,
            strict: self.strict,
            without_effects: self.without_effects,
        })
    }
}

pub(super) enum LocalOutput {
    Unopened {
        choice: BackendChoice,
        selection: NativeSelection,
    },
    Ffplay(OutputSession),
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    Alsa {
        output: alsa::AlsaOutput,
        selection: NativeSelection,
    },
    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", target_env = "gnu")
    ))]
    Native {
        output: native::NativeOutput,
        required: bool,
        selection: NativeSelection,
    },
}

impl LocalOutput {
    pub(super) fn requires_exact(&self) -> bool {
        match self {
            Self::Unopened { selection, .. } => selection.strict,
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { selection, .. } => selection.strict,
            Self::Ffplay(_) => false,
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { .. } => true,
        }
    }

    pub(super) fn with_selection(
        choice: BackendChoice,
        selection: NativeSelection,
    ) -> Result<Self, Box<dyn Error>> {
        let named_or_strict = selection.device.is_some() || selection.strict;
        if named_or_strict && matches!(choice, BackendChoice::Ffplay) {
            return Err("named or strict native output cannot use ffplay fallback".into());
        }
        #[cfg(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", target_env = "gnu")
        ))]
        {
            selection.request()?;
        }
        Ok(Self::Unopened {
            choice: if named_or_strict {
                BackendChoice::Native
            } else {
                choice
            },
            selection,
        })
    }

    pub(super) fn write_exact(
        &mut self,
        samples: &[i32],
        canonical32: &[u8],
        byte_offset: usize,
    ) -> io::Result<usize> {
        if canonical32.len() != samples.len().saturating_mul(4)
            || byte_offset > canonical32.len()
            || !byte_offset.is_multiple_of(4)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid exact PCM block or offset",
            ));
        }
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => output
                .write_exact(&samples[byte_offset / 4..])
                .map(|count| count * 4),
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output
                .write_exact(&samples[byte_offset / 4..])
                .map(|count| count * 4),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "exact PCM requires validated native output",
            )),
        }
    }

    pub(super) fn ensure_started(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.ensure_started()?),
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.ensure_started()?),
            _ => Ok(()),
        }
    }

    pub(super) fn exact_needs_reopen(
        &mut self,
        format: IntegerPcmFormat,
    ) -> Result<bool, Box<dyn Error>> {
        if !self.requires_exact() {
            return Err("output policy does not permit exact integer PCM".into());
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", target_env = "gnu")
        )))]
        let _ = format;
        match self {
            Self::Unopened { .. } => Ok(false),
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native { output, .. } => Ok(output.exact_needs_reopen(format)?),
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.exact_needs_reopen(format)?),
            Self::Ffplay(_) => Err("strict output cannot use ffplay".into()),
        }
    }

    pub(super) fn prepare_exact(
        &mut self,
        format: IntegerPcmFormat,
    ) -> Result<ExactOutputFormat, Box<dyn Error>> {
        if !self.requires_exact() {
            return Err("output policy does not permit exact integer PCM".into());
        }
        #[cfg(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", target_env = "gnu")
        ))]
        {
            if let Self::Native { output, .. } = self {
                if !output.exact_needs_reopen(format)? {
                    return Ok(output.exact_format()?);
                }
                if !output.input_closed() {
                    return Err("strict output must be drained before a format change".into());
                }
                output.finish()?;
            }
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            if let Self::Alsa { output, .. } = self {
                if !output.exact_needs_reopen(format)? {
                    return Ok(output.exact_format());
                }
                if !output.input_closed() {
                    return Err("strict ALSA output must be drained before a format change".into());
                }
                output.finish()?;
            }
            let selection = match self {
                Self::Unopened { selection, .. } | Self::Native { selection, .. } => {
                    selection.clone()
                }
                Self::Ffplay(_) => return Err("strict output cannot use ffplay".into()),
                #[cfg(all(target_os = "linux", target_env = "gnu"))]
                Self::Alsa { selection, .. } => selection.clone(),
            };
            // Release the drained stream before a fresh exclusive/direct open.
            // Keep the explicit request even if preparation of the next route fails.
            *self = Self::Unopened {
                choice: BackendChoice::Native,
                selection: selection.clone(),
            };
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            if let Some((host, name)) = selection
                .device
                .as_deref()
                .and_then(|id| id.split_once(':'))
                && host.eq_ignore_ascii_case("alsa")
            {
                let output = alsa::AlsaOutput::open(format, name)?;
                let exact_format = output.exact_format();
                *self = Self::Alsa { output, selection };
                return Ok(exact_format);
            }
            let output = native::NativeOutput::open_exact(format, selection.request()?)?;
            let exact_format = output.exact_format()?;
            *self = Self::Native {
                output,
                required: true,
                selection,
            };
            Ok(exact_format)
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "windows",
            all(target_os = "linux", target_env = "gnu")
        )))]
        {
            let _ = format;
            Err("strict native audio output is unavailable on this build".into())
        }
    }

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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output.stage_description(),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Some(output.consumed_frames()),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output.integer_description(),
            _ => None,
        }
    }

    pub(super) fn new(selection: NativeSelection) -> Result<Self, Box<dyn Error>> {
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
        Self::with_selection(choice, selection)
    }

    pub(super) fn prepare(
        &mut self,
        format: PcmStreamFormat,
    ) -> Result<PcmStreamFormat, Box<dyn Error>> {
        if self.requires_exact() {
            return Err("strict output requires typed integer source preparation".into());
        }
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
                if native::negotiated_format(format, output.request())
                    .is_ok_and(|chosen| output.can_reuse(chosen)) =>
            {
                return Ok(output.format());
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native {
                output,
                required,
                selection,
            } => {
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
                    selection: selection.clone(),
                };
            }
            Self::Unopened { .. } => {}
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { .. } => return Err("strict ALSA output refuses processed PCM".into()),
        }
        let (choice, selection) = match self {
            Self::Unopened { choice, selection } => (*choice, selection.clone()),
            _ => (BackendChoice::Auto, NativeSelection::default()),
        };
        if !matches!(choice, BackendChoice::Ffplay) {
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            match native::NativeOutput::open(format, selection.request()?) {
                Ok(output) => {
                    let selected = output.format();
                    *self = Self::Native {
                        output,
                        required: matches!(choice, BackendChoice::Native),
                        selection,
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
            Self::Native { output, .. } => Ok(native::negotiated_format(format, output.request())
                .map_or(true, |chosen| !output.can_reuse(chosen))),
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { .. } => Err("strict ALSA output refuses processed PCM".into()),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.drain_progress()?),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output.host_tail(),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output.close_input(),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => {
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.finish()?),
        }
    }

    pub(super) fn abort(&mut self) -> Result<(), Box<dyn Error>> {
        let previous = std::mem::replace(
            self,
            Self::Unopened {
                choice: BackendChoice::Auto,
                selection: NativeSelection::default(),
            },
        );
        match previous {
            Self::Ffplay(mut output) => {
                output.abort()?;
                *self = Self::Ffplay(OutputSession::new());
            }
            Self::Unopened { choice, selection } => *self = Self::Unopened { choice, selection },
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, selection } => {
                let result = output.abort();
                *self = Self::Unopened {
                    choice: BackendChoice::Native,
                    selection,
                };
                result?;
            }
            #[cfg(any(
                target_os = "macos",
                target_os = "windows",
                all(target_os = "linux", target_env = "gnu")
            ))]
            Self::Native {
                required,
                output,
                selection,
            } => {
                output.abort();
                *self = Self::Unopened {
                    choice: if required {
                        BackendChoice::Native
                    } else {
                        BackendChoice::Auto
                    },
                    selection,
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.pause()?),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => Ok(output.resume()?),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { .. } => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "strict ALSA output refuses untyped PCM",
            )),
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
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Alsa { output, .. } => output.check().map_err(io::Error::other),
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
