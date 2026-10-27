//! Honour AAC packet crop fields without replacing the existing AAC decoder.
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Signal, SignalSpec},
    codecs::{CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult},
    errors::{Error, Result},
    formats::Packet,
};

pub(super) struct TrimDecoder {
    inner: Box<dyn Decoder>,
    output: AudioBuffer<f32>,
    cropped: bool,
    failed: bool,
    verify_packet_lengths: bool,
}
impl TrimDecoder {
    pub(super) fn new(inner: Box<dyn Decoder>) -> Self {
        let spec = SignalSpec::new(48000, symphonia::core::audio::Channels::FRONT_LEFT);
        let mut output = AudioBuffer::new(1, spec);
        output.clear();
        let params = inner.codec_params();
        let verify_packet_lengths = params.delay.is_some()
            && params.padding.is_some()
            && params
                .time_base
                .zip(params.sample_rate)
                .is_some_and(|(t, r)| t.numer == 1 && t.denom == r);
        Self {
            inner,
            output,
            cropped: false,
            failed: false,
            verify_packet_lengths,
        }
    }
}
impl Decoder for TrimDecoder {
    fn try_new(_: &CodecParameters, _: &DecoderOptions) -> Result<Self> {
        Err(Error::Unsupported("use the shared codec factory"))
    }
    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[]
    }
    fn codec_params(&self) -> &CodecParameters {
        self.inner.codec_params()
    }
    fn reset(&mut self) {
        if !self.failed {
            self.inner.reset();
            self.output.clear();
            self.cropped = false;
        }
    }
    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        if self.failed {
            return Err(Error::ResetRequired);
        }
        self.output.clear();
        self.cropped = true;
        let decoded = match self.inner.decode(packet) {
            Ok(value) => value,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        if self.verify_packet_lengths
            && packet
                .dur()
                .checked_add(u64::from(packet.trim_start))
                .and_then(|n| n.checked_add(u64::from(packet.trim_end)))
                != Some(decoded.frames() as u64)
        {
            self.failed = true;
            return Err(Error::DecodeError(
                "AAC PCM length disagrees with the packet edit",
            ));
        }
        let start = packet.trim_start as usize;
        let end = decoded
            .frames()
            .checked_sub(packet.trim_end as usize)
            .filter(|end| *end >= start)
            .ok_or(Error::DecodeError("invalid AAC PCM cropping"));
        let end = match end {
            Ok(end) => end,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        if start == 0 && packet.trim_end == 0 {
            self.cropped = false;
            return Ok(decoded);
        }
        let AudioBufferRef::F32(source) = decoded else {
            self.failed = true;
            return Err(Error::Unsupported("AAC crop requires floating PCM"));
        };
        let spec = *source.spec();
        let count = end - start;
        if spec.channels.count() == 0 || count > 1_048_576 / spec.channels.count() {
            self.failed = true;
            return Err(Error::DecodeError("AAC PCM crop exceeds its budget"));
        }
        if self.output.spec() != &spec || self.output.capacity() < count {
            self.output = AudioBuffer::new(count.max(1) as u64, spec);
        }
        self.output.render_reserved(Some(count));
        for channel in 0..spec.channels.count() {
            let samples = source
                .chan(channel)
                .get(start..end)
                .ok_or(Error::DecodeError("AAC crop exceeds its PCM source"))?;
            self.output.chan_mut(channel).copy_from_slice(samples);
        }
        Ok(self.output.as_audio_buffer_ref())
    }
    fn finalize(&mut self) -> FinalizeResult {
        if self.failed {
            FinalizeResult {
                verify_ok: Some(false),
            }
        } else {
            self.inner.finalize()
        }
    }
    fn last_decoded(&self) -> AudioBufferRef<'_> {
        if self.cropped || self.failed {
            self.output.as_audio_buffer_ref()
        } else {
            self.inner.last_decoded()
        }
    }
}
