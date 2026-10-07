//! Direct ALSA integer output with an owned, nonblocking hardware PCM handle.
//!
//! The software ring and the hardware buffer have separate clocks/counters.
//! The worker never substitutes silence, recovers an xrun, or changes controls.
//! Source admission and allocation finish before that worker is launched.

use std::ffi::CString;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::fmt::Write as _;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::thread;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::thread::JoinHandle;
use std::time::Duration;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::time::Instant;

use aede_core::playback::exact_output::{
    ExactOutputFormat, ExactPcmAdapter, ExactSampleRepresentation,
};
use aede_core::playback::format::IntegerPcmFormat;
use aede_dsp::ChannelLayout;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use alsa::ctl::{ElemIface, ElemType, ElemValue};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use alsa::hctl::HCtl;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use alsa::pcm::{Access, Format, Frames, HwParams, PCM, State};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use alsa::{Ctl, Direction, ValueOr};

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::DrainProgress;
use super::queue::{PcmConsumer, QueueFailure};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::queue::{PcmProducer, pcm_queue};

const MAX_FRAMES: usize = 4096;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const MAX_QUEUE_FRAMES: usize = 262_144;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const CONTROL_TIMEOUT: Duration = Duration::from_millis(500);
const WORKER_WAIT: Duration = Duration::from_millis(2);
const RUNNING: u8 = 0;
const PAUSED: u8 = 1;
const STOPPED: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Encoding {
    Signed16,
    Signed24In32,
    Packed24,
    Signed32,
}

impl Encoding {
    fn width(self) -> usize {
        if self == Self::Signed16 {
            2
        } else if self == Self::Packed24 {
            3
        } else {
            4
        }
    }
    fn bits(self) -> u32 {
        match self {
            Self::Signed16 => 16,
            Self::Signed24In32 | Self::Packed24 => 24,
            Self::Signed32 => 32,
        }
    }
    fn representation(self) -> ExactSampleRepresentation {
        match self {
            Self::Signed16 => ExactSampleRepresentation::Signed16Le,
            Self::Signed24In32 | Self::Packed24 => ExactSampleRepresentation::PackedSigned24Le,
            Self::Signed32 => ExactSampleRepresentation::Signed32Le,
        }
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn alsa(self) -> Format {
        match self {
            Self::Signed16 => Format::S16LE,
            Self::Signed24In32 => Format::S24LE,
            Self::Packed24 => Format::S243LE,
            Self::Signed32 => Format::S32LE,
        }
    }
}

#[derive(Clone, Copy)]
struct Readback {
    hardware: bool,
    rate: (u32, u32),
    channels: u32,
    layout: Option<ChannelLayout>,
    effective_bits: u32,
    resampling: bool,
    interleaved: bool,
    encoding: Encoding,
}

fn admit(source: IntegerPcmFormat, actual: Readback) -> Result<ExactOutputFormat, String> {
    if !actual.hardware {
        return Err("ALSA PCM is a plugin rather than direct hardware".to_owned());
    }
    if actual.rate.1 == 0
        || u64::from(actual.rate.0) != u64::from(source.sample_rate()) * u64::from(actual.rate.1)
    {
        return Err("ALSA hardware did not retain the exact source rate".to_owned());
    }
    if actual.resampling {
        return Err("ALSA software rate conversion is enabled".to_owned());
    }
    if !actual.interleaved {
        return Err("ALSA hardware did not retain interleaved frame access".to_owned());
    }
    if actual.channels != u32::from(source.channels()) {
        return Err("ALSA hardware changed the source channel count".to_owned());
    }
    let layout = actual
        .layout
        .ok_or("ALSA hardware channel mapping is unknown")?;
    let format = ExactOutputFormat::new(
        source.sample_rate(),
        layout,
        actual.encoding.representation(),
        Some(actual.effective_bits),
    )
    .map_err(|error| error.to_string())?;
    ExactPcmAdapter::new(source, format).map_err(|error| error.to_string())?;
    Ok(format)
}

fn channel_layout(text: &str) -> Option<ChannelLayout> {
    match text.trim() {
        "MONO" | "FC" => Some(ChannelLayout::MONO),
        "FL FR" => Some(ChannelLayout::STEREO),
        _ => None,
    }
}

struct ControlMap<'a> {
    card: i32,
    device: u32,
    subdevice: u32,
    index: u32,
    integer: bool,
    positions: &'a [i32],
}

fn verify_control_map(
    selected: (i32, u32, u32),
    pcm: ChannelLayout,
    actual: Option<ControlMap<'_>>,
) -> Result<(), String> {
    let actual = actual.ok_or("selected ALSA hardware channel-map control is missing")?;
    if selected.0 < 0
        || actual.card != selected.0
        || actual.device != selected.1
        || actual.index != selected.2
        || actual.subdevice != 0
        || !actual.integer
    {
        return Err("ALSA hardware channel-map control identity or type is unverified".to_owned());
    }
    // ALSA's stable channel-position IDs: MONO=2, FL=3, FR=4, FC=7.
    // Extra channel flags/positions are not assumptions about this route.
    let hardware = match actual.positions {
        [2] | [7] => ChannelLayout::MONO,
        [3, 4] => ChannelLayout::STEREO,
        _ => {
            return Err(
                "ALSA hardware channel-map positions are unknown or unsupported".to_owned(),
            );
        }
    };
    if hardware != pcm {
        return Err("configured ALSA PCM mapping differs from the hardware control".to_owned());
    }
    Ok(())
}

struct Selection {
    card: String,
    device: u32,
    subdevice: u32,
}

fn number(value: &str) -> Result<u32, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("ALSA device/subdevice index must be an unsigned decimal integer".to_owned());
    }
    value
        .parse()
        .map_err(|_| "ALSA device/subdevice index exceeds u32".to_owned())
}

fn selection(name: &str) -> Result<Selection, String> {
    if !name.starts_with("hw:") || name.len() > 256 || name[3..].is_empty() {
        return Err("strict ALSA output requires an explicit hw: device ID".to_owned());
    }
    let fields = name[3..].split(',').collect::<Vec<_>>();
    if !(2..=3).contains(&fields.len()) {
        return Err("strict ALSA selection requires CARD and DEV, with optional SUBDEV".to_owned());
    }
    let (card, device, subdevice) = if fields.iter().any(|field| field.contains('=')) {
        let (mut card, mut device, mut subdevice) = (None, None, None);
        for field in &fields {
            match field.split_once('=') {
                Some(("CARD", value)) if card.is_none() => card = Some(value),
                Some(("DEV", value)) if device.is_none() => device = Some(number(value)?),
                Some(("SUBDEV", value)) if subdevice.is_none() => subdevice = Some(number(value)?),
                _ => return Err("ALSA selection has an unknown or duplicated field".to_owned()),
            }
        }
        (
            card.ok_or("ALSA selection is missing CARD")?,
            device.ok_or("ALSA selection is missing DEV")?,
            subdevice.unwrap_or(0),
        )
    } else {
        (
            fields[0],
            number(fields[1])?,
            if fields.len() == 3 {
                number(fields[2])?
            } else {
                0
            },
        )
    };
    if card.is_empty()
        || !card
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "ALSA card ID must be a numeric index or a simple symbolic identifier".to_owned(),
        );
    }
    Ok(Selection {
        card: card.to_owned(),
        device,
        subdevice,
    })
}

fn matches_identity(
    requested: &Selection,
    resolved_card: i32,
    actual: (i32, u32, u32),
    substreams: u32,
) -> bool {
    resolved_card >= 0
        && substreams == 1
        && actual == (resolved_card, requested.device, requested.subdevice)
}

fn hardware_name(name: &str) -> Result<CString, String> {
    selection(name)?;
    CString::new(name).map_err(|_| "ALSA device ID contains a NUL byte".to_owned())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn observe(pcm: &PCM, encoding: Encoding) -> Result<Readback, String> {
    let hw = pcm.hw_params_current().map_err(|error| error.to_string())?;
    if hw.get_format().map_err(|error| error.to_string())? != encoding.alsa() {
        return Err("ALSA negotiated another PCM representation".to_owned());
    }
    let mapping = pcm
        .get_chmap()
        .map_err(|error| format!("cannot read ALSA channel mapping: {error}"))?;
    let mut text = String::new();
    write!(&mut text, "{mapping}").map_err(|_| "cannot describe ALSA channel mapping")?;
    Ok(Readback {
        hardware: pcm.is_hardware(),
        rate: hw.get_rate_numden().map_err(|error| error.to_string())?,
        channels: hw.get_channels().map_err(|error| error.to_string())?,
        layout: channel_layout(&text),
        effective_bits: hw.get_sbits().map_err(|error| error.to_string())?,
        resampling: hw.get_rate_resample().map_err(|error| error.to_string())?,
        interleaved: hw.get_access().map_err(|error| error.to_string())? == Access::RWInterleaved,
        encoding,
    })
}

// Admit only standard controls with a directly observable neutral meaning.
// Unknown effects, enumerations, routing and vendor controls are refusals.
#[derive(Clone, Copy, Eq, PartialEq)]
enum ControlKind {
    Integer,
    Boolean,
    Unknown,
}

fn neutral_control(name: &str, kind: ControlKind, values: &[i64], db: Option<&[i64]>) -> bool {
    match name {
        "PCM Playback Volume"
        | "Master Playback Volume"
        | "Speaker Playback Volume"
        | "Headphone Playback Volume" => {
            kind == ControlKind::Integer
                && !values.is_empty()
                && db.is_some_and(|db| {
                    db.len() == values.len() && db.iter().all(|value| *value == 0)
                })
        }
        "PCM Playback Switch"
        | "Master Playback Switch"
        | "Speaker Playback Switch"
        | "Headphone Playback Switch" => {
            kind == ControlKind::Boolean
                && !values.is_empty()
                && values.iter().all(|value| *value == 1)
        }
        _ => false,
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn controls(card: i32, device: u32, subdevice: u32, layout: ChannelLayout) -> Result<Ctl, String> {
    if card < 0 {
        return Err("ALSA hardware card identity is unknown".to_owned());
    }
    let name = format!("hw:{card}");
    let ctl = Ctl::new(&name, true).map_err(|error| error.to_string())?;
    let actual_card = ctl
        .card_info()
        .map_err(|error| error.to_string())?
        .get_card()
        .get_index();
    if actual_card != card {
        return Err("ALSA controls resolved to another hardware card".to_owned());
    }
    ctl.subscribe_events(true)
        .map_err(|error| error.to_string())?;
    let inventory = ctl.elem_list().map_err(|error| error.to_string())?;
    if inventory.get_used() > 256 {
        return Err("ALSA control inventory exceeds the bounded strict profile".to_owned());
    }
    let hctl =
        HCtl::new(&name, true).map_err(|error| format!("cannot observe ALSA controls: {error}"))?;
    hctl.load().map_err(|error| error.to_string())?;
    let mut verified_map = false;
    for index in 0..inventory.get_used() {
        let id = inventory.get_id(index).map_err(|error| error.to_string())?;
        let name = id.get_name().map_err(|error| error.to_string())?;
        let display_name = crate::ui::literal(name);
        let is_map = inventory
            .get_interface(index)
            .map_err(|error| error.to_string())?
            == ElemIface::PCM
            && name == "Playback Channel Map";
        if is_map && (id.get_device() != device || id.get_index() != subdevice) {
            continue;
        }
        let element = hctl
            .find_elem(&id)
            .ok_or("ALSA control metadata changed during admission")?;
        let info = element.info().map_err(|error| error.to_string())?;
        if !(1..=2).contains(&info.get_count()) {
            return Err(format!("unknown ALSA control shape: {display_name}"));
        }
        let mut value = ElemValue::new(info.get_type()).map_err(|error| error.to_string())?;
        value.set_id(&id);
        // Read values and retain event monitoring on the same verified card
        // handle. PCM::get_chmap can instead return an ALSA config override.
        ctl.elem_read(&mut value)
            .map_err(|error| error.to_string())?;
        if is_map {
            if verified_map {
                return Err("ALSA selected channel-map control is ambiguous".to_owned());
            }
            let positions = (0..info.get_count())
                .map(|channel| {
                    value
                        .get_integer(channel)
                        .ok_or("invalid ALSA channel-map control")
                })
                .collect::<Result<Vec<_>, _>>()?;
            verify_control_map(
                (card, device, subdevice),
                layout,
                Some(ControlMap {
                    card: actual_card,
                    device: id.get_device(),
                    subdevice: id.get_subdevice(),
                    index: id.get_index(),
                    integer: info.get_type() == ElemType::Integer,
                    positions: &positions,
                }),
            )?;
            verified_map = true;
            continue;
        }
        let mut values = Vec::with_capacity(info.get_count() as usize);
        let mut decibels = Vec::with_capacity(info.get_count() as usize);
        for channel in 0..info.get_count() {
            match info.get_type() {
                ElemType::Integer => {
                    let integer = value
                        .get_integer(channel)
                        .ok_or("invalid ALSA integer control")?;
                    values.push(i64::from(integer));
                    decibels.push(
                        ctl.convert_to_db(&id, i64::from(integer))
                            .map_err(|error| {
                                format!("unknown ALSA gain for {display_name}: {error}")
                            })?
                            .0,
                    );
                }
                ElemType::Boolean => values.push(i64::from(
                    value
                        .get_boolean(channel)
                        .ok_or("invalid ALSA switch control")?,
                )),
                _ => {
                    return Err(format!(
                        "unknown ALSA enhancement or routing control: {display_name}"
                    ));
                }
            }
        }
        let kind = match info.get_type() {
            ElemType::Integer => ControlKind::Integer,
            ElemType::Boolean => ControlKind::Boolean,
            _ => ControlKind::Unknown,
        };
        if !neutral_control(
            name,
            kind,
            &values,
            (!decibels.is_empty()).then_some(decibels.as_slice()),
        ) {
            return Err(format!(
                "ALSA control is modifying or unverified: {display_name}; strict output does not change system controls"
            ));
        }
    }
    if !verified_map {
        verify_control_map((card, device, subdevice), layout, None)?;
    }
    Ok(ctl)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Fault {
    Host = 1,
    Xrun,
    RouteChanged,
    InvalidFrames,
    PauseUnsupported,
    WorkerPanic,
    StopTimeout,
}

impl Fault {
    fn message(self) -> &'static str {
        match self {
            Self::Host => "strict ALSA host operation failed",
            Self::Xrun => "strict ALSA host reported an xrun",
            Self::RouteChanged => "strict ALSA route or controls changed",
            Self::InvalidFrames => "strict ALSA frame transfer is invalid",
            Self::PauseUnsupported => "ALSA hardware cannot pause without altering the programme",
            Self::WorkerPanic => "strict ALSA output worker panicked",
            Self::StopTimeout => {
                "strict ALSA output worker did not stop within its bounded deadline"
            }
        }
    }
    fn from_code(code: u8) -> Self {
        match code {
            2 => Self::Xrun,
            3 => Self::RouteChanged,
            4 => Self::InvalidFrames,
            5 => Self::PauseUnsupported,
            6 => Self::WorkerPanic,
            7 => Self::StopTimeout,
            _ => Self::Host,
        }
    }
}

#[derive(Default)]
struct Shared {
    command: AtomicU8,
    acknowledged: AtomicU8,
    start: AtomicBool,
    started: AtomicBool,
    complete: AtomicBool,
    exited: AtomicBool,
    fatal: AtomicU8,
    played: AtomicU64,
}

impl Shared {
    fn fail(&self, fault: Fault, queue: &QueueFailure) {
        let _ = self
            .fatal
            .compare_exchange(0, fault as u8, Ordering::Release, Ordering::Relaxed);
        queue.fail();
    }
    fn check(&self) -> Result<(), String> {
        let code = self.fatal.load(Ordering::Acquire);
        if code == 0 {
            Ok(())
        } else {
            Err(Fault::from_code(code).message().to_owned())
        }
    }
    fn joined(&self, result: Result<(), String>) -> Result<(), String> {
        // Joining proves thread termination, not a successful host stop. Keep
        // the first latched cause even if cleanup reports another failure.
        self.check()?;
        result
    }
}

fn encode(
    source: IntegerPcmFormat,
    encoding: Encoding,
    samples: &[i32],
    output: &mut [u8],
) -> Result<usize, Fault> {
    let limit = 1_i32 << (source.bits_per_sample() - 1);
    let bytes = samples
        .len()
        .checked_mul(encoding.width())
        .ok_or(Fault::InvalidFrames)?;
    if !samples.len().is_multiple_of(usize::from(source.channels()))
        || samples.len() / usize::from(source.channels()) > MAX_FRAMES
        || source.bits_per_sample() > encoding.bits()
        || bytes > output.len()
        || samples.iter().any(|value| !(-limit..limit).contains(value))
    {
        return Err(Fault::InvalidFrames);
    }
    for (index, sample) in samples.iter().copied().enumerate() {
        let widened = sample << (encoding.bits() - source.bits_per_sample());
        let bytes = widened.to_le_bytes();
        let start = index * encoding.width();
        output[start..start + encoding.width()].copy_from_slice(&bytes[..encoding.width()]);
    }
    Ok(bytes)
}

trait Transport: Send {
    fn controls_unchanged(&self) -> Result<(), Fault>;
    fn space(&self) -> Result<usize, Fault>;
    fn write(&mut self, bytes: &[u8]) -> Result<usize, Fault>;
    fn start(&mut self) -> Result<(), Fault>;
    fn pause(&mut self, paused: bool) -> Result<(), Fault>;
    fn delay(&self) -> Result<u64, Fault>;
    fn finish(&mut self) -> Result<bool, Fault>;
    fn stop(&mut self) -> Result<(), Fault>;
}

#[derive(Clone, Copy)]
enum HardwareState {
    Prepared,
    Running,
    Paused,
    Draining,
    Setup,
    Xrun,
    Unknown,
}

fn healthy_state(
    state: HardwareState,
    started: bool,
    paused: bool,
    draining: bool,
) -> Result<(), Fault> {
    match state {
        HardwareState::Prepared if !started => Ok(()),
        HardwareState::Running if started && !paused => Ok(()),
        HardwareState::Paused if started && paused => Ok(()),
        HardwareState::Draining if started && draining && !paused => Ok(()),
        HardwareState::Setup if started && draining => Ok(()),
        HardwareState::Xrun => Err(Fault::Xrun),
        _ => Err(Fault::RouteChanged),
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct Hardware {
    pcm: PCM,
    controls: Ctl,
    can_pause: bool,
    started: bool,
    draining: bool,
    paused: bool,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl Hardware {
    fn healthy(&self) -> Result<(), Fault> {
        let observed = match self.pcm.state() {
            State::Prepared => HardwareState::Prepared,
            State::Running => HardwareState::Running,
            State::Paused => HardwareState::Paused,
            State::Draining => HardwareState::Draining,
            State::Setup => HardwareState::Setup,
            State::XRun => HardwareState::Xrun,
            _ => HardwareState::Unknown,
        };
        healthy_state(observed, self.started, self.paused, self.draining)
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl Transport for Hardware {
    fn controls_unchanged(&self) -> Result<(), Fault> {
        self.healthy()?;
        if self.controls.wait(Some(0)).map_err(|_| Fault::Host)? {
            Err(Fault::RouteChanged)
        } else {
            Ok(())
        }
    }
    fn space(&self) -> Result<usize, Fault> {
        self.healthy()?;
        usize::try_from(self.pcm.avail_update().map_err(|_| Fault::Host)?)
            .map_err(|_| Fault::InvalidFrames)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, Fault> {
        let result = self.pcm.io_bytes().writei(bytes);
        match result {
            Ok(count) => Ok(count),
            Err(error)
                if io::Error::from_raw_os_error(error.errno()).kind()
                    == io::ErrorKind::WouldBlock =>
            {
                Ok(0)
            }
            Err(_) => {
                self.healthy()?;
                Err(Fault::Host)
            }
        }
    }
    fn start(&mut self) -> Result<(), Fault> {
        self.pcm.start().map_err(|_| Fault::Host)?;
        self.started = true;
        Ok(())
    }
    fn pause(&mut self, paused: bool) -> Result<(), Fault> {
        if !self.can_pause {
            return Err(Fault::PauseUnsupported);
        }
        self.pcm.pause(paused).map_err(|_| Fault::Host)?;
        self.paused = paused;
        Ok(())
    }
    fn delay(&self) -> Result<u64, Fault> {
        u64::try_from(self.pcm.delay().map_err(|_| Fault::Host)?).map_err(|_| Fault::Xrun)
    }
    fn finish(&mut self) -> Result<bool, Fault> {
        self.draining = true;
        match self.pcm.drain() {
            Ok(()) => Ok(true),
            Err(error)
                if io::Error::from_raw_os_error(error.errno()).kind()
                    == io::ErrorKind::WouldBlock =>
            {
                Ok(false)
            }
            Err(_) => {
                self.healthy()?;
                Err(Fault::Host)
            }
        }
    }
    fn stop(&mut self) -> Result<(), Fault> {
        self.pcm.drop().map_err(|_| Fault::Host)
    }
}

struct Worker<T: Transport> {
    hardware: T,
    pending: PcmConsumer<i32>,
    failure: QueueFailure,
    shared: Arc<Shared>,
    source: IntegerPcmFormat,
    encoding: Encoding,
    period: usize,
    samples: Vec<i32>,
    bytes: Vec<u8>,
    offset: usize,
    length: usize,
    written: u64,
    paused: bool,
    started: bool,
}

impl<T: Transport> Worker<T> {
    fn interrupted(&self) -> bool {
        self.shared.command.load(Ordering::Acquire) == STOPPED || self.pending.has_failed()
    }

    fn step(&mut self) -> Result<bool, Fault> {
        if self.interrupted() {
            self.hardware.stop()?;
            return Ok(true);
        }
        self.hardware.controls_unchanged()?;
        let paused = self.shared.command.load(Ordering::Acquire) == PAUSED;
        if self.started && paused != self.paused {
            self.hardware.pause(paused)?;
        }
        self.paused = paused;
        if self.started && paused {
            self.observe_played()?;
            self.shared.acknowledged.store(PAUSED, Ordering::Release);
            return Ok(false);
        }
        self.shared
            .acknowledged
            .store(if paused { PAUSED } else { RUNNING }, Ordering::Release);
        let frame_bytes = usize::from(self.source.channels()) * self.encoding.width();
        if self.offset == self.length {
            let available = self.pending.available_frames().min(self.period);
            let frames = if available == 0 {
                0
            } else {
                available.min(self.hardware.space()?)
            };
            self.offset = 0;
            self.length = 0;
            if frames > 0 {
                let count = frames * usize::from(self.source.channels());
                self.pending
                    .render_mapped(&mut self.samples[..count], 0, |value| value);
                if self.pending.has_failed() {
                    return Err(Fault::InvalidFrames);
                }
                self.length = encode(
                    self.source,
                    self.encoding,
                    &self.samples[..count],
                    &mut self.bytes,
                )?;
            }
        }
        if self.offset < self.length {
            let frames = self.hardware.write(&self.bytes[self.offset..self.length])?;
            let accepted = frames
                .checked_mul(frame_bytes)
                .ok_or(Fault::InvalidFrames)?;
            if accepted > self.length - self.offset {
                return Err(Fault::InvalidFrames);
            }
            self.offset += accepted;
            self.written = self
                .written
                .checked_add(frames as u64)
                .ok_or(Fault::InvalidFrames)?;
        }
        // A host write can overlap stop, pause or source failure. Those events
        // must be observed before a prepared device is first activated.
        if self.interrupted() {
            if self.started {
                self.observe_played()?;
            }
            self.hardware.stop()?;
            return Ok(true);
        }
        if !self.started
            && self.written > 0
            && self.shared.start.load(Ordering::Acquire)
            && ((self.pending.available_frames() == 0 && self.offset == self.length)
                || self.hardware.space()? == 0)
            && self.shared.command.load(Ordering::Acquire) == RUNNING
            && !self.pending.has_failed()
        {
            self.hardware.start()?;
            self.started = true;
            self.shared.started.store(true, Ordering::Release);
        }
        if self.interrupted() {
            if self.started {
                self.observe_played()?;
            }
            self.hardware.stop()?;
            return Ok(true);
        }
        let ended = self.pending.input_closed()
            && self.pending.available_frames() == 0
            && self.offset == self.length;
        if ended && self.written == 0 {
            self.shared.complete.store(true, Ordering::Release);
            return Ok(true);
        }
        if self.started
            && ended
            && self.shared.command.load(Ordering::Acquire) == RUNNING
            && self.hardware.finish()?
        {
            if self.interrupted() {
                self.hardware.stop()?;
                return Ok(true);
            }
            self.shared.played.store(self.written, Ordering::Release);
            self.shared.complete.store(true, Ordering::Release);
            return Ok(true);
        }
        if self.started {
            self.observe_played()?;
        }
        Ok(false)
    }

    fn observe_played(&self) -> Result<(), Fault> {
        let delay = self.hardware.delay()?;
        if delay > self.written {
            return Err(Fault::InvalidFrames);
        }
        self.shared
            .played
            .fetch_max(self.written - delay, Ordering::Release);
        Ok(())
    }

    fn run(mut self) {
        loop {
            match self.step() {
                Ok(true) => break,
                Ok(false) => thread::sleep(WORKER_WAIT),
                Err(error) => {
                    self.shared.fail(error, &self.failure);
                    if let Err(cleanup) = self.hardware.stop() {
                        self.shared.fail(cleanup, &self.failure);
                    }
                    break;
                }
            }
        }
        self.shared.exited.store(true, Ordering::Release);
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(super) struct AlsaOutput {
    source: IntegerPcmFormat,
    exact: ExactOutputFormat,
    encoding: Encoding,
    producer: PcmProducer<i32>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    closed: bool,
    can_pause: bool,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl AlsaOutput {
    pub(super) fn open(source: IntegerPcmFormat, hw_name: &str) -> Result<Self, String> {
        let requested = selection(hw_name)?;
        let card_name =
            CString::new(requested.card.as_str()).map_err(|_| "ALSA card ID contains NUL")?;
        let resolved_card = alsa::Card::from_str(&card_name)
            .map_err(|error| format!("cannot resolve explicitly selected ALSA card: {error}"))?
            .get_index();
        let name = hardware_name(hw_name)?;
        let pcm = PCM::open(&name, Direction::Playback, true)
            .map_err(|error| format!("cannot open explicit ALSA hardware: {error}"))?;
        if !pcm.is_hardware() {
            return Err("ALSA device name resolved to a modifying/unknown plugin".to_owned());
        }
        let identity = pcm.info().map_err(|error| error.to_string())?;
        if !matches_identity(
            &requested,
            resolved_card,
            (
                identity.get_card(),
                identity.get_device(),
                identity.get_subdevice(),
            ),
            identity.get_subdevices_count(),
        ) {
            return Err("ALSA selected card/device/subdevice changed or has unverified multi-substream mixing".to_owned());
        }
        let capacity = usize::try_from(u64::from(source.sample_rate()).div_ceil(2))
            .map_err(|_| "ALSA queue capacity overflow")?;
        if capacity == 0 || capacity > MAX_QUEUE_FRAMES {
            return Err("source rate exceeds the bounded direct ALSA queue profile".to_owned());
        }
        let mut chosen = None;
        let mut refusal = "ALSA hardware has no eligible integer format".to_owned();
        let formats: &[Encoding] = if source.bits_per_sample() == 16 {
            &[
                Encoding::Signed16,
                Encoding::Signed24In32,
                Encoding::Packed24,
                Encoding::Signed32,
            ]
        } else {
            &[
                Encoding::Signed24In32,
                Encoding::Packed24,
                Encoding::Signed32,
            ]
        };
        for &encoding in formats {
            let result = (|| {
                let hw = HwParams::any(&pcm).map_err(|error| error.to_string())?;
                hw.set_access(Access::RWInterleaved)
                    .map_err(|error| error.to_string())?;
                hw.set_rate_resample(false)
                    .map_err(|error| error.to_string())?;
                hw.set_format(encoding.alsa())
                    .map_err(|error| error.to_string())?;
                hw.set_channels(u32::from(source.channels()))
                    .map_err(|error| error.to_string())?;
                hw.set_rate(source.sample_rate(), ValueOr::Nearest)
                    .map_err(|error| error.to_string())?;
                let period = hw
                    .set_period_size_near(
                        Frames::try_from(u64::from(source.sample_rate().div_ceil(100)))
                            .map_err(|_| "ALSA period exceeds the host frame type")?,
                        ValueOr::Nearest,
                    )
                    .map_err(|error| error.to_string())?;
                hw.set_buffer_size_near(period.checked_mul(4).ok_or("ALSA buffer overflow")?)
                    .map_err(|error| error.to_string())?;
                pcm.hw_params(&hw).map_err(|error| error.to_string())?;
                drop(hw);
                let exact = admit(source, observe(&pcm, encoding)?)?;
                let current = pcm.hw_params_current().map_err(|error| error.to_string())?;
                let period = usize::try_from(
                    current
                        .get_period_size()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|_| "invalid ALSA period")?;
                let buffer = usize::try_from(
                    current
                        .get_buffer_size()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|_| "invalid ALSA buffer")?;
                if period == 0 || period > MAX_FRAMES || buffer < period || buffer > capacity {
                    return Err(
                        "ALSA buffer geometry exceeds the bounded strict profile".to_owned()
                    );
                }
                Ok((exact, period, current.can_pause()))
            })();
            match result {
                Ok((exact, period, can_pause)) => {
                    chosen = Some((encoding, exact, period, can_pause));
                    break;
                }
                Err(error) => refusal = error,
            }
        }
        let (encoding, exact, period, can_pause) = chosen.ok_or(refusal)?;
        let sw = pcm.sw_params_current().map_err(|error| error.to_string())?;
        let boundary = sw.get_boundary().map_err(|error| error.to_string())?;
        let buffer_size = Frames::try_from(pcm.get_params().map_err(|error| error.to_string())?.0)
            .map_err(|_| "invalid ALSA buffer size")?;
        sw.set_start_threshold(boundary)
            .map_err(|error| error.to_string())?;
        sw.set_stop_threshold(buffer_size)
            .map_err(|error| error.to_string())?;
        sw.set_avail_min(period as Frames)
            .map_err(|error| error.to_string())?;
        pcm.sw_params(&sw).map_err(|error| error.to_string())?;
        drop(sw);
        let configured = pcm.sw_params_current().map_err(|error| error.to_string())?;
        if configured
            .get_start_threshold()
            .map_err(|error| error.to_string())?
            != boundary
            || configured
                .get_stop_threshold()
                .map_err(|error| error.to_string())?
                != buffer_size
            || configured
                .get_avail_min()
                .map_err(|error| error.to_string())?
                != period as Frames
        {
            return Err("ALSA startup/stop thresholds changed during preparation".to_owned());
        }
        // A fresh successful hw_params installs zero silence threshold/size in
        // alsa-lib. Changing only these sw thresholds preserves that state;
        // this owned handle has no other software-parameter writer.
        drop(configured);
        pcm.prepare().map_err(|error| error.to_string())?;
        let card = pcm.info().map_err(|error| error.to_string())?.get_card();
        let controls = controls(card, requested.device, requested.subdevice, source.layout())?;
        let (producer, pending) = pcm_queue::<i32>(usize::from(source.channels()), capacity, true)
            .map_err(|error| error.to_string())?;
        let shared = Arc::new(Shared::default());
        let worker = Worker {
            hardware: Hardware {
                pcm,
                controls,
                can_pause,
                started: false,
                draining: false,
                paused: false,
            },
            pending,
            failure: producer.failure_handle(),
            shared: Arc::clone(&shared),
            source,
            encoding,
            period,
            samples: vec![0; MAX_FRAMES * usize::from(source.channels())],
            bytes: vec![0; MAX_FRAMES * usize::from(source.channels()) * encoding.width()],
            offset: 0,
            length: 0,
            written: 0,
            paused: false,
            started: false,
        };
        let panic_status = Arc::clone(&shared);
        let failure = producer.failure_handle();
        let handle = thread::Builder::new()
            .name("aede-alsa-output".to_owned())
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker.run())).is_err()
                {
                    panic_status.fail(Fault::WorkerPanic, &failure);
                    panic_status.exited.store(true, Ordering::Release);
                }
            })
            .map_err(|error| format!("cannot launch ALSA output worker: {error}"))?;
        Ok(Self {
            source,
            exact,
            encoding,
            producer,
            shared,
            thread: Some(handle),
            closed: false,
            can_pause,
        })
    }
    pub(super) fn exact_format(&self) -> ExactOutputFormat {
        self.exact
    }
    pub(super) fn input_closed(&self) -> bool {
        self.closed
    }
    pub(super) fn exact_needs_reopen(&self, source: IntegerPcmFormat) -> Result<bool, String> {
        self.check()?;
        Ok(self.closed || self.source != source)
    }
    pub(super) fn consumed_frames(&self) -> u64 {
        self.shared.played.load(Ordering::Acquire)
    }
    pub(super) fn drain_progress(&self) -> Result<DrainProgress, String> {
        self.check()?;
        Ok(DrainProgress {
            drained: self.shared.complete.load(Ordering::Acquire),
            consumed_frames: Some(self.consumed_frames()),
        })
    }
    pub(super) fn host_tail(&self) -> Duration {
        Duration::ZERO
    }
    pub(super) fn check(&self) -> Result<(), String> {
        self.shared.check()?;
        if self.producer.snapshot().failed {
            return Err("strict ALSA source queue failed".to_owned());
        }
        Ok(())
    }
    fn command(&self, command: u8) -> Result<(), String> {
        self.check()?;
        self.shared.command.store(command, Ordering::Release);
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        while self.shared.acknowledged.load(Ordering::Acquire) != command {
            self.check()?;
            if self.shared.complete.load(Ordering::Acquire) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                self.shared
                    .fail(Fault::StopTimeout, &self.producer.failure_handle());
                return Err("ALSA control acknowledgement timed out".to_owned());
            }
            thread::sleep(WORKER_WAIT);
        }
        Ok(())
    }
    pub(super) fn pause(&self) -> Result<(), String> {
        if self.shared.started.load(Ordering::Acquire) && !self.can_pause {
            self.shared
                .fail(Fault::PauseUnsupported, &self.producer.failure_handle());
            return Err(Fault::PauseUnsupported.message().to_owned());
        }
        self.command(PAUSED)
    }
    pub(super) fn resume(&self) -> Result<(), String> {
        self.command(RUNNING)
    }
    pub(super) fn ensure_started(&self) -> Result<(), String> {
        self.check()?;
        self.shared.start.store(true, Ordering::Release);
        Ok(())
    }
    pub(super) fn write_exact(&mut self, samples: &[i32]) -> io::Result<usize> {
        self.check().map_err(io::Error::other)?;
        let queued = self.producer.snapshot();
        if queued.queued_frames == queued.capacity_frames {
            self.ensure_started().map_err(io::Error::other)?;
        }
        self.producer.write_integer(samples, self.source)
    }
    pub(super) fn close_input(&mut self) {
        self.closed = true;
        self.producer.close_input();
        self.shared.start.store(true, Ordering::Release);
    }
    fn join(&mut self) -> Result<(), String> {
        self.shared.command.store(STOPPED, Ordering::Release);
        let deadline = Instant::now() + CONTROL_TIMEOUT;
        if let Some(handle) = self.thread.as_ref() {
            while !handle.is_finished() {
                if Instant::now() >= deadline {
                    self.shared
                        .fail(Fault::StopTimeout, &self.producer.failure_handle());
                    return Err(Fault::StopTimeout.message().to_owned());
                }
                thread::sleep(WORKER_WAIT);
            }
        }
        if let Some(handle) = self.thread.take() {
            handle
                .join()
                .map_err(|_| Fault::WorkerPanic.message().to_owned())?;
        }
        Ok(())
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        self.check()?;
        if !self.shared.complete.load(Ordering::Acquire) {
            return Err("direct ALSA output must be drained before finalization".to_owned());
        }
        let result = self.join();
        self.shared.joined(result)
    }
    pub(super) fn abort(mut self) -> Result<(), String> {
        self.producer.failure_handle().fail();
        let result = self.join();
        self.shared.joined(result)
    }
    pub(super) fn stage_description(&self) -> &'static str {
        "direct ALSA exact PCM; no DSP or dither"
    }
    pub(super) fn integer_description(&self) -> Option<&'static str> {
        Some(match self.encoding {
            Encoding::Signed16 => "signed 16-bit PCM",
            Encoding::Signed24In32 => "signed 24-bit PCM in a 32-bit container",
            Encoding::Packed24 => "packed signed 24-bit PCM",
            Encoding::Signed32 => "signed 32-bit PCM with observed effective precision",
        })
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl Drop for AlsaOutput {
    fn drop(&mut self) {
        self.producer.failure_handle().fail();
        let _ = self.join();
    }
}

#[cfg(test)]
#[path = "play_alsa_output_tests.rs"]
mod tests;
