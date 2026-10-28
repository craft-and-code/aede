//! Minimal DoP control handle. Native output never accepts a PCM gain or DSP.
use super::{
    Format, Reader, Stamp,
    driver::{self, Factory},
    state::{Fault, Signals},
    worker,
};
use crate::dsd::Info;
use std::{
    fs::File,
    path::Path,
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, RecvTimeoutError},
    },
    thread::JoinHandle,
    time::Duration,
};

/// Exact original-bit progress, separate from padded carrier frames and DAC latency.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// Original one-bit samples per source channel handed to the native output.
    pub position_samples: u64,
    /// Original clock, without labelling the DoP carrier as converted PCM.
    pub source_rate: u32,
    /// The original valid source-bit length per channel.
    pub source_samples: u64,
    /// Confirmed natural software completion after bounded native drain/cleanup.
    pub completed: bool,
    /// User pause, which emits framed DSD silence without consuming programme bits.
    pub paused: bool,
    /// User mute, which emits DSD silence while programme progress continues.
    pub muted: bool,
    /// Permanent source/route/worker failure, never a converted-PCM fallback.
    pub error: Option<String>,
}

/// Owned source, producer and native control thread. Drop stops output before
/// joining a possibly in-flight source read; OS filesystem I/O is not forced.
pub struct Player {
    retained: File,
    source: Info,
    stamp: Stamp,
    wire: Format,
    offset: u64,
    factory: Factory,
    signals: Arc<Signals>,
    decoder: Option<JoinHandle<()>>,
    driver: Option<JoinHandle<Result<(), String>>>,
    closed: bool,
}
impl Player {
    /// Open a DAC explicitly declared DoP-compatible by its user. Hardware must
    /// expose a matching exclusive integer route; a rate alone is not proof of
    /// DoP support. No default device, volume change or PCM fallback is selected.
    pub fn open(path: &Path, device: &str) -> Result<Self, String> {
        Self::open_admitted(path, device, || Ok(()))
    }

    /// Preserve caller admission through source/native setup and before original
    /// bits can be consumed. Prepared native callbacks remain on DSD silence.
    pub fn open_admitted(
        path: &Path,
        device: &str,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::open_at_admitted(path, device, Duration::ZERO, current)
    }

    /// Prepare an initial source position before opening native output. Time is
    /// rounded down to a 16-bit source boundary and clamped at original EOF.
    pub fn open_at_admitted(
        path: &Path,
        device: &str,
        position: Duration,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::open_at_admitted_with_controls(path, device, position, false, false, current)
    }

    /// Prepare caller-selected pause/mute before the first native callback can
    /// consume programme bits. Volume remains a hardware-only control for DoP.
    pub fn open_at_admitted_with_controls(
        path: &Path,
        device: &str,
        position: Duration,
        paused: bool,
        muted: bool,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        current()?;
        let factory = super::platform::factory(device)?;
        Self::open_with_factory(path, factory, position, paused, muted, current)
    }

    fn open_with_factory(
        path: &Path,
        factory: Factory,
        position: Duration,
        paused: bool,
        muted: bool,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        current()?;
        let file = File::open(path).map_err(|e| e.to_string())?;
        let reader = Reader::from_file(file.try_clone().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let source = reader.info();
        let offset = frame_at_time(position, source);
        Self::prepare(
            file,
            factory,
            offset,
            paused,
            muted,
            Some((source, reader.stamp)),
            &current,
        )
    }

    fn prepare(
        retained: File,
        factory: Factory,
        offset: u64,
        paused: bool,
        muted: bool,
        expected: Option<(Info, Stamp)>,
        current: &impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        let mut player =
            Self::prepare_held(retained, factory, offset, paused, muted, expected, current)?;
        player.activate(current)?;
        Ok(player)
    }

    fn prepare_held(
        retained: File,
        factory: Factory,
        offset: u64,
        paused: bool,
        muted: bool,
        expected: Option<(Info, Stamp)>,
        current: &impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        current()?;
        let mut reader = Reader::from_file(retained.try_clone().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        reader.seek(offset).map_err(|e| e.to_string())?;
        let source = reader.info();
        let stamp = reader.stamp;
        if expected.is_some_and(|expected| expected != (source, stamp)) {
            return Err("the retained DSD source changed before seek preparation".into());
        }
        // Mono is duplicated bit for bit to the verified stereo DAC association.
        let wire = Format::new(source.sample_rate, 2).map_err(|e| e.to_string())?;
        let signals = Arc::new(Signals::new(reader.frames() - offset, paused, muted));
        let mut player = Self {
            retained,
            source,
            stamp,
            wire,
            offset,
            factory,
            signals: signals.clone(),
            decoder: None,
            driver: None,
            closed: false,
        };
        current()?;
        if signals.expected == 0 {
            signals.completed.store(true, Ordering::Release);
            signals.stopped.store(true, Ordering::Release);
            return Ok(player);
        }
        let (producer, consumer) =
            crate::queue::pcm_queue(2, (wire.carrier_rate() / 2) as usize, true)
                .map_err(|e| e.to_string())?;
        let owned = signals.clone();
        player.decoder = Some(
            std::thread::Builder::new()
                .name("dsd-dop-source".into())
                .spawn(move || worker::run(reader, producer, owned))
                .map_err(|e| e.to_string())?,
        );
        let (send, receive) = mpsc::sync_channel(1);
        let owned = signals.clone();
        let factory = player.factory.clone();
        player.driver = Some(
            std::thread::Builder::new()
                .name("dsd-dop-output".into())
                .spawn(move || driver::run(factory, wire, consumer, owned, send))
                .map_err(|e| e.to_string())?,
        );
        let preparation = (|| {
            loop {
                current()?;
                match receive.recv_timeout(Duration::from_millis(2)) {
                    Ok(result) => {
                        result?;
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err("DoP native preparation stopped unexpectedly".into());
                    }
                }
            }
            current()?;
            Ok(())
        })();
        if let Err(error) = preparation {
            return match player.close() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; {cleanup}")),
            };
        }
        Ok(player)
    }

    fn activate(&mut self, current: &impl Fn() -> Result<(), String>) -> Result<(), String> {
        if let Err(error) = current().and_then(|()| self.signals.current()) {
            self.signals.completed.store(false, Ordering::Release);
            self.signals.fail_message(Fault::Source, error.clone());
            return match self.close() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; {cleanup}")),
            };
        }
        self.signals.admitted.store(true, Ordering::Release);
        Ok(())
    }

    /// Genuine source facts. A mono source stays mono in this metadata.
    pub fn info(&self) -> Info {
        self.source
    }
    /// Derived stereo carrier; mono payloads are copied to both speakers.
    pub fn output_format(&self) -> Format {
        self.wire
    }
    /// Pause preserves marker continuity and the queued original source words.
    pub fn pause(&self) {
        self.signals.paused.store(true, Ordering::Release);
    }
    /// Resume consumes the retained words without resetting link marker phase.
    pub fn resume(&self) {
        self.signals.paused.store(false, Ordering::Release);
    }
    /// Mute is DSD silence, never multiplication of encoded DoP carrier values.
    pub fn set_muted(&self, muted: bool) {
        self.signals.muted.store(muted, Ordering::Release);
    }
    /// Poll outside the callback. Counts are software handoff evidence, not a
    /// physical-DAC bit-identity measurement or a claim of hardware gaplessness.
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
                .offset
                .saturating_add(self.signals.consumed.load(Ordering::Acquire))
                .saturating_mul(16)
                .min(self.source.samples),
            source_rate: self.source.sample_rate,
            source_samples: self.source.samples,
            completed: self.signals.completed.load(Ordering::Acquire),
            paused: self.signals.paused.load(Ordering::Acquire),
            muted: self.signals.muted.load(Ordering::Acquire),
            error: self.signals.error(),
        }
    }
    /// Seek on the same retained file, at the 16-bit transport boundary. Output
    /// is stopped/restored before a new stream is prepared; no gapless seek or
    /// pathname replacement adoption is implied. Pause/mute are preserved.
    pub fn seek_frame_admitted(
        &mut self,
        frame: u64,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Snapshot, String> {
        current()?;
        if self.closed {
            return Err("DoP player is closed".into());
        }
        if frame > self.source.samples.div_ceil(16) {
            return Err("DoP seek exceeds original length".into());
        }
        if let Some(error) = self.signals.error() {
            return Err(error);
        }
        let file = self.retained.try_clone().map_err(|e| e.to_string())?;
        let paused = self.signals.paused.load(Ordering::Acquire);
        let muted = self.signals.muted.load(Ordering::Acquire);
        let replacement: Result<Self, String> = (|| {
            self.close()?;
            current()?;
            let next = Self::prepare_held(
                file,
                self.factory.clone(),
                frame,
                paused,
                muted,
                Some((self.source, self.stamp)),
                &current,
            )?;
            // install owns final admission and returns any rollback failure;
            // rejecting here would leave replacement cleanup to Drop.
            Ok(next)
        })();
        match replacement {
            Ok(next) => self.install(next, &current),
            Err(error) => {
                self.signals.completed.store(false, Ordering::Release);
                self.signals.fail_message(Fault::Source, error.clone());
                Err(error)
            }
        }
    }

    fn install(
        &mut self,
        mut next: Self,
        current: &impl Fn() -> Result<(), String>,
    ) -> Result<Snapshot, String> {
        let previous = self.snapshot();
        // A refused replacement has delivered no programme bits. Keep the
        // retired route's final count so callers cannot count the skipped
        // destination as played audio when activation fails.
        if let Err(error) = next.activate(current) {
            self.signals.completed.store(false, Ordering::Release);
            self.signals.fail_message(Fault::Source, error.clone());
            return Err(error);
        }
        *self = next;
        Ok(previous)
    }
    /// Stop native output and restore the device before waiting for source I/O.
    /// Repeated close is harmless; source/native errors remain observable.
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        self.signals.cancel.store(true, Ordering::Release);
        let mut errors = Vec::new();
        if let Some(driver) = self.driver.take() {
            match driver.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(error),
                Err(_) => errors.push("DoP native control thread panicked".into()),
            }
        }
        if let Some(decoder) = self.decoder.take()
            && decoder.join().is_err()
        {
            errors.push("DoP source thread panicked".into());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

fn frame_at_time(position: Duration, source: Info) -> u64 {
    (position.as_nanos() * u128::from(source.sample_rate) / 16_000_000_000)
        .min(u128::from(source.samples.div_ceil(16))) as u64
}
impl Drop for Player {
    fn drop(&mut self) {
        if !self.closed
            && let Err(error) = self.close()
        {
            eprintln!("DoP playback cleanup failed: {error}");
        }
    }
}

/// Enumerate native output IDs without opening, changing or starting a route.
/// Enumeration establishes no DoP compatibility or physical bit preservation.
pub fn devices() -> Result<Vec<(String, String)>, String> {
    super::platform::devices()
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
