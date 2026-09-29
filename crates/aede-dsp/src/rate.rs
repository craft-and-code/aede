//! Stateful, bandlimited conversion between fixed PCM sample rates.

use std::fmt;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, Fft, FixedAsync, FixedSync, Indexing, Resampler, SincInterpolationParameters};

const CHUNK_FRAMES: usize = 1024;

#[derive(Debug)]
pub enum RateError {
    InvalidFormat,
    IncompleteFrame,
    NonFiniteSample,
    Finished,
    Resampler(String),
}

impl fmt::Display for RateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("unsupported PCM rate or channel count"),
            Self::IncompleteFrame => f.write_str("PCM buffer does not contain complete frames"),
            Self::NonFiniteSample => f.write_str("PCM buffer contains a non-finite sample"),
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
    scratch: Vec<f32>,
    output: Vec<f32>,
    skip_frames: usize,
    input_frames: u64,
    emitted_frames: u64,
    finished: bool,
}

impl RateConverter {
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
        let resampler: Box<dyn Resampler<f32>> = if input_rate / common > CHUNK_FRAMES as u32
            || output_rate / common > CHUNK_FRAMES as u32
        {
            Box::new(
                Async::<f32>::new_sinc(
                    f64::from(output_rate) / f64::from(input_rate),
                    1.0,
                    &SincInterpolationParameters::default(),
                    CHUNK_FRAMES,
                    channels,
                    FixedAsync::Input,
                )
                .map_err(|error| RateError::Resampler(error.to_string()))?,
            )
        } else {
            Box::new(
                Fft::<f32>::new(
                    input_rate as usize,
                    output_rate as usize,
                    CHUNK_FRAMES,
                    channels,
                    FixedSync::Input,
                )
                .map_err(|error| RateError::Resampler(error.to_string()))?,
            )
        };
        let skip_frames = resampler.output_delay();
        let scratch = vec![0.0; resampler.output_frames_max() * channels];
        Ok(Self {
            resampler,
            channels,
            input_rate,
            output_rate,
            pending: Vec::with_capacity(CHUNK_FRAMES * channels * 2),
            scratch,
            output: Vec::new(),
            skip_frames,
            input_frames: 0,
            emitted_frames: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, samples: &[f32], last: bool) -> Result<&mut [f32], RateError> {
        if self.finished {
            return Err(RateError::Finished);
        }
        if !samples.len().is_multiple_of(self.channels) {
            return Err(RateError::IncompleteFrame);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(RateError::NonFiniteSample);
        }
        self.output.clear();
        self.input_frames += (samples.len() / self.channels) as u64;
        self.pending.extend_from_slice(samples);
        let target = last.then(|| {
            self.input_frames
                .saturating_mul(u64::from(self.output_rate))
                .div_ceil(u64::from(self.input_rate))
        });
        while self.pending.len() / self.channels >= self.resampler.input_frames_next() {
            self.process_chunk(None, target)?;
        }
        if let Some(target) = target {
            if !self.pending.is_empty() {
                let partial = self.pending.len() / self.channels;
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
        let input = InterleavedSlice::new(&self.pending, self.channels, valid)
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
        let remaining = self.pending.len() - valid * self.channels;
        self.pending.copy_within(valid * self.channels.., 0);
        self.pending.truncate(remaining);
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
