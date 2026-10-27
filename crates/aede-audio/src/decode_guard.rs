//! Guard native codec state and ALAC packet coverage without copying PCM.
use std::panic::{AssertUnwindSafe, catch_unwind};
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec},
    codecs::{CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult},
    errors::{Error, Result},
    formats::Packet,
};

pub(super) struct GuardedDecoder {
    inner: Box<dyn Decoder>,
    failed: bool,
    empty: AudioBuffer<f32>,
    alac_timeline: Option<super::alac::PacketTimeline>,
}

impl GuardedDecoder {
    pub(super) fn new(inner: Box<dyn Decoder>) -> Result<Self> {
        let params = inner.codec_params();
        let alac_timeline = super::alac::PacketTimeline::from_parameters(params)?;
        let rate = params.sample_rate.filter(|n| *n > 0).unwrap_or(48000);
        let channels = params
            .channels
            .filter(|c| c.count() > 0)
            .unwrap_or(Channels::FRONT_LEFT);
        let mut empty = AudioBuffer::new(1, SignalSpec::new(rate, channels));
        empty.clear();
        Ok(Self {
            inner,
            failed: false,
            empty,
            alac_timeline,
        })
    }
}

impl Decoder for GuardedDecoder {
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
        if self.failed {
            return;
        }
        if catch_unwind(AssertUnwindSafe(|| self.inner.reset())).is_err() {
            self.failed = true;
        } else if let Some(timeline) = &mut self.alac_timeline {
            timeline.reset();
        }
    }
    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        if self.failed {
            return Err(Error::ResetRequired);
        }
        if let Some(timeline) = &mut self.alac_timeline
            && let Err(error) = timeline.admit(packet)
        {
            self.failed = true;
            return Err(error);
        }
        let failed = &mut self.failed;
        let timeline = &self.alac_timeline;
        let alac = self.alac_timeline.is_some();
        let inner = self.inner.as_mut();
        match catch_unwind(AssertUnwindSafe(move || {
            let active = inner;
            active.decode(packet)
        })) {
            Ok(Ok(decoded)) => {
                if let Some(timeline) = timeline
                    && let Err(error) = timeline.check_pcm_duration(packet, decoded.frames() as u64)
                {
                    *failed = true;
                    return Err(error);
                }
                Ok(decoded)
            }
            Ok(Err(error)) if alac => {
                *failed = true;
                Err(error)
            }
            Ok(result) => result,
            Err(_) => {
                *failed = true;
                Err(Error::ResetRequired)
            }
        }
    }
    fn finalize(&mut self) -> FinalizeResult {
        if self.failed || self.alac_timeline.as_ref().is_some_and(|t| !t.complete()) {
            self.failed = true;
            return FinalizeResult {
                verify_ok: Some(false),
            };
        }
        match catch_unwind(AssertUnwindSafe(|| self.inner.finalize())) {
            Ok(mut result) => {
                if self.alac_timeline.is_some() && result.verify_ok.is_none() {
                    result.verify_ok = Some(true);
                }
                result
            }
            Err(_) => {
                self.failed = true;
                FinalizeResult {
                    verify_ok: Some(false),
                }
            }
        }
    }
    fn last_decoded(&self) -> AudioBufferRef<'_> {
        if self.failed {
            self.empty.as_audio_buffer_ref()
        } else {
            self.inner.last_decoded()
        }
    }
}
