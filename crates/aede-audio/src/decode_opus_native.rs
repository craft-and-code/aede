//! Owned libopus decoding. Family-one speaker order follows RFC 7845 §5.1.1.2.
//! The multistream FFI is explicitly approved; no separate libopus is linked.
use std::ptr::NonNull;

use audiopus::{Channels, SampleRate, coder::Decoder as Single};
use symphonia::core::errors::{Error, Result};

use super::{INVALID, MAX_FRAMES};

pub(super) enum Native {
    Single(Single),
    Multi(Multistream),
}

impl Native {
    pub(super) fn new(head: &[u8], mask: u32) -> Result<Self> {
        if head.len() < 19 {
            return Err(INVALID);
        }
        let channels = usize::from(head[9]);
        match head[18] {
            0 if matches!((channels, mask), (1, 1 | 4) | (2, 3)) => {
                let channels = if channels == 1 {
                    Channels::Mono
                } else {
                    Channels::Stereo
                };
                Single::new(SampleRate::Hz48000, channels)
                    .map(Self::Single)
                    .map_err(|_| INVALID)
            }
            1 => Multistream::new(head, mask).map(Self::Multi),
            _ => Err(Error::Unsupported(
                "unsupported Opus channel mapping or layout",
            )),
        }
    }

    pub(super) fn set_gain(&mut self, gain: i16) -> Result<()> {
        match self {
            Self::Single(native) => native.set_gain(i32::from(gain)).map_err(|_| INVALID),
            Self::Multi(native) => {
                // SAFETY: the owned live state is exclusively borrowed; this
                // libopus CTL takes exactly one promoted int (Q8 dB gain).
                let code = unsafe {
                    audiopus_sys::opus_multistream_decoder_ctl(
                        native.state.as_ptr(),
                        audiopus_sys::OPUS_SET_GAIN_REQUEST,
                        i32::from(gain),
                    )
                };
                success(code)
            }
        }
    }

    pub(super) fn reset(&mut self) -> Result<()> {
        use audiopus::coder::GenericCtl;
        match self {
            Self::Single(native) => native.reset_state().map_err(|_| INVALID),
            Self::Multi(native) => {
                // SAFETY: exclusive live state; OPUS_RESET_STATE has no varargs.
                success(unsafe {
                    audiopus_sys::opus_multistream_decoder_ctl(
                        native.state.as_ptr(),
                        audiopus_sys::OPUS_RESET_STATE,
                    )
                })
            }
        }
    }

    pub(super) fn decode(&mut self, packet: &[u8], pcm: &mut [f32]) -> Result<usize> {
        match self {
            Self::Single(native) if !packet.is_empty() && packet.len() <= 61_440 => native
                .decode_float(Some(packet), pcm, false)
                .map_err(|_| INVALID),
            Self::Single(_) => Err(INVALID),
            Self::Multi(native) => native.decode(packet, pcm),
        }
    }
}

pub(super) struct Multistream {
    state: NonNull<audiopus_sys::OpusMSDecoder>,
    channels: usize,
    packet_budget: usize,
}

// SAFETY: libopus owns no thread-affine resources. Ownership can move between
// decoder workers; calls require &mut self and the outer codec uses a Mutex.
// The raw state is never shared or exposed. This type is deliberately not Sync.
unsafe impl Send for Multistream {}

impl Multistream {
    fn new(head: &[u8], mask: u32) -> Result<Self> {
        let channels = usize::from(head[9]);
        // Canonical speaker-mask order -> RFC 7845 Vorbis output order.
        let (expected, order): (u32, &[usize]) = match channels {
            1 => (mask, &[0]),
            2 => (0x003, &[0, 1]),
            3 => (0x007, &[0, 2, 1]),
            4 => (0x033, &[0, 1, 2, 3]),
            5 => (0x037, &[0, 2, 1, 3, 4]),
            6 => (0x03f, &[0, 2, 1, 5, 3, 4]),
            7 => (0x70f, &[0, 2, 1, 6, 5, 3, 4]),
            8 => (0x63f, &[0, 2, 1, 7, 5, 6, 3, 4]),
            _ => return Err(INVALID),
        };
        if mask != expected || (channels == 1 && !matches!(mask, 1 | 4)) {
            return Err(INVALID);
        }
        let table = head.get(19..21 + channels).ok_or(INVALID)?;
        let streams = u16::from(table[0]);
        let coupled = u16::from(table[1]);
        let decoded = streams + coupled;
        if streams == 0
            || coupled > streams
            || decoded > 255
            || table[2..]
                .iter()
                .any(|&index| index != 255 && u16::from(index) >= decoded)
        {
            return Err(INVALID);
        }
        let mapping: Vec<u8> = order.iter().map(|&index| table[2 + index]).collect();
        let mut error = 0;
        // SAFETY: 1..=8 output channels, validated stream counts and exactly C
        // initialized mapping bytes live for this call. libopus copies mapping
        // into the returned allocation and writes one int to `error`.
        let raw = unsafe {
            audiopus_sys::opus_multistream_decoder_create(
                48_000,
                channels as i32,
                i32::from(streams),
                i32::from(coupled),
                mapping.as_ptr(),
                &mut error,
            )
        };
        let state = NonNull::new(raw).ok_or(INVALID)?;
        let native = Self {
            state,
            channels,
            packet_budget: 61_440 * usize::from(streams),
        };
        success(error)?;
        Ok(native)
    }

    fn decode(&mut self, packet: &[u8], pcm: &mut [f32]) -> Result<usize> {
        if packet.is_empty()
            || packet.len() > self.packet_budget
            || pcm.len() != MAX_FRAMES * self.channels
        {
            return Err(INVALID);
        }
        let length = i32::try_from(packet.len()).map_err(|_| INVALID)?;
        // SAFETY: exclusive live state; packet contains `length` bytes and is
        // nonempty (no concealment). PCM has MAX_FRAMES*C writable floats.
        // libopus writes at most frame_size*C values and retains neither slice.
        let frames = unsafe {
            audiopus_sys::opus_multistream_decode_float(
                self.state.as_ptr(),
                packet.as_ptr(),
                length,
                pcm.as_mut_ptr(),
                MAX_FRAMES as i32,
                0,
            )
        };
        usize::try_from(frames)
            .ok()
            .filter(|&n| n > 0 && n <= MAX_FRAMES)
            .ok_or(INVALID)
    }
}

impl Drop for Multistream {
    fn drop(&mut self) {
        // SAFETY: this non-null state came from create, has one owner, and is
        // freed exactly once after the last exclusive codec call has ended.
        unsafe { audiopus_sys::opus_multistream_decoder_destroy(self.state.as_ptr()) };
    }
}

fn success(code: i32) -> Result<()> {
    if code == 0 { Ok(()) } else { Err(INVALID) }
}
