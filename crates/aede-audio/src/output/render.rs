//! Callback-side rendering: preallocated queue, atomics and device conversion only.
use super::*;
use crate::queue::PcmConsumer;
use cpal::{FromSample, SampleFormat, SizedSample, SupportedStreamConfig};
fn render<T: SizedSample + FromSample<f32>>(
    data: &mut [T],
    ring: &mut PcmConsumer,
    signals: &Signals,
    gain: &Gain,
) {
    if signals.paused.load(Ordering::Acquire) || signals.failed.load(Ordering::Acquire) {
        data.fill(T::from_sample(0.));
        signals.level.store(0_f32.to_bits(), Ordering::Relaxed);
        return;
    }
    let volume = gain.value();
    let mut energy = 0_f64;
    ring.render_mapped(data, T::from_sample(0.), |sample| {
        let value = sample * volume;
        energy += f64::from(value).powi(2);
        // I24's unchecked float conversion cannot represent the guard's inclusive +1 endpoint.
        let value = if T::FORMAT == SampleFormat::I24 {
            value.min(8_388_607. / 8_388_608.)
        } else {
            value
        };
        T::from_sample(value)
    });
    // This meter includes output silence; clock counts only actual source consumption.
    let level = if data.is_empty() {
        0.
    } else {
        ((energy / data.len() as f64).sqrt() as f32 * 4.).min(1.)
    };
    signals.level.store(level.to_bits(), Ordering::Relaxed);
    // The consumer's snapshot is callback-local except for atomic counters.
    signals
        .consumed
        .store(ring.consumed_frames(), Ordering::Release);
    if ring.input_closed() && ring.available_frames() == 0 {
        signals.finished.store(true, Ordering::Release);
    }
}
pub(super) fn build(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    mut ring: PcmConsumer,
    signals: Arc<Signals>,
    gain: Arc<Gain>,
) -> Result<cpal::Stream, String> {
    macro_rules! build {
        ($ty:ty) => {{
            let failed = signals.clone();
            device
                .build_output_stream(
                    config.config(),
                    move |data: &mut [$ty], _| render(data, &mut ring, &signals, &gain),
                    move |error| {
                        if error.kind() != cpal::ErrorKind::DeviceChanged {
                            failed.failed.store(true, Ordering::Release);
                        }
                    },
                    None,
                )
                .map_err(|e| e.to_string())
        }};
    }
    match config.sample_format() {
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I8 => build!(i8),
        SampleFormat::U8 => build!(u8),
        SampleFormat::I24 => build!(cpal::I24),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        _ => Err("Unsupported audio output sample format.".into()),
    }
}
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
