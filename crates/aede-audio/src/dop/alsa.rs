//! Nonblocking direct ALSA DoP. Hardware parameters belong to one retained PCM
//! handle; no mixer setting, resampling, recovery or default device is selected.
use super::{
    Carrier, Format, Payload,
    driver::Driver,
    push::{Push, Sink},
    state::Signals,
};
use crate::{alsa_hw, queue::PcmConsumer};
use alsa::{
    Ctl, Direction, ValueOr,
    pcm::{Access, Format as Encoding, Frames, HwParams, PCM, State},
};
use std::{ffi::CString, sync::Arc, time::Duration};

pub(super) fn open(
    name: &str,
    format: Format,
    input: PcmConsumer<Payload>,
    signals: Arc<Signals>,
) -> Result<Box<dyn Driver>, String> {
    signals.current()?;
    let route = alsa_hw::selection(name)?;
    let card = alsa::Card::from_str(&CString::new(route.card.as_str()).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .get_index();
    let pcm = PCM::open(&alsa_hw::hardware_name(name)?, Direction::Playback, true)
        .map_err(|e| e.to_string())?;
    let info = pcm.info().map_err(|e| e.to_string())?;
    if !pcm.is_hardware()
        || !alsa_hw::matches_identity(
            &route,
            card,
            (info.get_card(), info.get_device(), info.get_subdevice()),
            info.get_subdevices_count(),
        )
    {
        return Err(
            "DoP requires the exact selected direct ALSA hardware and a single substream".into(),
        );
    }
    let mut selected = None;
    let mut refusal = String::from("ALSA has no exact integer DoP format");
    for (encoding, carrier) in [
        (Encoding::S243LE, Carrier::Packed24Le),
        (Encoding::S24LE, Carrier::Low24In32Le),
    ] {
        match configure(&pcm, format, encoding) {
            Ok(capacity) => {
                selected = Some((carrier, capacity, encoding));
                break;
            }
            Err(error) => refusal = error,
        }
    }
    let (carrier, capacity, encoding) = selected.ok_or(refusal)?;
    let controls = alsa_hw::open_controls(
        card,
        route.device,
        route.subdevice,
        crate::dsp::ChannelLayout::STEREO,
    )?;
    pcm.prepare().map_err(|e| e.to_string())?;
    signals.current()?;
    let sink = Hardware {
        pcm: Some(pcm),
        controls: Some(controls),
        format,
        encoding,
        capacity,
        started: false,
    };
    sink.verify()?;
    Ok(Box::new(Push::new(
        sink, format, carrier, input, signals, capacity,
    )?))
}
fn configure(pcm: &PCM, format: Format, encoding: Encoding) -> Result<usize, String> {
    let hw = HwParams::any(pcm).map_err(|e| e.to_string())?;
    hw.set_access(Access::RWInterleaved)
        .map_err(|e| e.to_string())?;
    hw.set_rate_resample(false).map_err(|e| e.to_string())?;
    hw.set_format(encoding).map_err(|e| e.to_string())?;
    hw.set_channels(2).map_err(|e| e.to_string())?;
    hw.set_rate(format.carrier_rate(), ValueOr::Nearest)
        .map_err(|e| e.to_string())?;
    let period = hw
        .set_period_size_near(
            Frames::from(format.carrier_rate().div_ceil(100)),
            ValueOr::Nearest,
        )
        .map_err(|e| e.to_string())?;
    hw.set_buffer_size_near(period.checked_mul(4).ok_or("ALSA DoP buffer overflow")?)
        .map_err(|e| e.to_string())?;
    pcm.hw_params(&hw).map_err(|e| e.to_string())?;
    drop(hw);
    let actual = pcm.hw_params_current().map_err(|e| e.to_string())?;
    verify_format(&actual, format, encoding)?;
    let capacity = usize::try_from(actual.get_buffer_size().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let period = actual.get_period_size().map_err(|e| e.to_string())?;
    if !(1..=16384).contains(&capacity) || period <= 0 || period > capacity as Frames {
        return Err("ALSA DoP buffer exceeds the bounded profile".into());
    }
    drop(actual);
    let sw = pcm.sw_params_current().map_err(|e| e.to_string())?;
    let boundary = sw.get_boundary().map_err(|e| e.to_string())?;
    sw.set_start_threshold(boundary)
        .map_err(|e| e.to_string())?;
    sw.set_stop_threshold(capacity as Frames)
        .map_err(|e| e.to_string())?;
    sw.set_avail_min(period).map_err(|e| e.to_string())?;
    // Fresh hw_params installs zero silence threshold/size. Preserve those
    // defaults while changing only the three verified startup/stop thresholds.
    pcm.sw_params(&sw).map_err(|e| e.to_string())?;
    drop(sw);
    let actual = pcm.sw_params_current().map_err(|e| e.to_string())?;
    if actual.get_start_threshold().map_err(|e| e.to_string())? != boundary
        || actual.get_stop_threshold().map_err(|e| e.to_string())? != capacity as Frames
        || actual.get_avail_min().map_err(|e| e.to_string())? != period
    {
        return Err("ALSA changed DoP startup, stop or silence parameters".into());
    }
    Ok(capacity)
}
fn verify_format(hw: &HwParams<'_>, format: Format, encoding: Encoding) -> Result<(), String> {
    let (numerator, denominator) = hw.get_rate_numden().map_err(|e| e.to_string())?;
    if denominator == 0
        || u64::from(numerator) != u64::from(format.carrier_rate()) * u64::from(denominator)
        || hw.get_format().map_err(|e| e.to_string())? != encoding
        || hw.get_channels().map_err(|e| e.to_string())? != 2
        || hw.get_sbits().map_err(|e| e.to_string())? != 24
        || hw.get_rate_resample().map_err(|e| e.to_string())?
        || hw.get_access().map_err(|e| e.to_string())? != Access::RWInterleaved
    {
        return Err("ALSA did not retain exact stereo 24-bit integer DoP I/O".into());
    }
    Ok(())
}
struct Hardware {
    pcm: Option<PCM>,
    controls: Option<Ctl>,
    format: Format,
    encoding: Encoding,
    capacity: usize,
    started: bool,
}
impl Hardware {
    fn pcm(&self) -> Result<&PCM, String> {
        self.pcm
            .as_ref()
            .ok_or_else(|| "ALSA DoP output is closed".into())
    }
}
impl Sink for Hardware {
    fn verify(&self) -> Result<(), String> {
        let pcm = self.pcm()?;
        let expected = if self.started {
            State::Running
        } else {
            State::Prepared
        };
        if pcm.state() != expected {
            return Err("ALSA DoP route stopped, disconnected or reported an xrun".into());
        }
        verify_format(
            &pcm.hw_params_current().map_err(|e| e.to_string())?,
            self.format,
            self.encoding,
        )?;
        if alsa_hw::channel_layout(&pcm.get_chmap().map_err(|e| e.to_string())?.to_string())
            != Some(crate::dsp::ChannelLayout::STEREO)
        {
            return Err("ALSA DoP channel mapping changed".into());
        }
        if self
            .controls
            .as_ref()
            .ok_or("ALSA DoP controls are closed")?
            .wait(Some(0))
            .map_err(|e| e.to_string())?
        {
            return Err("ALSA DoP controls or route changed".into());
        }
        Ok(())
    }
    fn space(&self) -> Result<usize, String> {
        self.verify()?;
        let pcm = self.pcm()?;
        if self.started && pcm.delay().map_err(|e| e.to_string())? <= 0 {
            return Err("ALSA DoP buffer ran dry; no xrun recovery is performed".into());
        }
        usize::try_from(pcm.avail_update().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, String> {
        match self.pcm()?.io_bytes().writei(bytes) {
            Ok(frames) => Ok(frames),
            Err(error)
                if std::io::Error::from_raw_os_error(error.errno()).kind()
                    == std::io::ErrorKind::WouldBlock =>
            {
                Ok(0)
            }
            Err(error) => Err(error.to_string()),
        }
    }
    fn start(&mut self) -> Result<(), String> {
        self.pcm()?.start().map_err(|e| e.to_string())?;
        self.started = true;
        Ok(())
    }
    fn tail(&self) -> Duration {
        Duration::from_secs_f64(
            (self.capacity as f64 * 2.0) / f64::from(self.format.carrier_rate()),
        )
    }
    fn close(&mut self) -> Result<(), String> {
        let Some(pcm) = self.pcm.take() else {
            return Ok(());
        };
        let stop = pcm.drop().map_err(|e| e.to_string());
        // Drop closes the owned PCM descriptor and releases its parameters
        // before the control thread can publish successful completion.
        drop(pcm);
        self.controls.take();
        stop
    }
}
impl Drop for Hardware {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("ALSA DoP cleanup failed: {error}");
        }
    }
}

/// Read hardware names only; opening/format admission belongs to explicit playback.
pub(super) fn inventory() -> Result<Vec<(String, String)>, String> {
    let mut result = Vec::new();
    for card in alsa::card::Iter::new() {
        let card = card.map_err(|e| e.to_string())?;
        let index = card.get_index();
        let ctl = Ctl::new(&format!("hw:{index}"), true).map_err(|e| e.to_string())?;
        for device in alsa::ctl::DeviceIter::new(&ctl) {
            let info = match ctl.pcm_info(device as u32, 0, Direction::Playback) {
                Ok(info) => info,
                Err(_) => continue,
            };
            if result.len() >= 1024 {
                return Err("ALSA endpoint inventory exceeds its budget".into());
            }
            result.push((
                format!("hw:{index},{device},0"),
                info.get_name().map_err(|e| e.to_string())?.to_owned(),
            ));
        }
    }
    Ok(result)
}
