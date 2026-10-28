//! ALAC codec configuration, independent of incomplete MP4 sample-entry hints.
//! Fields follow Apple's public ALACMagicCookieDescription.txt, not decoder code.
use symphonia::core::{
    audio::Channels,
    codecs::{CODEC_TYPE_ALAC, CodecParameters},
    errors::{Error, Result},
    formats::Packet,
};

/// ALAC cookie properties; inspecting them does not admit native decoding.
#[derive(Debug, Clone, Copy)]
pub struct AlacConfiguration {
    /// Default decoded frames per packet.
    pub frame_length: u32,
    /// Codec configuration version.
    pub compatible_version: u8,
    /// Original integer PCM precision.
    pub bit_depth: u8,
    /// Channel count, without inventing a multichannel speaker order.
    pub channels: u8,
    /// Optional average encoded bits per second; zero means unknown.
    pub average_bitrate: Option<u32>,
    /// Optional actual PCM clock, independent of MP4's sample-entry hint.
    pub sample_rate: Option<u32>,
}

/// Inspect an unwrapped ALAC cookie without constructing a codec or allocating.
/// Partial metadata may expose precision/count without a clock; callers decide
/// which complete configuration/layout/version is admitted for decoding.
pub fn read_alac_configuration(cookie: &[u8]) -> Option<AlacConfiguration> {
    let word = |at| {
        let bytes = cookie.get(at..at + 4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    Some(AlacConfiguration {
        frame_length: word(0)?,
        compatible_version: *cookie.get(4)?,
        bit_depth: *cookie.get(5)?,
        channels: *cookie.get(9)?,
        average_bitrate: word(16),
        sample_rate: word(20),
    })
}

/// Exact decoded-frame length when the ALAC media clock resolves whole PCM frames.
/// A demuxer's ALAC length may be expressed in media ticks, not sample frames.
/// Coarse/rounded clocks or fractional sample boundaries provide no exact count.
/// Without an ALAC timebase, and for other codecs, retain the declared count.
pub fn declared_pcm_frames(input: &CodecParameters) -> Result<Option<u64>> {
    let Some(frames) = input.n_frames else {
        return Ok(None);
    };
    if input.codec != CODEC_TYPE_ALAC {
        return Ok(Some(frames));
    }
    let Some(clock) = input.time_base else {
        return Ok(Some(frames));
    };
    let rate = input
        .sample_rate
        .filter(|n| *n > 0)
        .ok_or(Error::DecodeError("missing ALAC PCM clock"))?;
    if clock.numer == 0 || clock.denom == 0 {
        return Err(Error::DecodeError("invalid ALAC media clock"));
    }
    let frame_clock = u64::from(clock.numer) * u64::from(rate);
    let denom = u64::from(clock.denom);
    if !denom.is_multiple_of(frame_clock) {
        return Ok(None);
    }
    let ticks_per_frame = denom / frame_clock;
    if !frames.is_multiple_of(ticks_per_frame) {
        return Ok(None);
    }
    Ok(Some(frames / ticks_per_frame))
}

/// Packet coverage stays exact in container ticks even with a rounded PCM clock.
pub(super) struct PacketTimeline {
    start: u64,
    next: u64,
    end: u64,
    // Common integer units per PCM frame and per media tick, respectively.
    pcm_clock: Option<(u128, u128)>,
}

impl PacketTimeline {
    pub(super) fn from_parameters(input: &CodecParameters) -> Result<Option<Self>> {
        if input.codec != CODEC_TYPE_ALAC {
            return Ok(None);
        }
        let Some(duration) = input.n_frames else {
            return Ok(None);
        };
        let end = input
            .start_ts
            .checked_add(duration)
            .ok_or(Error::DecodeError("ALAC media extent overflow"))?;
        let pcm_clock = match (input.time_base, input.sample_rate) {
            (Some(clock), Some(rate)) => {
                if clock.numer == 0 || clock.denom == 0 || rate == 0 {
                    return Err(Error::DecodeError("invalid ALAC packet clock"));
                }
                Some((
                    u128::from(clock.denom),
                    u128::from(clock.numer) * u128::from(rate),
                ))
            }
            _ => None,
        };
        Ok(Some(Self {
            start: input.start_ts,
            next: input.start_ts,
            end,
            pcm_clock,
        }))
    }

    pub(super) fn admit(&mut self, packet: &Packet) -> Result<()> {
        let end = packet
            .ts()
            .checked_add(packet.dur())
            .ok_or(Error::DecodeError("ALAC packet extent overflow"))?;
        if packet.ts() != self.next || end > self.end || packet.dur() == 0 {
            return Err(Error::DecodeError("inconsistent ALAC packet timeline"));
        }
        self.next = end;
        Ok(())
    }

    pub(super) fn check_pcm_duration(&self, packet: &Packet, frames: u64) -> Result<()> {
        let Some((frame_units, tick_units)) = self.pcm_clock else {
            return Ok(());
        };
        // u64 frame/tick counts multiplied by two u32 clock factors fit u128.
        let actual = u128::from(frames) * frame_units;
        let declared = u128::from(packet.dur()) * tick_units;
        // An exact sample clock needs no rounding. Otherwise retain one media
        // tick of quantization, without making every rounded packet an exact
        // PCM length or accepting an arbitrary short decoded packet.
        let tolerance =
            if frame_units.is_multiple_of(tick_units) && declared.is_multiple_of(frame_units) {
                0
            } else {
                tick_units
            };
        if actual.abs_diff(declared) > tolerance {
            return Err(Error::DecodeError(
                "ALAC packet PCM length disagrees with its media duration",
            ));
        }
        Ok(())
    }

    pub(super) fn complete(&self) -> bool {
        self.next == self.end
    }

    pub(super) fn reset(&mut self) {
        self.next = self.start;
    }
}

pub(super) fn parameters(input: &CodecParameters) -> Result<CodecParameters> {
    let mut output = input.clone();
    if input.codec != CODEC_TYPE_ALAC || input.channels.is_some() {
        return Ok(output);
    }
    let cookie = input
        .extra_data
        .as_deref()
        .ok_or(Error::DecodeError("missing ALAC configuration"))?;
    if !matches!(cookie.len(), 24 | 48) {
        return Err(Error::Unsupported(
            "ALAC requires the standard configuration cookie",
        ));
    }
    let config =
        read_alac_configuration(cookie).ok_or(Error::DecodeError("invalid ALAC configuration"))?;
    let rate = config
        .sample_rate
        .ok_or(Error::DecodeError("missing ALAC clock"))?;
    let frames = config.frame_length;
    let channels = config.channels;
    let bits = config.bit_depth;
    if config.compatible_version != 0
        || !matches!(bits, 16 | 20 | 24 | 32)
        || rate == 0
        || frames == 0
    {
        return Err(Error::DecodeError("invalid ALAC configuration"));
    }
    // Apple's optional layout is mandatory above stereo: a channel count alone
    // describes discrete channels, not the standard speaker ordering. The
    // pinned codec emits these named layouts in ascending speaker-mask order.
    let (mask, tag) = match channels {
        1 => (0x004, 100),
        2 => (0x003, 101),
        3 if cookie.len() == 48 => (0x007, 113),
        4 if cookie.len() == 48 => (0x107, 116),
        5 if cookie.len() == 48 => (0x607, 120),
        6 if cookie.len() == 48 => (0x60f, 124),
        7 if cookie.len() == 48 => (0x70f, 142),
        8 if cookie.len() == 48 => (0x6cf, 127),
        _ => return Err(Error::Unsupported("ALAC requires a known speaker layout")),
    };
    let layout = Channels::from_bits_truncate(mask);
    if frames > 1_048_576 / u32::from(channels) {
        return Err(Error::Unsupported("ALAC packet exceeds PCM budget"));
    }
    if cookie.len() == 48 {
        let word =
            |at| u32::from_be_bytes([cookie[at], cookie[at + 1], cookie[at + 2], cookie[at + 3]]);
        let tag = (tag << 16) | u32::from(channels);
        if word(24) != 24
            || &cookie[28..32] != b"chan"
            || word(32) != 0
            || word(36) != tag
            || word(40) != 0
            || word(44) != 0
        {
            return Err(Error::Unsupported("unsupported ALAC channel layout"));
        }
    }
    // MP4 v0 sample entries can carry a 48 kHz placeholder for high-rate ALAC.
    // The codec cookie, retained unchanged, declares the actual PCM properties.
    output.sample_rate = Some(rate);
    output.channels = Some(layout);
    output.bits_per_sample = Some(u32::from(bits));
    Ok(output)
}

#[cfg(test)]
#[path = "decode_alac_tests.rs"]
mod tests;
