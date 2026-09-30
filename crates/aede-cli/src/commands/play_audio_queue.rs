//! Bounded single-producer PCM handoff to an audio callback.
//!
//! The ring stores interleaved samples and publishes only complete frames.
//! Allocation and validation happen outside the callback. Rendering copies
//! directly from preallocated ring storage and never converts inserted silence.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rtrb::{Consumer, Producer, RingBuffer};

/// A concurrent observation of the bounded queue, expressed in PCM frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct QueueSnapshot {
    pub(super) queued_frames: usize,
    pub(super) capacity_frames: usize,
    pub(super) consumed_frames: u64,
    pub(super) underrun_frames: u64,
    pub(super) underrun_callbacks: u64,
}

struct Counters {
    submitted_samples: AtomicU64,
    consumed_samples: AtomicU64,
    underrun_frames: AtomicU64,
    underrun_callbacks: AtomicU64,
    closed: AtomicBool,
}

/// The decoding thread's sole writer to a fixed-capacity PCM queue.
pub(super) struct PcmProducer {
    ring: Producer<f32>,
    channels: usize,
    capacity_frames: usize,
    counters: Arc<Counters>,
}

/// The audio callback's sole reader of complete interleaved PCM frames.
pub(super) struct PcmConsumer {
    ring: Consumer<f32>,
    channels: usize,
    counters: Arc<Counters>,
}

/// Allocate the queue before starting the audio stream. Zero channels, zero
/// capacity and capacities that overflow an allocation are rejected.
pub(super) fn pcm_queue(
    channels: usize,
    capacity_frames: usize,
) -> io::Result<(PcmProducer, PcmConsumer)> {
    let capacity_samples = capacity_frames
        .checked_mul(channels)
        .filter(|&samples| {
            samples > 0 && samples <= isize::MAX as usize / std::mem::size_of::<f32>()
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid PCM queue capacity"))?;
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    let counters = Arc::new(Counters {
        submitted_samples: AtomicU64::new(0),
        consumed_samples: AtomicU64::new(0),
        underrun_frames: AtomicU64::new(0),
        underrun_callbacks: AtomicU64::new(0),
        closed: AtomicBool::new(false),
    });
    Ok((
        PcmProducer {
            ring: producer,
            channels,
            capacity_frames,
            counters: Arc::clone(&counters),
        },
        PcmConsumer {
            ring: consumer,
            channels,
            counters,
        },
    ))
}

impl PcmProducer {
    /// Validate the entire input, then publish the largest complete-frame
    /// prefix that fits. Return samples written, or `WouldBlock` if no frame
    /// fits. Invalid PCM never publishes even a preceding valid prefix.
    pub(super) fn write_samples(&mut self, samples: &[f32]) -> io::Result<usize> {
        if !samples.len().is_multiple_of(self.channels)
            || samples.iter().any(|&sample| !valid_sample(sample))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PCM requires complete finite guarded frames",
            ));
        }
        self.write_validated(samples.len(), samples.iter().copied())
    }

    /// Validate complete little-endian floating-point PCM before publication.
    /// Return bytes written; the writer initializes ring slots directly from
    /// the encoded samples and does not allocate an intermediate PCM buffer.
    pub(super) fn write_f32le(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let (samples, remainder) = bytes.as_chunks::<4>();
        if !remainder.is_empty()
            || !samples.len().is_multiple_of(self.channels)
            || samples
                .iter()
                .any(|&sample| !valid_sample(f32::from_le_bytes(sample)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "f32le requires complete finite guarded frames",
            ));
        }
        self.write_validated(
            samples.len(),
            samples.iter().map(|&sample| f32::from_le_bytes(sample)),
        )
        .map(|written| written * 4)
    }

    fn write_validated(
        &mut self,
        samples: usize,
        values: impl Iterator<Item = f32>,
    ) -> io::Result<usize> {
        if self.counters.closed.load(Ordering::Acquire) || self.ring.is_abandoned() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PCM queue is closed",
            ));
        }
        if samples == 0 {
            return Ok(0);
        }
        let available = self.ring.slots();
        let accepted = samples.min(available / self.channels * self.channels);
        if accepted == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        // Only the consumer can change available slots, and it can only add
        // free slots. This reservation cannot shrink between these calls.
        let chunk = self
            .ring
            .write_chunk_uninit(accepted)
            .map_err(|_| io::Error::from(io::ErrorKind::WouldBlock))?;
        let written = chunk.fill_from_iter(values.take(accepted));
        self.counters
            .submitted_samples
            .fetch_add(written as u64, Ordering::Release);
        Ok(written)
    }

    /// Seal the producer while allowing queued frames to reach the callback.
    pub(super) fn close_input(&self) {
        self.counters.closed.store(true, Ordering::Release);
    }

    /// Whether the callback has consumed every submitted sample.
    pub(super) fn drained(&self) -> bool {
        self.counters.submitted_samples.load(Ordering::Acquire)
            == self.counters.consumed_samples.load(Ordering::Acquire)
    }

    /// Observe bounded queue occupancy and monotonically increasing counters.
    /// Occupancy is approximate while the producer and callback are advancing.
    pub(super) fn snapshot(&self) -> QueueSnapshot {
        let submitted = self.counters.submitted_samples.load(Ordering::Acquire);
        let consumed = self.counters.consumed_samples.load(Ordering::Acquire);
        let queued = submitted.saturating_sub(consumed) / self.channels as u64;
        QueueSnapshot {
            queued_frames: usize::try_from(queued)
                .unwrap_or(usize::MAX)
                .min(self.capacity_frames),
            capacity_frames: self.capacity_frames,
            consumed_frames: consumed / self.channels as u64,
            underrun_frames: self.counters.underrun_frames.load(Ordering::Acquire),
            underrun_callbacks: self.counters.underrun_callbacks.load(Ordering::Acquire),
        }
    }
}

impl PcmConsumer {
    /// Render available whole frames, then exact digital silence. The callback
    /// performs bounded copying and atomic counter updates; it does not allocate,
    /// deallocate, lock, log or wait. An incomplete final output frame receives
    /// silence without consuming any source channel.
    pub(super) fn render_mapped<T: Copy>(
        &mut self,
        output: &mut [T],
        silence: T,
        mut convert: impl FnMut(f32) -> T,
    ) {
        let closed_before = self.counters.closed.load(Ordering::Acquire);
        let started_before = self.counters.submitted_samples.load(Ordering::Acquire) > 0;
        output.fill(silence);
        let requested_frames = output.len() / self.channels;
        if requested_frames == 0 {
            return;
        }
        let available_frames = self.ring.slots() / self.channels;
        let samples = requested_frames.min(available_frames) * self.channels;
        let mut consumed = 0;
        if samples > 0
            && let Ok(chunk) = self.ring.read_chunk(samples)
        {
            let (first, second) = chunk.as_slices();
            for (source, target) in first.iter().chain(second).zip(&mut output[..samples]) {
                *target = convert(*source);
            }
            chunk.commit_all();
            consumed = samples;
            self.counters
                .consumed_samples
                .fetch_add(consumed as u64, Ordering::Release);
        }
        let missing_frames = requested_frames - consumed / self.channels;
        if missing_frames > 0 && started_before && !closed_before {
            self.counters
                .underrun_frames
                .fetch_add(missing_frames as u64, Ordering::Release);
            self.counters
                .underrun_callbacks
                .fetch_add(1, Ordering::Release);
        }
    }
}

fn valid_sample(sample: f32) -> bool {
    sample.is_finite() && (-1.0..=1.0).contains(&sample)
}

#[cfg(test)]
#[path = "play_audio_queue_tests.rs"]
mod tests;
