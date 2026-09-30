//! Resolve delayed true-peak FIR output without extending programme loudness.

use ebur128::{EbuR128, Mode};

use crate::PcmFormat;

// ebur128 0.1.10 uses 12 source taps for 4x and 24 for 2x interpolation.
// Retaining 24 source frames supplies the complete state for either path.
const TAIL_FRAMES: usize = 24;

pub(crate) struct PeakTail {
    format: PcmFormat,
    samples: Vec<f32>,
}

impl PeakTail {
    pub(crate) fn new(format: PcmFormat) -> Self {
        Self {
            format,
            samples: if format.sample_rate() < 192_000 {
                Vec::with_capacity(TAIL_FRAMES * usize::from(format.channels()))
            } else {
                Vec::new()
            },
        }
    }

    /// Called after the owner validates finite PCM and complete frames.
    pub(crate) fn push(&mut self, samples: &[f32]) {
        if self.format.sample_rate() >= 192_000 {
            return;
        }
        let capacity = TAIL_FRAMES * usize::from(self.format.channels());
        if samples.len() >= capacity {
            self.samples.clear();
            self.samples
                .extend_from_slice(&samples[samples.len() - capacity..]);
        } else {
            let discard = (self.samples.len() + samples.len()).saturating_sub(capacity);
            if discard > 0 {
                self.samples.copy_within(discard.., 0);
                self.samples.truncate(self.samples.len() - discard);
            }
            self.samples.extend_from_slice(samples);
        }
    }

    /// Snapshot a finite signal's peak, treating samples after its end as zero.
    ///
    /// Only a temporary peak meter receives zero frames. The original meter and
    /// retained PCM stay unchanged, so future samples and LUFS are unaffected.
    pub(crate) fn peak(&self, original: &EbuR128) -> Result<Option<f32>, ebur128::Error> {
        if self.format.sample_rate() >= 192_000 {
            return Ok(None);
        }
        let channels = u32::from(self.format.channels());
        let mut peak = 0.0_f64;
        for channel in 0..channels {
            peak = peak.max(original.true_peak(channel)?);
        }
        if !self.samples.is_empty() {
            let mut tail = EbuR128::new(channels, self.format.sample_rate(), Mode::TRUE_PEAK)?;
            tail.add_frames_f32(&self.samples)?;
            tail.add_frames_f32(&vec![0.0; TAIL_FRAMES * channels as usize])?;
            for channel in 0..channels {
                // Ignore the temporary meter's synthetic zero-to-seed edge.
                // The zero block reports only genuinely pending final output.
                peak = peak.max(tail.prev_true_peak(channel)?);
            }
        }
        Ok(Some(peak as f32))
    }
}

#[cfg(test)]
#[path = "peak_tail_tests.rs"]
mod tests;
