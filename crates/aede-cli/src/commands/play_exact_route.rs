//! Explicit native selection and conservative exact-output admission.
//!
//! Advertised CPAL formats describe a software interface, not the effective
//! device precision or a transparent system route. Keep those facts separate
//! and refuse unknown evidence rather than manufacture it from container size.

use aede_core::playback::exact_output::{
    ExactOutputFormat, ExactPcmAdapter, ExactSampleRepresentation,
};
use aede_core::playback::format::IntegerPcmFormat;
use aede_dsp::ChannelLayout;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, DeviceId, HostId, SampleFormat, SupportedStreamConfig};

#[derive(Clone, Debug, Default)]
pub(super) struct NativeRequest {
    pub(super) device: Option<DeviceId>,
    pub(super) strict: bool,
}

pub(super) struct SelectedDevice {
    pub(super) device: Device,
    pub(super) host: HostId,
    pub(super) id: DeviceId,
    explicit: bool,
}

pub(super) struct NativePlan {
    pub(super) config: SupportedStreamConfig,
    pub(super) output: ExactOutputFormat,
}

/// Resolve the requested host and ID once, without a named-device fallback.
pub(super) fn resolve_output(request: &NativeRequest) -> Result<SelectedDevice, String> {
    if request.strict && request.device.is_none() {
        return Err("strict output needs an explicitly selected native device ID".to_owned());
    }
    let host = match request.device.as_ref() {
        Some(id) => cpal::host_from_id(id.host())
            .map_err(|error| format!("cannot open requested audio host: {error}"))?,
        None => cpal::default_host(),
    };
    let device = choose_device(
        request.device.as_ref(),
        |id| host.device_by_id(id),
        || host.default_output_device(),
    )?;
    let id = device
        .id()
        .map_err(|error| format!("cannot identify selected audio device: {error}"))?;
    if let Some(requested) = request.device.as_ref()
        && requested != &id
    {
        return Err(format!(
            "requested audio device {requested} resolved to a different ID ({id})"
        ));
    }
    Ok(SelectedDevice {
        device,
        host: host.id(),
        id,
        explicit: request.device.is_some(),
    })
}

fn choose_device<T>(
    requested: Option<&DeviceId>,
    by_id: impl FnOnce(&DeviceId) -> Option<T>,
    default: impl FnOnce() -> Option<T>,
) -> Result<T, String> {
    match requested {
        Some(id) => by_id(id).ok_or_else(|| format!("requested audio device is unavailable: {id}")),
        None => default().ok_or_else(|| "no default audio output device".to_owned()),
    }
}

/// Inspect advertised formats without opening or starting an output stream.
///
/// The pinned CPAL backends do not expose all the required observations. No
/// CLI value or environment variable can substitute for those observations.
pub(super) fn strict_plan(
    source: IntegerPcmFormat,
    selected: &SelectedDevice,
) -> Result<NativePlan, String> {
    if !selected.explicit {
        return Err("strict output needs an explicitly selected native device ID".to_owned());
    }
    let candidates = selected
        .device
        .supported_output_configs()
        .map_err(|error| {
            format!(
                "cannot query selected audio device {}: {error}",
                selected.id
            )
        })?
        .map(|advertised| Candidate {
            advertised,
            layout: None,
            effective_bits: None,
            actual_rate: None,
            transparency: Transparency::Unknown,
        })
        .collect::<Vec<_>>();
    plan_candidates(source, &candidates).map_err(|error| {
        format!(
            "strict output refused for {}: {}; {error}",
            selected.id,
            backend_refusal(selected.host.name(), selected.id.id())
        )
    })
}

fn backend_refusal(host: &str, device_id: &str) -> &'static str {
    match host {
        "CoreAudio" => {
            "CPAL cannot verify exclusive access, physical precision, channel mapping or unity system gain; its physical-format request permits AudioUnit conversion"
        }
        "WASAPI" => {
            "CPAL opens shared WASAPI with automatic PCM conversion and does not expose exclusive output"
        }
        "ALSA" if device_id.starts_with("hw:") => {
            "the ALSA hardware name is only a direct-route candidate; CPAL cannot read effective precision, channel mapping or controls from the opened PCM handle"
        }
        "ALSA" => {
            "ALSA default, plug, mixer and unknown plugin routes are not eligible for strict output"
        }
        _ => "this CPAL backend has no verified strict-output route",
    }
}

#[derive(Clone, Copy)]
enum Transparency {
    Unknown,
    #[cfg(test)]
    Modifying,
    #[cfg(test)]
    DirectUnity,
}

struct Candidate {
    advertised: cpal::SupportedStreamConfigRange,
    layout: Option<ChannelLayout>,
    effective_bits: Option<u32>,
    actual_rate: Option<u32>,
    transparency: Transparency,
}

fn representation(format: SampleFormat) -> Option<ExactSampleRepresentation> {
    match format {
        SampleFormat::I16 => Some(ExactSampleRepresentation::Signed16Le),
        // This is the session's packed intermediate representation. CPAL I24
        // instead uses four bytes, with the signed value in the low 24 bits.
        SampleFormat::I24 => Some(ExactSampleRepresentation::PackedSigned24Le),
        SampleFormat::I32 => Some(ExactSampleRepresentation::Signed32Le),
        SampleFormat::F32 => Some(ExactSampleRepresentation::Float32Le),
        SampleFormat::F64 => Some(ExactSampleRepresentation::Float64Le),
        _ => None,
    }
}

fn representation_rank(format: SampleFormat, source_bits: u32) -> u8 {
    match (format, source_bits) {
        (SampleFormat::I16, 16) | (SampleFormat::I24, 24) => 0,
        (SampleFormat::I24, _) => 1,
        (SampleFormat::I32, _) => 2,
        (SampleFormat::F32, _) => 3,
        (SampleFormat::F64, _) => 4,
        _ => u8::MAX,
    }
}

fn plan_candidates(
    source: IntegerPcmFormat,
    candidates: &[Candidate],
) -> Result<NativePlan, String> {
    let mut eligible = None;
    let mut refusal =
        "device has no supported exact-rate, exact-layout PCM representation".to_owned();
    for candidate in candidates {
        let Some(representation) = representation(candidate.advertised.sample_format()) else {
            continue;
        };
        if candidate.advertised.channels() != source.channels() {
            continue;
        }
        let Some(config) = candidate
            .advertised
            .try_with_sample_rate(source.sample_rate())
        else {
            continue;
        };
        let result = admit_candidate(source, candidate, representation);
        match result {
            Ok(output) => {
                let rank = representation_rank(config.sample_format(), source.bits_per_sample());
                if eligible
                    .as_ref()
                    .is_none_or(|(previous, _)| rank < *previous)
                {
                    eligible = Some((rank, NativePlan { config, output }));
                }
            }
            Err(error) => refusal = error,
        }
    }
    eligible.map(|(_, plan)| plan).ok_or(refusal)
}

fn admit_candidate(
    source: IntegerPcmFormat,
    candidate: &Candidate,
    representation: ExactSampleRepresentation,
) -> Result<ExactOutputFormat, String> {
    let layout = candidate
        .layout
        .ok_or("native output channel mapping is unknown")?;
    let rate = candidate
        .actual_rate
        .ok_or("native output effective sample rate is unknown")?;
    let output = ExactOutputFormat::new(rate, layout, representation, candidate.effective_bits)
        .map_err(|error| error.to_string())?;
    ExactPcmAdapter::new(source, output).map_err(|error| error.to_string())?;
    match candidate.transparency {
        #[cfg(test)]
        Transparency::DirectUnity => Ok(output),
        #[cfg(test)]
        Transparency::Modifying => Err("native route mixes or modifies source samples".to_owned()),
        Transparency::Unknown => {
            Err("native route transparency and unity system gain are unknown".to_owned())
        }
    }
}

#[cfg(test)]
#[path = "play_exact_route_tests.rs"]
mod tests;
