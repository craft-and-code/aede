//! Stateful, bandlimited conversion between fixed PCM sample rates.

use std::fmt;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, Fft, FixedAsync, FixedSync, Indexing, Resampler, SincInterpolationParameters,
    WindowFunction, calculate_cutoff,
};

const CHUNK_FRAMES: usize = 1024;
const MIN_FILTER_FRAMES: usize = 256;

/// Validation or numerical failures while converting one PCM stream.
#[derive(Debug)]
pub enum RateError {
    /// A rate is outside 8–384 kHz, channels exceed 32, or rates are equal.
    InvalidFormat,
    /// The input ends inside an interleaved frame.
    IncompleteFrame,
    /// An input sample is NaN or infinite.
    NonFiniteSample,
    /// Magnitudes exceed the conservative internal arithmetic range, or a
    /// conversion produces non-finite output.
    SampleOverflow,
    /// Finalization or an internal numerical failure closed the converter.
    Finished,
    /// The underlying resampler refused construction or processing.
    Resampler(String),
}

impl fmt::Display for RateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("unsupported PCM rate or channel count"),
            Self::IncompleteFrame => f.write_str("PCM buffer does not contain complete frames"),
            Self::NonFiniteSample => f.write_str("PCM buffer contains a non-finite sample"),
            Self::SampleOverflow => {
                f.write_str("sample-rate conversion would overflow an f32 sample")
            }
            Self::Finished => f.write_str("sample-rate conversion has already finished"),
            Self::Resampler(error) => write!(f, "sample-rate conversion failed: {error}"),
        }
    }
}

impl std::error::Error for RateError {}

/// A fixed-ratio converter for one uninterrupted source stream.
///
/// `push` accepts any number of complete interleaved frames. Set `last` on
/// the final call to emit the filter tail, trim its startup delay, and produce
/// exactly ceil(input frames × output rate / input rate) frames. Calls do not
/// allocate in the output callback; this type runs on the decoding thread.
pub struct RateConverter {
    resampler: Box<dyn Resampler<f32>>,
    channels: usize,
    input_rate: u32,
    output_rate: u32,
    pending: Vec<f32>,
    pending_start: usize,
    maximum_input: f32,
    scratch: Vec<f32>,
    output: Vec<f32>,
    skip_frames: usize,
    input_frames: u64,
    emitted_frames: u64,
    finished: bool,
}

impl RateConverter {
    /// Prepare FFT or sinc conversion for 1–32 channels and rates within
    /// 8–384 kHz. Equal rates should use a bypass instead of a converter.
    pub fn new(input: PcmFormat, output_rate: u32) -> Result<Self, RateError> {
        let input_rate = input.sample_rate();
        let channels = usize::from(input.channels());
        if !(8_000..=384_000).contains(&input_rate)
            || !(8_000..=384_000).contains(&output_rate)
            || channels == 0
            || channels > 32
            || input_rate == output_rate
        {
            return Err(RateError::InvalidFormat);
        }
        let common = gcd(input_rate, output_rate);
        let filter_input_frames = (MIN_FILTER_FRAMES * input_rate as usize)
            .div_ceil(input_rate.min(output_rate) as usize);
        let resampler: Box<dyn Resampler<f32>> = if input_rate / common > CHUNK_FRAMES as u32
            || output_rate / common > CHUNK_FRAMES as u32
        {
            // Sinc taps are in input frames too. Keep the same lower-rate
            // bandwidth and guard band when downsampling; an automatic cutoff
            // based on the enlarged input length would admit aliases.
            let parameters = SincInterpolationParameters::new(
                filter_input_frames,
                WindowFunction::BlackmanHarris2,
            )
            .f_cutoff(calculate_cutoff(
                MIN_FILTER_FRAMES,
                WindowFunction::BlackmanHarris2,
            ));
            Box::new(
                Async::<f32>::new_sinc(
                    f64::from(output_rate) / f64::from(input_rate),
                    1.0,
                    &parameters,
                    CHUNK_FRAMES,
                    channels,
                    FixedAsync::Input,
                )
                .map_err(|error| RateError::Resampler(error.to_string()))?,
            )
        } else {
            // Rubato sizes its filter on the smaller side of the rate pair.
            // Its default 256 input frames leave only 64 filter frames at
            // 192 -> 48 kHz, attenuating 19.2 kHz by nearly 8 dB. Preserve
            // at least 256 frames at the lower rate, even for large ratios.
            let chunk_frames = CHUNK_FRAMES.max(filter_input_frames);
            Box::new(
                Fft::<f32>::new_custom(
                    input_rate as usize,
                    output_rate as usize,
                    chunk_frames,
                    chunk_frames / filter_input_frames,
                    channels,
                    WindowFunction::BlackmanHarris2,
                    FixedSync::Input,
                )
                .map_err(|error| RateError::Resampler(error.to_string()))?,
            )
        };
        let skip_frames = resampler.output_delay();
        let scratch = vec![0.0; resampler.output_frames_max() * channels];
        let pending = Vec::with_capacity(resampler.input_frames_max() * channels * 2);
        // Rubato's FFT performs unchecked f32 arithmetic before an internal
        // inverse-transform assertion. Bound intermediates conservatively,
        // retaining hundreds of dB of headroom without clipping source PCM.
        let work_frames = resampler
            .input_frames_max()
            .max(resampler.output_frames_max())
            .max(filter_input_frames) as f64;
        let maximum_input = (f64::from(f32::MAX) / (16.0 * work_frames.powi(2))) as f32;
        Ok(Self {
            resampler,
            channels,
            input_rate,
            output_rate,
            pending,
            pending_start: 0,
            maximum_input,
            scratch,
            output: Vec::new(),
            skip_frames,
            input_frames: 0,
            emitted_frames: 0,
            finished: false,
        })
    }

    /// Convert complete interleaved frames and return this call's output.
    ///
    /// `last` drains and closes the stream, including for an empty final
    /// block. Input validation leaves state unchanged. Magnitudes beyond a
    /// conservative, size-dependent numerical bound are refused rather than
    /// clipped; ordinary PCM above full scale remains supported. Any internal
    /// processing failure requires abandoning the stream. The returned slice
    /// is invalidated by the next call.
    pub fn push(&mut self, samples: &[f32], last: bool) -> Result<&mut [f32], RateError> {
        if self.finished {
            return Err(RateError::Finished);
        }
        if !samples.len().is_multiple_of(self.channels) {
            return Err(RateError::IncompleteFrame);
        }
        let mut overflow = false;
        for &sample in samples {
            if !sample.is_finite() {
                return Err(RateError::NonFiniteSample);
            }
            overflow |= sample.abs() > self.maximum_input;
        }
        if overflow {
            return Err(RateError::SampleOverflow);
        }
        self.output.clear();
        self.input_frames += (samples.len() / self.channels) as u64;
        if self.pending_start > 0 {
            self.pending.copy_within(self.pending_start.., 0);
            self.pending
                .truncate(self.pending.len() - self.pending_start);
            self.pending_start = 0;
        }
        self.pending.extend_from_slice(samples);
        let target = last.then(|| {
            self.input_frames
                .saturating_mul(u64::from(self.output_rate))
                .div_ceil(u64::from(self.input_rate))
        });
        while (self.pending.len() - self.pending_start) / self.channels
            >= self.resampler.input_frames_next()
        {
            self.process_chunk(None, target)?;
        }
        if let Some(target) = target {
            if self.pending_start < self.pending.len() {
                let partial = (self.pending.len() - self.pending_start) / self.channels;
                self.process_chunk(Some(partial), Some(target))?;
            }
            while self.emitted_frames < target {
                self.process_chunk(Some(0), Some(target))?;
            }
            self.finished = true;
        }
        Ok(&mut self.output)
    }

    fn process_chunk(
        &mut self,
        partial: Option<usize>,
        target: Option<u64>,
    ) -> Result<(), RateError> {
        let needed = self.resampler.input_frames_next();
        let valid = partial.unwrap_or(needed);
        let input =
            InterleavedSlice::new(&self.pending[self.pending_start..], self.channels, valid)
                .map_err(|error| RateError::Resampler(error.to_string()))?;
        let mut output = InterleavedSlice::new_mut(
            &mut self.scratch,
            self.channels,
            self.resampler.output_frames_max(),
        )
        .map_err(|error| RateError::Resampler(error.to_string()))?;
        let indexing = Indexing {
            partial_len: partial,
            ..Indexing::default()
        };
        let (_, produced) = self
            .resampler
            .process_into_buffer(&input, &mut output, Some(&indexing))
            .map_err(|error| RateError::Resampler(error.to_string()))?;
        if self.scratch[..produced * self.channels]
            .iter()
            .any(|sample| !sample.is_finite())
        {
            self.finished = true;
            self.output.clear();
            return Err(RateError::SampleOverflow);
        }
        self.pending_start += valid * self.channels;
        let skipped = self.skip_frames.min(produced);
        self.skip_frames -= skipped;
        let available = produced - skipped;
        let accepted = target
            .map(|limit| available.min(limit.saturating_sub(self.emitted_frames) as usize))
            .unwrap_or(available);
        self.output.extend_from_slice(
            &self.scratch[skipped * self.channels..(skipped + accepted) * self.channels],
        );
        self.emitted_frames += accepted as u64;
        Ok(())
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

use super::PcmFormat;

#[cfg(test)]
#[path = "rate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rate_quality_tests.rs"]
mod quality_tests;
