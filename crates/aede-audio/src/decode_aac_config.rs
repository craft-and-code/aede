//! Bounded AAC-LC AudioSpecificConfig admission (ISO/IEC 14496-3).
use symphonia::core::{
    audio::Channels,
    codecs::{CODEC_TYPE_AAC, CodecParameters},
    errors::{Error, Result},
};

pub(super) const FRAMES: u64 = 1024;
const UNSUPPORTED: Error =
    Error::Unsupported("native AAC requires AAC-LC mono/stereo without SBR/PS");

/// Properties of the admitted AAC-LC AudioSpecificConfig, not container timing.
#[derive(Debug, Clone, Copy)]
pub struct AacLcConfiguration {
    /// Actual decoded PCM frequency, independent of the MP4 sample entry.
    pub sample_rate: u32,
    /// Mono or stereo channel count.
    pub channels: u8,
}
/// Inspect a known AAC-LC configuration without admitting its container/edit.
/// Other profiles remain unknown rather than guessing an HE-AAC output format.
pub fn read_aac_lc_configuration(data: &[u8]) -> Option<AacLcConfiguration> {
    parse(data).ok()
}

pub(super) fn parameters(input: &CodecParameters) -> Result<CodecParameters> {
    let mut result = input.clone();
    if input.codec != CODEC_TYPE_AAC {
        return Ok(result);
    }
    let Some(data) = input.extra_data.as_deref() else {
        // ADTS already supplies its encoded stream properties to the decoder.
        return Ok(result);
    };
    let config = parse(data)?;
    result.sample_rate = Some(config.sample_rate);
    result.channels = Some(if config.channels == 1 {
        Channels::FRONT_LEFT
    } else {
        Channels::FRONT_LEFT | Channels::FRONT_RIGHT
    });
    result.bits_per_sample = None;
    result.max_frames_per_packet = Some(FRAMES);
    Ok(result)
}

fn parse(data: &[u8]) -> Result<AacLcConfiguration> {
    if data.len() > 64 {
        return Err(UNSUPPORTED);
    }
    let mut bits = Bits { data, at: 0 };
    if bits.read(5)? != 2 {
        return Err(UNSUPPORTED);
    }
    let index = bits.read(4)? as usize;
    const RATES: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
    let rate = if index == 15 {
        bits.read(24)?
    } else {
        *RATES.get(index).ok_or(UNSUPPORTED)?
    };
    if !(8000..=96000).contains(&rate) {
        return Err(UNSUPPORTED);
    }
    let channels = match bits.read(4)? {
        1 => 1,
        2 => 2,
        _ => return Err(UNSUPPORTED),
    };
    // 1024-frame GA profile, no core-coder dependency or extension flag.
    if bits.read(3)? != 0 {
        return Err(UNSUPPORTED);
    }
    if bits.remaining_nonzero() {
        // Accept the reference encoder's explicit SBR-disabled sync extension.
        if bits.read(11)? != 0x2b7
            || bits.read(5)? != 5
            || bits.read(1)? != 0
            || bits.remaining_nonzero()
        {
            return Err(UNSUPPORTED);
        }
    }
    Ok(AacLcConfiguration {
        sample_rate: rate,
        channels,
    })
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}
impl Bits<'_> {
    fn read(&mut self, count: usize) -> Result<u32> {
        let mut value = 0;
        for _ in 0..count {
            let byte = self
                .data
                .get(self.at / 8)
                .ok_or(Error::DecodeError("truncated AAC configuration"))?;
            value = (value << 1) | u32::from((byte >> (7 - self.at % 8)) & 1);
            self.at += 1;
        }
        Ok(value)
    }
    fn remaining_nonzero(&self) -> bool {
        (self.at..self.data.len() * 8).any(|at| self.data[at / 8] & (1 << (7 - at % 8)) != 0)
    }
}

#[cfg(test)]
#[path = "decode_aac_config_tests.rs"]
mod tests;
