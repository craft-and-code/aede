//! Bounded background decode and existing aede-dsp format adaptation.
use super::*;
use crate::{
    dsp::{ChannelLayout, PcmFormat, RateConverter, StereoDownmixer},
    queue::PcmProducer,
};
/// Adapt only channels and rate required by the device, without loudness/EQ.
pub(super) struct Converter {
    input_channels: usize,
    output_channels: usize,
    downmix: Option<StereoDownmixer>,
    rate: Option<RateConverter>,
    scratch: Vec<f32>,
}
impl Converter {
    pub fn new(decoder: &PcmStreamDecoder, rate: u32, channels: u16) -> Result<Self, String> {
        let downmix = if decoder.channels > 2 {
            let layout =
                ChannelLayout::from_mask(decoder.channel_mask.ok_or("Missing speaker positions.")?)
                    .map_err(|e| e.to_string())?;
            Some(StereoDownmixer::new(layout).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let input = PcmFormat::new(decoder.sample_rate, channels).map_err(|e| e.to_string())?;
        let rate = if rate != decoder.sample_rate {
            Some(RateConverter::new(input, rate).map_err(|e| e.to_string())?)
        } else {
            None
        };
        Ok(Self {
            input_channels: decoder.channels,
            output_channels: channels as usize,
            downmix,
            rate,
            scratch: Vec::new(),
        })
    }
    fn adapt(&mut self, input: &[f32], last: bool) -> Result<&[f32], String> {
        self.scratch.clear();
        let frames = input.len() / self.input_channels;
        if let Some(downmix) = &self.downmix {
            self.scratch.resize(frames * 2, 0.);
            downmix
                .process(input, &mut self.scratch)
                .map_err(|e| e.to_string())?;
            if self.output_channels == 1 {
                for i in 0..frames {
                    self.scratch[i] = (self.scratch[2 * i] + self.scratch[2 * i + 1]) * 0.5;
                }
                self.scratch.truncate(frames);
            }
        } else {
            for frame in input.chunks_exact(self.input_channels) {
                match (self.input_channels, self.output_channels) {
                    (1, 2) => self.scratch.extend_from_slice(&[frame[0], frame[0]]),
                    (2, 1) => self.scratch.push((frame[0] + frame[1]) * 0.5),
                    _ => self.scratch.extend_from_slice(frame),
                }
            }
        }
        let samples = if let Some(rate) = &mut self.rate {
            rate.push(&self.scratch, last).map_err(|e| e.to_string())?
        } else {
            &mut self.scratch
        };
        // The shared bounded queue accepts guarded device PCM, never NaN/overflow.
        crate::dsp::protect_output(samples).map_err(|e| e.to_string())?;
        Ok(samples)
    }
}
fn publish(
    producer: &mut PcmProducer,
    mut samples: &[f32],
    signals: &Signals,
) -> Result<(), String> {
    while !samples.is_empty() {
        if signals.cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        if signals.failed.load(Ordering::Acquire) {
            return Err("Audio device failed.".into());
        }
        match producer.write_samples(samples) {
            Ok(written) => samples = &samples[written..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(2))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
pub(super) fn produce(
    mut decoder: PcmStreamDecoder,
    mut converter: Converter,
    mut producer: PcmProducer,
    offset: f64,
    signals: &Signals,
) -> Result<(), String> {
    let mut skip = (offset * f64::from(decoder.sample_rate)) as u64;
    let mut frames = 0_u64;
    while !signals.cancel.load(Ordering::Acquire) {
        let Some(packet) = decoder
            .next_chunk_admitted(|| {
                if signals.cancel.load(Ordering::Acquire) || signals.failed.load(Ordering::Acquire)
                {
                    Err(crate::decode::DecodeError("Audio decoding retired.".into()))
                } else {
                    Ok(())
                }
            })
            .map_err(|e| e.to_string())?
        else {
            break;
        };
        let count = packet.len() / decoder.channels;
        let discarded = skip.min(count as u64) as usize;
        skip -= discarded as u64;
        let input = &packet[discarded * decoder.channels..];
        frames += (input.len() / decoder.channels) as u64;
        if input.is_empty() {
            continue;
        }
        publish(&mut producer, converter.adapt(input, false)?, signals)?;
    }
    if signals.cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    // Seek at/past EOF naturally drains; an empty original file remains a failure.
    if frames == 0 && offset == 0. {
        return Err("No audio data decoded.".into());
    }
    publish(&mut producer, converter.adapt(&[], true)?, signals)?;
    producer.close_input();
    Ok(())
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
