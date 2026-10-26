//! RFC 7845 mapping-family-zero Opus, using the existing safe libopus binding.
use std::sync::Mutex;

use audiopus::{Channels as OpusChannels, SampleRate, coder::Decoder as NativeDecoder};
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Signal, SignalSpec},
    codecs::{
        CODEC_TYPE_OPUS, CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult,
    },
    errors::{Error, Result},
    formats::Packet,
};

const RATE: u32 = 48_000;
const MAX_FRAMES: usize = 5760; // Opus packets contain at most 120 ms.
const INVALID: Error = Error::DecodeError("invalid Opus packet or trimming");

pub(super) struct OpusDecoder {
    // libopus state is Send but not Sync. Only this control/decoder worker accesses it.
    native: Mutex<NativeDecoder>,
    params: CodecParameters,
    output: AudioBuffer<f32>,
    scratch: Vec<f32>,
    channels: usize,
    pre_skip: usize,
    skip: usize,
    expected: u64,
    decoded: u64,
    failed: bool,
    at_start: bool,
}
impl Decoder for OpusDecoder {
    fn try_new(params: &CodecParameters, _: &DecoderOptions) -> Result<Self> {
        let head = params
            .extra_data
            .as_deref()
            .ok_or(Error::Unsupported("missing OpusHead"))?;
        if params.codec != CODEC_TYPE_OPUS
            || head.len() < 19
            || head.len() > 65_025
            || &head[..8] != b"OpusHead"
            || head[8] == 0
            || head[8] > 15
            || head[18] != 0
            || ![1, 2].contains(&head[9])
        {
            return Err(Error::Unsupported(
                "Opus requires a mono/stereo family-zero header",
            ));
        }
        let channels = usize::from(head[9]);
        let mask = params
            .channels
            .ok_or(Error::Unsupported("missing Opus channels"))?;
        let valid_layout = match channels {
            1 => matches!(mask.bits(), 1 | 4),
            2 => mask.bits() == 3,
            _ => false,
        };
        if params.sample_rate != Some(RATE) || !valid_layout || params.start_ts != 0 {
            return Err(Error::Unsupported(
                "unsupported Opus rate, channels or initial timestamp",
            ));
        }
        let pre_skip = usize::from(u16::from_le_bytes([head[10], head[11]]));
        let expected = params
            .n_frames
            .and_then(|n| n.checked_sub(pre_skip as u64))
            .filter(|n| *n > 0)
            .ok_or(Error::Unsupported(
                "Opus requires a known complete frame count",
            ))?;
        let gain = i16::from_le_bytes([head[16], head[17]]);
        let native = NativeDecoder::new(
            SampleRate::Hz48000,
            if channels == 1 {
                OpusChannels::Mono
            } else {
                OpusChannels::Stereo
            },
        )
        .map_err(|_| Error::Unsupported("cannot initialize libopus"))?;
        native
            .set_gain(i32::from(gain))
            .map_err(|_| Error::Unsupported("invalid Opus gain"))?;
        Ok(Self {
            native: Mutex::new(native),
            params: params.clone(),
            output: AudioBuffer::new(MAX_FRAMES as u64, SignalSpec::new(RATE, mask)),
            scratch: vec![0.0; MAX_FRAMES * channels],
            channels,
            pre_skip,
            skip: pre_skip,
            expected,
            decoded: 0,
            failed: false,
            at_start: true,
        })
    }
    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[]
    }
    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }
    fn reset(&mut self) {
        use audiopus::coder::GenericCtl;
        self.output.clear();
        self.failed = self
            .native
            .get_mut()
            .map_or(true, |decoder| decoder.reset_state().is_err());
        self.skip = self.pre_skip;
        self.decoded = 0;
        self.at_start = true;
    }
    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        self.output.clear();
        if self.failed {
            return Err(INVALID);
        }
        if let Err(error) = self.decode_packet(packet) {
            self.failed = true;
            self.output.clear();
            return Err(error);
        }
        Ok(self.output.as_audio_buffer_ref())
    }
    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult {
            verify_ok: Some(!self.failed && self.decoded == self.expected),
        }
    }
    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.output.as_audio_buffer_ref()
    }
}
impl OpusDecoder {
    fn decode_packet(&mut self, packet: &Packet) -> Result<()> {
        // Callers seek by reopening and progressively decoding pre-roll. A cold
        // codec cannot safely reconstruct an arbitrary Ogg packet's history.
        if self.at_start && packet.ts() != 0 {
            return Err(Error::Unsupported(
                "Opus seeking requires reopening with progressive pre-roll",
            ));
        }
        self.at_start = false;
        // No packet-loss concealment for corrupt local-file input; never pass None to libopus.
        if packet.data.is_empty() || packet.data.len() > 61_440 {
            return Err(INVALID);
        }
        let native = self.native.get_mut().map_err(|_| INVALID)?;
        let frames = native
            .decode_float(
                Some(packet.data.as_ref()),
                self.scratch.as_mut_slice(),
                false,
            )
            .map_err(|_| INVALID)?;
        if frames == 0 || frames > MAX_FRAMES || self.decoded == self.expected {
            return Err(INVALID);
        }
        if packet.trim_start as usize > frames {
            return Err(INVALID);
        }
        let start = self.skip.max(packet.trim_start as usize);
        let end = frames
            .checked_sub(packet.trim_end as usize)
            .ok_or(INVALID)?;
        let skip = start.min(frames);
        self.skip = self.skip.saturating_sub(frames);
        if end < skip {
            return Err(INVALID);
        }
        // Symphonia 0.5.5 can describe a one-audio-page EOS crop in n_frames
        // without setting packet.trim_end. The declared granule remains authoritative.
        let count =
            (end - skip).min((self.expected - self.decoded).min(MAX_FRAMES as u64) as usize);
        self.decoded = self
            .decoded
            .checked_add(count as u64)
            .filter(|n| *n <= self.expected)
            .ok_or(INVALID)?;
        self.output.render_reserved(Some(count));
        for channel in 0..self.channels {
            for (i, sample) in self.output.chan_mut(channel).iter_mut().enumerate() {
                let value = self.scratch[(skip + i) * self.channels + channel];
                if !value.is_finite() {
                    return Err(INVALID);
                }
                *sample = value;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "decode_opus_tests.rs"]
mod tests;
