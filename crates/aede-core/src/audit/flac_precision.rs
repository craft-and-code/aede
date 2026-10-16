//! Confirms structural padding candidates from the already-read FLAC frames.
//!
//! Wasted-bit headers cannot establish the low bits after stereo
//! decorrelation, and a nonzero constant subframe can carry significant low
//! bits of its own. Reuse the existing integer codec for those candidates,
//! retaining only its single bounded output block.

use std::panic::{AssertUnwindSafe, catch_unwind};

use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_FLAC, CodecParameters, Decoder, DecoderOptions};
use symphonia::core::formats::Packet;

/// A lazy, file-local decoder. Once confirmation fails it cannot support a
/// padding claim, and the partially advanced codec is never resumed.
pub(super) struct PrecisionConfirmation {
    stream_info: Option<[u8; 34]>,
    decoder: Option<Box<dyn Decoder>>,
}

impl PrecisionConfirmation {
    pub(super) fn new(stream_info: Option<[u8; 34]>) -> Self {
        Self {
            stream_info,
            decoder: None,
        }
    }

    /// Returns the low zero bits shared by every decoded sample in this
    /// frame. `None` means that no padding claim can be confirmed.
    pub(super) fn confirm(&mut self, frame: &[u8], block_size: u32) -> Option<u32> {
        // The pinned codec has unchecked arithmetic/assertions on malformed
        // input, as in playback. A failed decoder must not be reused.
        let result = catch_unwind(AssertUnwindSafe(|| self.confirm_inner(frame, block_size)));
        match result {
            Ok(Some(wasted)) => Some(wasted),
            Ok(None) | Err(_) => {
                self.stream_info = None;
                self.decoder = None;
                None
            }
        }
    }

    fn confirm_inner(&mut self, frame: &[u8], block_size: u32) -> Option<u32> {
        let stream_info = self.stream_info.as_ref()?;
        let max_block = u32::from(u16::from_be_bytes(stream_info.get(2..4)?.try_into().ok()?));
        let packed = u64::from_be_bytes(stream_info.get(10..18)?.try_into().ok()?);
        let channels = (((packed >> 41) & 7) + 1) as usize;
        let bits_per_sample = (((packed >> 36) & 31) + 1) as u32;
        if max_block == 0
            || block_size == 0
            || block_size > max_block
            || !(4..=32).contains(&bits_per_sample)
        {
            return None;
        }

        let header = *frame.get(3)?;
        let frame_bits = match (header >> 1) & 7 {
            0 => bits_per_sample,
            1 => 8,
            2 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            7 => 32,
            _ => return None,
        };
        let assignment = header >> 4;
        let frame_channels = match assignment {
            0..=7 => usize::from(assignment) + 1,
            8..=10 => 2,
            _ => return None,
        };
        if frame_bits != bits_per_sample
            || frame_channels != channels
            // The pinned codec cannot represent the 33-bit difference channel.
            || (frame_bits == 32 && assignment >= 8)
        {
            return None;
        }

        if self.decoder.is_none() {
            let mut params = CodecParameters::new();
            params
                .for_codec(CODEC_TYPE_FLAC)
                .with_extra_data(stream_info.to_vec().into_boxed_slice());
            // STREAMINFO bounds this codec's PCM allocation to at most
            // 65,535 frames × eight channels × one i32, below 2 MiB.
            self.decoder = Some(
                symphonia::default::get_codecs()
                    .make(&params, &DecoderOptions { verify: false })
                    .ok()?,
            );
        }
        let packet = Packet::new_from_slice(0, 0, 0, frame);
        let decoded = self.decoder.as_mut()?.decode(&packet).ok()?;
        let AudioBufferRef::S32(pcm) = decoded else {
            return None;
        };
        if pcm.frames() != block_size as usize || pcm.spec().channels.count() != channels {
            return None;
        }

        // Symphonia's FLAC codec left-aligns its exact integer output in i32.
        // Exclude that container shift when counting source-sample zero bits.
        let container_shift = 32 - bits_per_sample;
        let mut wasted = bits_per_sample;
        for channel in 0..channels {
            for sample in pcm.chan(channel) {
                wasted = wasted.min(
                    (*sample as u32)
                        .trailing_zeros()
                        .saturating_sub(container_shift),
                );
            }
        }
        Some(wasted)
    }
}

#[cfg(test)]
#[path = "flac_precision_tests.rs"]
mod tests;
