use super::*;
use aede_moods::background::{Admission, admission};
use jobs::NativeProgress;
use std::sync::atomic::AtomicBool;
pub(super) fn lock(
    state: &ApiState,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<std::sync::MutexGuard<'_, ()>, String> {
    loop {
        check()?;
        match state.audio_features.worker.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err("feature worker lock unavailable".into()),
        }
    }
}
pub(super) struct Control<'a> {
    state: &'a ApiState,
    cancel: &'a AtomicBool,
    shutdown: &'a mut crate::shutdown::Receiver,
    id: Option<u64>,
    pub(super) progress: NativeProgress,
    quantum: Instant,
    stopped: bool,
}
impl<'a> Control<'a> {
    pub(super) fn new(
        state: &'a ApiState,
        cancel: &'a AtomicBool,
        shutdown: &'a mut crate::shutdown::Receiver,
        id: Option<u64>,
        progress: NativeProgress,
    ) -> Self {
        Self {
            state,
            cancel,
            shutdown,
            id,
            progress,
            quantum: Instant::now(),
            stopped: false,
        }
    }
    fn phase(&mut self, value: &'static str) -> Result<(), String> {
        if self.progress.phase != value {
            self.progress.phase = value;
            self.publish()?;
        }
        Ok(())
    }
    pub(super) fn publish(&self) -> Result<(), String> {
        if let Some(id) = self.id {
            jobs::progress(self.state, id, self.progress.clone())?;
        }
        Ok(())
    }
    pub(super) fn checkpoint(&mut self) -> Result<(), String> {
        loop {
            self.stopped |= self.cancel.load(Ordering::Acquire)
                || self.state.audio_features.stopping.load(Ordering::Acquire)
                || matches!(
                    self.shutdown.try_recv(),
                    Ok(()) | Err(broadcast::error::TryRecvError::Closed)
                );
            let stopped = self.stopped;
            let leases = self
                .state
                .audio_features
                .leases
                .lock()
                .map_err(|_| "foreground state unavailable")?
                .values()
                .any(|time| time.elapsed() < Duration::from_secs(6));
            let foreground =
                self.state.playback_slots.available_permits() < MAX_PLAYBACKS || leases;
            match admission(stopped, foreground, self.quantum.elapsed()) {
                Admission::Stop => return Err("acoustic task cancelled".into()),
                Admission::Foreground => {
                    self.phase("waiting_for_playback")?;
                    std::thread::sleep(Duration::from_millis(50));
                    self.quantum = Instant::now();
                }
                Admission::Yield => {
                    std::thread::sleep(Duration::from_millis(15));
                    self.quantum = Instant::now();
                }
                Admission::Work => {
                    self.phase("analyzing")?;
                    return Ok(());
                }
            }
        }
    }
}
pub(super) fn run(
    state: ApiState,
    cancel: Arc<AtomicBool>,
    id: u64,
    force: bool,
) -> Result<jobs::JobOutput, String> {
    let mut shutdown = state.shutdown.subscribe();
    let mut control = Control::new(&state, &cancel, &mut shutdown, Some(id), Default::default());
    let (epoch, sources) = sources(&state)?;
    control.progress.total = sources.len();
    control.publish()?;
    let _worker = match lock(&state, || control.checkpoint()) {
        Ok(guard) => guard,
        Err(_) if control.stopped => {
            return Ok(jobs::JobOutput {
                exit_code: 130,
                ..Default::default()
            });
        }
        Err(error) => return Err(error),
    };
    let mut failures = Vec::new();
    for source in sources {
        if let Err(error) = control.checkpoint() {
            if !control.stopped {
                return Err(error);
            }
            return Ok(jobs::JobOutput {
                exit_code: 130,
                ..Default::default()
            });
        }
        let result: Result<(Record, bool), String> = (|| {
            let source = source.as_ref().map_err(Clone::clone)?;
            let input = identity(source);
            playback_api::validate_source(source)
                .map_err(|_| "audio source changed or unavailable")?;
            if !force
                && let Some(record) = aede_core::audio_features::load(&state.data_dir, &input)?
            {
                return Ok((record, true));
            }
            let features =
                aede_core::audio_features::extract(&source.path, || control.checkpoint())?;
            control.checkpoint()?;
            playback_api::validate_source(source)
                .map_err(|_| "audio source changed during extraction")?;
            let catalog = state.catalog.blocking_read();
            if catalog.as_ref().is_none_or(|c| {
                c.file(source.file.id)
                    .filter(|f| f.path == input.path)
                    .or_else(|| c.files.iter().find(|f| f.path == input.path))
                    .is_none_or(|f| {
                        f.size != input.size
                            || f.mtime != input.modified
                            || c.file_mtime_subseconds.get(&f.path) != Some(&input.nanos)
                    })
            }) {
                return Err("catalog source changed during extraction".into());
            }
            let record = Record {
                input: input.clone(),
                features,
            };
            drop(catalog);
            aede_core::audio_features::save(&state.data_dir, &record)?;
            Ok((record, false))
        })();
        if control.stopped || cancel.load(Ordering::Acquire) {
            return Ok(jobs::JobOutput {
                exit_code: 130,
                ..Default::default()
            });
        }
        match result {
            Ok((record, reused)) => {
                let mut facts = state
                    .audio_features
                    .facts
                    .lock()
                    .map_err(|_| "feature state unavailable")?;
                if facts.rows.get(&record.input.path) != Some(&record) {
                    facts.revision = facts.revision.saturating_add(1);
                }
                facts.rows.insert(record.input.path.clone(), record);
                if reused {
                    control.progress.reused += 1;
                } else {
                    control.progress.analyzed += 1;
                }
            }
            Err(message) => {
                control.progress.failed += 1;
                if failures.len() < 8 {
                    failures.push(match &source {
                        Ok(source) => format!(
                            "{}: {}",
                            source
                                .path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .chars()
                                .take(128)
                                .collect::<String>(),
                            message.chars().take(512).collect::<String>()
                        ),
                        Err(_) => message.chars().take(512).collect(),
                    });
                }
            }
        }
        control.progress.done += 1;
        control.publish()?;
    }
    if let Ok(mut facts) = state.audio_features.facts.lock() {
        facts.epoch = Some(epoch);
    }
    control.phase("finished")?;
    Ok(jobs::JobOutput {
        exit_code: if control.progress.failed > 0 { 1 } else { 0 },
        stdout: format!(
            "{} analyzed, {} reused, {} unavailable",
            control.progress.analyzed, control.progress.reused, control.progress.failed
        ),
        stderr: failures.join("\n"),
        ..Default::default()
    })
}
