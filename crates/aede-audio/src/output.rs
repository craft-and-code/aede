//! Single-file preview playback. No catalogue, normalization, EQ, queue or history.
use crate::decode::PcmStreamDecoder;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};
mod render;
mod worker;

/// Independent mute and volume shared across successive preview streams.
pub struct Gain {
    volume: AtomicU32,
    muted: AtomicBool,
}
impl Default for Gain {
    fn default() -> Self {
        Self {
            volume: AtomicU32::new(1_f32.to_bits()),
            muted: AtomicBool::new(false),
        }
    }
}
impl Gain {
    /// Non-finite choices are ignored; finite volume is clamped to 0–1.
    pub fn set_volume(&self, volume: f32) {
        if volume.is_finite() {
            self.volume
                .store(volume.clamp(0., 1.).to_bits(), Ordering::Relaxed);
        }
    }
    /// Muting preserves the selected volume.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }
    pub(super) fn value(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.
        } else {
            f32::from_bits(self.volume.load(Ordering::Relaxed))
        }
    }
}
struct Signals {
    paused: AtomicBool,
    cancel: AtomicBool,
    finished: AtomicBool,
    failed: AtomicBool,
    consumed: AtomicU64,
    level: AtomicU32,
    error: Mutex<Option<String>>,
}
impl Default for Signals {
    fn default() -> Self {
        Self {
            paused: AtomicBool::new(true),
            cancel: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            consumed: AtomicU64::new(0),
            level: AtomicU32::new(0),
            error: Mutex::new(None),
        }
    }
}
/// Device-clocked observation, polled outside the audio callback.
pub struct Snapshot {
    /// Seconds actually consumed, plus the requested seek offset.
    pub position_secs: f64,
    /// Rough post-volume RMS display level, not an analysis measurement.
    pub level: f32,
    /// Natural EOF after all admitted PCM has drained.
    pub finished: bool,
    /// Decode or device error. Never reinterpret it as natural completion.
    pub error: Option<String>,
}
/// Validate a seek before any decoder/device work; reject non-representable frame arithmetic.
pub fn validate_position(seconds: f64) -> Result<(), String> {
    if !seconds.is_finite() || seconds < 0. || seconds > u64::MAX as f64 / 384_000. {
        return Err("Invalid audio position.".into());
    }
    Ok(())
}
/// A local device stream and one bounded decoder worker, owned by the creating thread.
/// Drop stops future production; no callback locks, logging, IPC or track-sized buffer.
pub struct MinimalPlayer {
    _stream: cpal::Stream,
    worker: Option<std::thread::JoinHandle<()>>,
    signals: Arc<Signals>,
    gain: Arc<Gain>,
    path: PathBuf,
    rate: u32,
    offset: f64,
}
impl MinimalPlayer {
    /// Open the default device, initially paused; reading is progressive and bounded.
    pub fn open(path: &Path, gain: Arc<Gain>) -> Result<Self, String> {
        Self::open_admitted(path, gain, || Ok(()))
    }
    /// Recheck a captured owner between synchronous preparation stages and before publication.
    /// Retiring admission does not forcibly interrupt a filesystem/backend call already in flight.
    pub fn open_admitted(
        path: &Path,
        gain: Arc<Gain>,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        Self::open_at(path, gain, 0., &current)
    }
    fn open_at(
        path: &Path,
        gain: Arc<Gain>,
        offset: f64,
        current: &impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        current()?;
        let decoder = PcmStreamDecoder::open_for_playback(path).map_err(|e| e.to_string());
        current()?;
        let decoder = decoder?;
        let device = cpal::default_host()
            .default_output_device()
            .ok_or("No audio output device found.")?;
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        let config = if (1..=2).contains(&default.channels()) {
            default
        } else {
            device
                .supported_output_configs()
                .map_err(|e| e.to_string())?
                .filter(|c| (1..=2).contains(&c.channels()))
                .find_map(|c| c.try_with_sample_rate(default.sample_rate()))
                .ok_or("No mono/stereo output configuration available.")?
        };
        let rate = config.sample_rate();
        let channels = config.channels();
        if !(8_000..=384_000).contains(&rate) {
            return Err("Unsupported device sample rate.".into());
        }
        current()?;
        let converter = worker::Converter::new(&decoder, rate, channels)?;
        // The existing half-second Phémios budget, expressed in complete frames.
        let (producer, consumer) =
            crate::queue::pcm_queue::<f32>(channels as usize, (rate / 2).max(1) as usize, false)
                .map_err(|e| e.to_string())?;
        current()?;
        let signals = Arc::new(Signals::default());
        let stream = render::build(&device, config, consumer, signals.clone(), gain.clone())?;
        current()?;
        let owned = signals.clone();
        let worker = std::thread::Builder::new()
            .name("audio-preview-decode".into())
            .spawn(move || {
                if let Err(error) = worker::produce(decoder, converter, producer, offset, &owned)
                    && !owned.cancel.load(Ordering::Acquire)
                {
                    if let Ok(mut slot) = owned.error.lock() {
                        *slot = Some(error);
                    }
                    owned.failed.store(true, Ordering::Release);
                }
            })
            .map_err(|e| e.to_string())?;
        let player = Self {
            _stream: stream,
            worker: Some(worker),
            signals,
            gain,
            path: path.to_owned(),
            rate,
            offset,
        };
        current()?;
        player._stream.play().map_err(|e| e.to_string())?;
        current()?;
        Ok(player)
    }
    /// Suspend consumption immediately; already decoded bounded PCM is retained.
    pub fn pause(&self) {
        self.signals.paused.store(true, Ordering::Release);
    }
    /// Resume consumption without rebuilding the stream.
    pub fn resume(&self) {
        self.signals.paused.store(false, Ordering::Release);
    }
    /// Reopen the decoder and discard a cancellable prefix. No full-file PCM cache.
    /// Only this explicit seek replaces the stream; pause and gain are preserved.
    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        self.seek_admitted(seconds, || Ok(()))
    }
    /// Keep the caller's exact owner through preparation and before replacing/resuming the stream.
    pub fn seek_admitted(
        &mut self,
        seconds: f64,
        current: impl Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        validate_position(seconds)?;
        let path = self.path.clone();
        let gain = self.gain.clone();
        replace_prepared(
            &current,
            || Self::open_at(&path, gain, seconds, &current),
            |next| {
                install_prepared(
                    self,
                    next,
                    &current,
                    |player| player.signals.paused.load(Ordering::Acquire),
                    Self::pause,
                    Self::resume,
                )
            },
        )
    }
    /// No UI/framework work is performed by the callback; poll on the application's control thread.
    pub fn snapshot(&self) -> Snapshot {
        let failed = self.signals.failed.load(Ordering::Acquire);
        Snapshot {
            position_secs: self.offset
                + self.signals.consumed.load(Ordering::Acquire) as f64 / self.rate as f64,
            level: f32::from_bits(self.signals.level.load(Ordering::Relaxed)),
            finished: !failed && self.signals.finished.load(Ordering::Acquire),
            error: if failed {
                self.signals
                    .error
                    .lock()
                    .ok()
                    .and_then(|e| e.clone())
                    .or_else(|| Some("Audio output failed.".into()))
            } else {
                None
            },
        }
    }
}
impl Drop for MinimalPlayer {
    fn drop(&mut self) {
        self.signals.paused.store(true, Ordering::Release);
        self.signals.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // No retired worker accumulation. Stop/seek may wait for one regular-file decode read.
    }
}

// Keep the existing prepare/replace order testable without opening a physical device.
fn replace_prepared<T>(
    current: impl Fn() -> Result<(), String>,
    prepare: impl FnOnce() -> Result<T, String>,
    commit: impl FnOnce(T) -> Result<(), String>,
) -> Result<(), String> {
    current()?;
    let prepared = prepare();
    current()?;
    commit(prepared?)
}
// Replacing a visit drops/joins its previous worker, which can block after preparation admission.
fn install_prepared<T>(
    slot: &mut T,
    next: T,
    current: &impl Fn() -> Result<(), String>,
    paused: impl Fn(&T) -> bool,
    pause: impl Fn(&T),
    resume: impl Fn(&T),
) -> Result<(), String> {
    current()?;
    let was_paused = paused(slot);
    pause(slot);
    *slot = next;
    // Retirement joined the previous decoder; recheck before permitting any new audio.
    current()?;
    if !was_paused {
        resume(slot);
    }
    Ok(())
}
#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
