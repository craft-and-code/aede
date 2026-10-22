use super::*;
use aede_moods::background::{Admission, admission};
use jobs::NativeProgress;
use std::sync::atomic::AtomicBool;

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;

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
    mut state: ApiState,
    cancel: Arc<AtomicBool>,
    id: u64,
    force: bool,
) -> Result<jobs::JobOutput, String> {
    if let Some(scope) = &state.parental_scope {
        state.catalog = scope.shared_catalog.clone();
    }
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
    synchronize(&state, None)?;
    let outcome = (|| {
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
                if catalog
                    .as_ref()
                    .is_none_or(|catalog| !matches_source(catalog, &input, source.file.id))
                {
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
            let result = result.and_then(|(record, reused)| {
                let source = source.as_ref().map_err(Clone::clone)?;
                publish(&state, source, record)?;
                Ok(reused)
            });
            match result {
                Ok(reused) => {
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
    })();
    let completed = outcome.as_ref().is_ok_and(|output| output.exit_code != 130);
    synchronize(&state, completed.then_some(epoch))?;
    outcome
}

fn matches_source(catalog: &Catalog, input: &InputIdentity, file_id: Id) -> bool {
    catalog.files.len() <= MAX_FILES
        && catalog
            .file(file_id)
            .filter(|file| file.path == input.path)
            .or_else(|| catalog.files.iter().find(|file| file.path == input.path))
            .is_some_and(|file| {
                file.size == input.size
                    && file.mtime == input.modified
                    && catalog.file_mtime_subseconds.get(&file.path) == Some(&input.nanos)
            })
}

fn publish(
    state: &ApiState,
    source: &playback_api::TrackSource,
    record: Record,
) -> Result<(), String> {
    let guard = state.catalog.blocking_read();
    if guard
        .as_ref()
        .is_none_or(|catalog| !matches_source(catalog, &record.input, source.file.id))
    {
        return Err("catalog source changed before feature publication".into());
    }
    playback_api::validate_source(source).map_err(|_| "audio source changed before publication")?;
    let mut facts = state
        .audio_features
        .facts
        .lock()
        .map_err(|_| "feature state unavailable")?;
    if facts.rows.get(&record.input.path) != Some(&record) {
        facts.revision = facts.revision.saturating_add(1);
    }
    facts.rows.insert(record.input.path.clone(), record);
    Ok(())
}

fn synchronize(state: &ApiState, completed_epoch: Option<u64>) -> Result<(), String> {
    let guard = state.catalog.blocking_read();
    let catalog = guard
        .as_ref()
        .filter(|catalog| catalog.files.len() <= MAX_FILES);
    let identities = catalog
        .into_iter()
        .flat_map(|catalog| &catalog.files)
        .map(|file| (file.path.as_str(), (file.size, file.mtime)))
        .collect::<BTreeMap<_, _>>();
    let mut facts = state
        .audio_features
        .facts
        .lock()
        .map_err(|_| "feature state unavailable")?;
    let previous = facts.rows.len();
    // Pruning is bounded by the active catalogue and never removes durable work.
    facts.rows.retain(|path, record| {
        identities.get(path.as_str()) == Some(&(record.input.size, record.input.modified))
            && catalog.is_some_and(|catalog| {
                catalog.file_mtime_subseconds.get(path) == Some(&record.input.nanos)
            })
    });
    if facts.rows.len() != previous {
        facts.revision = facts.revision.saturating_add(1);
    }
    facts.epoch =
        completed_epoch.filter(|epoch| catalog.is_some_and(|catalog| signature(catalog) == *epoch));
    Ok(())
}
