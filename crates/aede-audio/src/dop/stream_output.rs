//! Bounded network DSD handoff using the existing exclusive integer backends.
//! Source words never pass through PCM gain, conversion or a device mixer.
use super::{
    Format, Payload,
    driver::{self, Factory},
    output::Snapshot,
    state::{Fault, Signals},
};
use crate::queue::{PcmConsumer, PcmProducer};
use std::{
    io::ErrorKind,
    sync::{Arc, atomic::Ordering, mpsc},
    thread::JoinHandle,
};
const MAX_WORDS: usize = 16_384;

/// One original-bit stream and one exclusive device visit. Input is bounded to
/// half a second; initial buffering opens no device before caller admission.
pub struct StreamOutput {
    source: Format,
    samples: u64,
    position: u64,
    supplied: u64,
    producer: PcmProducer<Payload>,
    consumer: Option<PcmConsumer<Payload>>,
    factory: Factory,
    signals: Arc<Signals>,
    preparing: Option<mpsc::Receiver<Result<(), String>>>,
    driver: Option<JoinHandle<Result<(), String>>>,
    finished: bool,
    closed: bool,
    close_error: Option<String>,
}
impl StreamOutput {
    /// Prepare bounded storage for an explicit DoP DAC. Length and position
    /// count original one-bit samples per channel, not padded carrier frames.
    pub fn open(format: Format, samples: u64, position: u64, device: &str) -> Result<Self, String> {
        Self::with_factory(format, samples, position, super::platform::factory(device)?)
    }
    fn with_factory(
        source: Format,
        samples: u64,
        position: u64,
        factory: Factory,
    ) -> Result<Self, String> {
        if samples == 0
            || position > samples
            || (position != samples && !position.is_multiple_of(16))
        {
            return Err("invalid original DSD stream length or position".into());
        }
        let (producer, consumer) =
            crate::queue::pcm_queue(2, (source.carrier_rate() / 2) as usize, true)
                .map_err(|e| e.to_string())?;
        Ok(Self {
            source,
            samples,
            position,
            supplied: 0,
            producer,
            consumer: Some(consumer),
            factory,
            signals: Arc::new(Signals::new(
                (samples - position).div_ceil(16),
                false,
                false,
            )),
            preparing: None,
            driver: None,
            finished: false,
            closed: false,
            close_error: None,
        })
    }
    /// Publish a complete-frame prefix of at most 32 KiB of original words.
    /// Returns SOURCE scalar words accepted; zero means bounded backpressure.
    /// Mono is duplicated bit for bit, without mixing or interpolation.
    pub fn write(&mut self, words: &[Payload]) -> Result<usize, String> {
        self.current()?;
        let channels = self.source.channels();
        if self.finished
            || words.is_empty()
            || words.len() > MAX_WORDS
            || !words.len().is_multiple_of(channels)
        {
            return self.refuse("invalid or late original DSD packet");
        }
        let frames = words.len() / channels;
        if frames as u64 > self.signals.expected - self.supplied {
            return self.refuse("original DSD stream exceeded its declared length");
        }
        let queue = self.producer.snapshot();
        let frames = frames.min(queue.capacity_frames - queue.queued_frames);
        if frames == 0 {
            return Ok(0);
        }
        let mono;
        let payload = if channels == 1 {
            mono = words[..frames]
                .iter()
                .flat_map(|&word| [word, word])
                .collect::<Vec<_>>();
            mono.as_slice()
        } else {
            &words[..frames * 2]
        };
        let accepted = match self.producer.write_samples(payload) {
            Ok(n) => n / 2,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(0),
            Err(e) => return self.refuse(&e.to_string()),
        };
        self.supplied += accepted as u64;
        let queue = self.producer.snapshot();
        if queue.queued_frames == queue.capacity_frames {
            self.signals.ready.store(true, Ordering::Release);
        }
        Ok(accepted * channels)
    }
    /// Seal input only after authenticated source EOF. Short input permanently
    /// fails; received bytes alone cannot establish natural completion.
    pub fn finish(&mut self) -> Result<(), String> {
        self.current()?;
        if self.supplied != self.signals.expected {
            return self.refuse("original DSD stream ended before its declared length");
        }
        self.finished = true;
        self.producer.close_input();
        self.signals.decoder_done.store(true, Ordering::Release);
        self.signals.ready.store(true, Ordering::Release);
        Ok(())
    }
    /// Poll on the control thread, including while paused and during drain.
    /// A full queue or validated short EOF starts preparation; the final caller
    /// check precedes admission of programme bits to the native output.
    pub fn poll_admitted(
        &mut self,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Snapshot, String> {
        let result = self.poll(&current);
        if let Err(error) = result {
            self.signals.completed.store(false, Ordering::Release);
            self.signals.fail_message(Fault::Source, error.clone());
            return match self.close() {
                Ok(()) => Err(error),
                Err(cleanup) if cleanup == error => Err(error),
                Err(cleanup) => Err(format!("{error}; {cleanup}")),
            };
        }
        result
    }
    fn poll(&mut self, current: &impl Fn() -> Result<(), String>) -> Result<Snapshot, String> {
        if self.closed {
            return self.current().map(|()| self.snapshot());
        }
        current()?;
        let snapshot = self.snapshot();
        if snapshot.completed && snapshot.error.is_none() {
            return Ok(snapshot);
        }
        self.current()?;
        if self.signals.expected == 0 && self.finished {
            self.signals.completed.store(true, Ordering::Release);
            self.signals.stopped.store(true, Ordering::Release);
        } else if self.signals.ready.load(Ordering::Acquire) && self.driver.is_none() {
            let input = self
                .consumer
                .take()
                .ok_or("DoP stream consumer was retired")?;
            let (sender, receiver) = mpsc::sync_channel(1);
            let signals = self.signals.clone();
            let factory = self.factory.clone();
            let wire = Format::new(self.source.source_rate(), 2).map_err(|e| e.to_string())?;
            self.driver = Some(
                std::thread::Builder::new()
                    .name("dsd-network-output".into())
                    .spawn(move || driver::run(factory, wire, input, signals, sender))
                    .map_err(|e| e.to_string())?,
            );
            self.preparing = Some(receiver);
        }
        if let Some(receiver) = &self.preparing {
            match receiver.try_recv() {
                Ok(result) => {
                    result?;
                    current()?;
                    self.signals.current()?;
                    self.preparing = None;
                    self.signals.admitted.store(true, Ordering::Release);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("DoP native preparation stopped unexpectedly".into());
                }
            }
        }
        let snapshot = self.snapshot();
        if let Some(error) = &snapshot.error {
            return Err(error.clone());
        }
        Ok(snapshot)
    }
    /// Carrier frames delivered relative to this visit, for network ACKs.
    /// This is software handoff evidence, not a physical DAC acknowledgement.
    pub fn consumed_frames(&self) -> u64 {
        self.signals
            .consumed
            .load(Ordering::Acquire)
            .min(self.signals.expected)
    }
    /// Original-bit progress, distinct from padded carrier frames and latency.
    pub fn snapshot(&self) -> Snapshot {
        if self.driver.as_ref().is_some_and(JoinHandle::is_finished)
            && !self.signals.stopped.load(Ordering::Acquire)
        {
            self.signals.fail_message(
                Fault::Worker,
                "DoP native control thread stopped unexpectedly".into(),
            );
        }
        Snapshot {
            position_samples: self
                .position
                .saturating_add(self.consumed_frames().saturating_mul(16))
                .min(self.samples),
            source_rate: self.source.source_rate(),
            source_samples: self.samples,
            completed: self.signals.completed.load(Ordering::Acquire),
            paused: self.signals.paused.load(Ordering::Acquire),
            muted: self.signals.muted.load(Ordering::Acquire),
            error: self.signals.error(),
        }
    }
    /// Pause emits framed DSD silence and retains original pending words.
    pub fn pause(&self) {
        self.signals.paused.store(true, Ordering::Release);
    }
    /// Resume programme consumption on the same exclusive route.
    pub fn resume(&self) {
        self.signals.paused.store(false, Ordering::Release);
    }
    /// Mute advances source time while writing balanced DSD silence.
    pub fn set_muted(&self, value: bool) {
        self.signals.muted.store(value, Ordering::Release);
    }
    /// Retire and restore the device. Failures remain errors on repeated close;
    /// manual retirement never establishes natural completion.
    pub fn close(&mut self) -> Result<(), String> {
        if !self.closed {
            self.closed = true;
            self.signals.cancel.store(true, Ordering::Release);
            self.producer.close_input();
            self.consumer.take();
            let worker = self.driver.take().map(|driver| driver.join());
            self.close_error = match worker {
                Some(Ok(Err(error))) => Some(error),
                Some(Err(_)) => Some("DoP native control thread panicked".into()),
                _ => self.signals.error(),
            };
        }
        self.close_error.clone().map_or(Ok(()), Err)
    }
    fn current(&self) -> Result<(), String> {
        if self.closed {
            return Err(self
                .close_error
                .clone()
                .unwrap_or_else(|| "DoP stream is retired".into()));
        }
        self.signals.current()
    }
    fn refuse<T>(&mut self, message: &str) -> Result<T, String> {
        self.signals.completed.store(false, Ordering::Release);
        self.signals.fail_message(Fault::Source, message.into());
        self.signals.cancel.store(true, Ordering::Release);
        Err(message.into())
    }
}
impl Drop for StreamOutput {
    fn drop(&mut self) {
        if !self.closed
            && let Err(error) = self.close()
        {
            eprintln!("network DoP cleanup failed: {error}");
        }
    }
}
#[cfg(test)]
#[path = "stream_output_tests.rs"]
mod tests;
