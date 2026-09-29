//! Explicit speaker positions and conservative multichannel downmix.

use crate::DspError;

/// Speaker positions in the interleaved order defined by the channel mask.
///
/// The low bits follow WAVEFORMATEXTENSIBLE and Symphonia. An unknown layout
/// retains its channel count but cannot be downmixed by guessing positions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelLayout {
    Unknown(u16),
    Mask(u32),
}

impl ChannelLayout {
    pub const MONO: Self = Self::Mask(0x1);
    pub const STEREO: Self = Self::Mask(0x3);

    pub fn from_mask(mask: u32) -> Result<Self, DspError> {
        if mask == 0 {
            return Err(DspError::InvalidLayout);
        }
        Ok(Self::Mask(mask))
    }

    pub fn channels(self) -> u16 {
        match self {
            Self::Unknown(channels) => channels,
            Self::Mask(mask) => mask.count_ones() as u16,
        }
    }

    pub fn mask(self) -> Option<u32> {
        match self {
            Self::Unknown(_) => None,
            Self::Mask(mask) => Some(mask),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Mask(0x1) => "mono",
            Self::Mask(0x3) => "stereo (FL FR)",
            Self::Mask(0xb) => "2.1 (FL FR LFE)",
            Self::Mask(0x7) => "3.0 (FL FR FC)",
            Self::Mask(0xf) => "3.1 (FL FR FC LFE)",
            Self::Mask(0x33) => "quad (FL FR BL BR)",
            Self::Mask(0x603) => "quad side (FL FR SL SR)",
            Self::Mask(0x107) => "4.0 (FL FR FC BC)",
            Self::Mask(0x37) => "5.0 (FL FR FC BL BR)",
            Self::Mask(0x3f) => "5.1 (FL FR FC LFE BL BR)",
            Self::Mask(0x607) => "5.0 side (FL FR FC SL SR)",
            Self::Mask(0x60f) => "5.1 side (FL FR FC LFE SL SR)",
            Self::Mask(0x637) => "7.0 (FL FR FC BL BR SL SR)",
            Self::Mask(0x63f) => "7.1 (FL FR FC LFE BL BR SL SR)",
            _ => "unrecognized channel layout",
        }
    }

    /// Explicit FFmpeg raw-PCM layout name; unknown masks must not be guessed.
    pub fn ffmpeg_name(self) -> Option<&'static str> {
        match self {
            Self::Mask(0x1) => Some("mono"),
            Self::Mask(0x3) => Some("stereo"),
            Self::Mask(0xb) => Some("2.1"),
            Self::Mask(0x7) => Some("3.0"),
            Self::Mask(0xf) => Some("3.1"),
            Self::Mask(0x33) => Some("quad"),
            Self::Mask(0x603) => Some("quad(side)"),
            Self::Mask(0x107) => Some("4.0"),
            Self::Mask(0x37) => Some("5.0"),
            Self::Mask(0x3f) => Some("5.1"),
            Self::Mask(0x607) => Some("5.0(side)"),
            Self::Mask(0x60f) => Some("5.1(side)"),
            Self::Mask(0x637) => Some("7.0"),
            Self::Mask(0x63f) => Some("7.1"),
            _ => None,
        }
    }
}

/// A stateless Lo/Ro stereo fold-down for known speaker layouts.
///
/// The center and one surround pair are mixed at -3 dB into their sides,
/// following ITU-R BS.775's conventional downmix. LFE is omitted. When both
/// side and rear pairs are present, each gets -6 dB. The matrix is then
/// divided by its largest absolute row sum to prevent coherent full-scale
/// channels from exceeding full scale. This may make isolated channels quiet.
#[derive(Clone, Copy, Debug)]
pub struct StereoDownmixer {
    channels: usize,
    left: [f64; 8],
    right: [f64; 8],
}

impl StereoDownmixer {
    pub fn new(layout: ChannelLayout) -> Result<Self, DspError> {
        let mask = layout.mask().ok_or(DspError::InvalidLayout)?;
        if !matches!(
            mask,
            0xb | 0x7 | 0xf | 0x33 | 0x603 | 0x107 | 0x37 | 0x3f | 0x607 | 0x60f | 0x637 | 0x63f
        ) {
            return Err(DspError::InvalidLayout);
        }
        let mut left = [0.0; 8];
        let mut right = [0.0; 8];
        let both_surround_pairs = mask & 0x630 == 0x630;
        for (index, bit) in (0..32).filter(|bit| mask & (1 << bit) != 0).enumerate() {
            let surround = if both_surround_pairs {
                0.5
            } else {
                std::f64::consts::FRAC_1_SQRT_2
            };
            let (l, r) = match bit {
                0 => (1.0, 0.0), // front left
                1 => (0.0, 1.0), // front right
                2 => (
                    std::f64::consts::FRAC_1_SQRT_2,
                    std::f64::consts::FRAC_1_SQRT_2,
                ),
                3 => (0.0, 0.0), // LFE: no bass management in a stereo downmix
                4 | 9 => (surround, 0.0),
                5 | 10 => (0.0, surround),
                8 => (0.5, 0.5), // rear center
                _ => return Err(DspError::InvalidLayout),
            };
            left[index] = l;
            right[index] = r;
        }
        let row_sum = left.iter().sum::<f64>().max(right.iter().sum::<f64>());
        for weight in &mut left {
            *weight /= row_sum;
        }
        for weight in &mut right {
            *weight /= row_sum;
        }
        Ok(Self {
            channels: usize::from(layout.channels()),
            left,
            right,
        })
    }

    /// Write one stereo frame for each source frame, without allocation.
    pub fn process(&self, source: &[f32], stereo: &mut [f32]) -> Result<(), DspError> {
        if !source.len().is_multiple_of(self.channels)
            || stereo.len() != source.len() / self.channels * 2
        {
            return Err(DspError::IncompleteFrame);
        }
        if source.iter().any(|sample| !sample.is_finite()) {
            return Err(DspError::NonFiniteSample);
        }
        for (frame, out) in source
            .chunks_exact(self.channels)
            .zip(stereo.as_chunks_mut::<2>().0)
        {
            let mut left = 0.0;
            let mut right = 0.0;
            for (index, &sample) in frame.iter().enumerate() {
                left += f64::from(sample) * self.left[index];
                right += f64::from(sample) * self.right[index];
            }
            out[0] = left as f32;
            out[1] = right as f32;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
