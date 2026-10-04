//! Playback chooses known gains without decoding the selection in advance.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aede_dsp::{
    PcmFormat,
    loudness::{LoudnessError, LoudnessProgramme},
};

use super::{GainPlan, album_groups, is_requested_scope, measured_plan, metadata_plan};
use crate::conclusions::{self, Conclusions};
use crate::model::Catalog;
use crate::playback::loudness::{self, CachedProgramme, CachedTrack, ProgrammeFile};
use crate::playback::normalization::{self, Mode as NormalizationMode};
use crate::store_lock::StoreLock;
use crate::tags;

/// A completed source measurement, saved outside the audio decoding loop.
pub struct LoudnessUpdate {
    track: Option<(ProgrammeFile, CachedTrack)>,
    programme: Option<CachedProgramme>,
    legacy_seed: Option<Arc<Conclusions>>,
}

impl LoudnessUpdate {
    /// Merge into the latest store under its writer lock. A source changed
    /// after decoding is rejected instead of publishing a stale measurement.
    pub fn save(&self, directory: &Path) -> Result<(), Box<dyn Error>> {
        let _lock = StoreLock::acquire(directory)?;
        let files = self
            .track
            .as_ref()
            .map(|(file, _)| std::slice::from_ref(file))
            .or_else(|| self.programme.as_ref().map(|item| item.files.as_slice()))
            .unwrap_or_default();
        for file in files {
            if loudness::identity(Path::new(&file.path))? != *file {
                return Err("audio file changed before its loudness could be cached".into());
            }
        }
        let path = conclusions::conclusions_path(directory);
        let mut current = conclusions::load(&path)?
            .unwrap_or_else(|| self.legacy_seed.as_deref().cloned().unwrap_or_default());
        if let Some((file, track)) = &self.track {
            current
                .loudness_tracks
                .insert(file.path.clone(), track.clone());
        }
        if let Some(programme) = &self.programme {
            replace_programme(&mut current, programme.clone());
        }
        conclusions::save(&current, &path)?;
        Ok(())
    }
}

fn replace_programme(cache: &mut Conclusions, item: CachedProgramme) {
    cache.loudness_programmes.retain(|cached| {
        cached
            .files
            .iter()
            .map(|file| &file.path)
            .ne(item.files.iter().map(|file| &file.path))
    });
    cache.loudness_programmes.push(item);
}

struct Capture {
    indices: Vec<usize>,
    files: Vec<ProgrammeFile>,
    next: usize,
    meter: Option<LoudnessProgramme>,
}

/// A lazy normalization session shared by CLI and native PCM playback.
///
/// Only the current track (or current album's tags/identities) is inspected.
/// The CLI can measure missing loudness from source PCM already being decoded;
/// native PCM playback only reuses existing gains and does not enable capture.
/// An album decision is frozen for the session, never changed midway through it.
pub struct ReadyNormalization<'a> {
    paths: &'a [PathBuf],
    catalog: Option<&'a Catalog>,
    mode: NormalizationMode,
    cache: Conclusions,
    legacy_seed: Option<Arc<Conclusions>>,
    groups: Vec<Vec<usize>>,
    prepared: Vec<bool>,
    learnable_album_starts: Vec<bool>,
    plans: Vec<Option<GainPlan>>,
    capture: Option<Capture>,
    capture_enabled: bool,
}

impl<'a> ReadyNormalization<'a> {
    /// Load available measurements without reading audio files or their tags.
    pub fn new(
        paths: &'a [PathBuf],
        is_album: bool,
        catalog: Option<&'a Catalog>,
        directory: &Path,
        mode: NormalizationMode,
    ) -> Result<Self, Box<dyn Error>> {
        let loaded = if mode == NormalizationMode::Off {
            Some(Conclusions::default())
        } else {
            conclusions::load(&conclusions::conclusions_path(directory))?
        };
        let legacy_seed = if loaded.is_none() {
            catalog.map(|catalog| Arc::new(Conclusions::from_catalog(catalog)))
        } else {
            None
        };
        let cache = loaded.unwrap_or_else(|| legacy_seed.as_deref().cloned().unwrap_or_default());
        Ok(Self {
            paths,
            catalog,
            mode,
            cache,
            legacy_seed,
            groups: if mode == NormalizationMode::Album {
                album_groups(paths, is_album, catalog)
            } else {
                Vec::new()
            },
            prepared: vec![false; paths.len()],
            learnable_album_starts: vec![false; paths.len()],
            plans: vec![None; paths.len()],
            capture: None,
            capture_enabled: true,
        })
    }

    /// Reuse known gains without measuring or publishing new source loudness.
    ///
    /// Native PCM transport selects this policy because it does not observe
    /// source samples or publish complete-track measurements. Missing track
    /// gains retain the existing fallback policy; missing album gain retains
    /// one unchanged level. No meter or capture is prepared for unknown values.
    /// Calling this after preparation discards any pending optional capture.
    pub fn without_capture(mut self) -> Self {
        self.capture_enabled = false;
        self.capture = None;
        self.learnable_album_starts.fill(false);
        self
    }

    /// Choose an already available gain for the current track or album.
    /// Missing source measurements are captured later through `observe_source`.
    pub fn prepare(&mut self, index: usize) -> Result<Option<GainPlan>, Box<dyn Error>> {
        let path = self
            .paths
            .get(index)
            .ok_or("playback track index is out of range")?;
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.indices.get(capture.next) != Some(&index))
        {
            self.capture = None;
        }
        if self.mode == NormalizationMode::Off {
            return Ok(None);
        }
        if self.mode == NormalizationMode::Album {
            if !self.prepared[index] {
                self.prepare_album(index)?;
            } else if self.capture.is_none() && self.learnable_album_starts[index] {
                let indices = self
                    .groups
                    .iter()
                    .find(|group| group.first() == Some(&index))
                    .ok_or("playback album group is missing")?
                    .clone();
                let files = indices
                    .iter()
                    .map(|&index| loudness::identity(&self.paths[index]))
                    .collect::<Result<Vec<_>, _>>()?;
                // Restart learning only from the album's beginning; its gain
                // decision remains frozen even after a complete measurement.
                self.capture = Some(Capture {
                    indices,
                    files,
                    next: 0,
                    meter: Some(LoudnessProgramme::new()),
                });
            }
            return Ok(self.plans[index]);
        }
        let file = loudness::identity(path)?;
        let raw = tags::read(path)?;
        let metadata =
            normalization::select_raw(&raw, self.mode, normalization::DEFAULT_TARGET_LUFS)?;
        if let Some(gain) = metadata.filter(|gain| is_requested_scope(gain.source, self.mode)) {
            self.capture = None;
            return Ok(Some(metadata_plan(gain)));
        }
        let imported = loudness::from_flaccompagnon(&self.cache.analyses, &file).or_else(|| {
            self.catalog
                .and_then(|catalog| loudness::from_flaccompagnon(&catalog.analyses, &file))
        });
        if let Some(measurement) = imported {
            self.capture = None;
            return Ok(Some(measured_plan(measurement, false, true)));
        }
        if let Some(cached) = self
            .cache
            .loudness_tracks
            .get(&file.path)
            .filter(|cached| cached.matches(&file))
        {
            self.capture = None;
            return Ok(cached
                .measurement
                .map(|value| measured_plan(value, false, false))
                .or_else(|| metadata.map(metadata_plan)));
        }
        if self.capture_enabled {
            self.capture = Some(Capture {
                indices: vec![index],
                files: vec![file],
                next: 0,
                meter: Some(LoudnessProgramme::new()),
            });
        }
        Ok(metadata.map(metadata_plan))
    }

    fn prepare_album(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        let indices = self
            .groups
            .iter()
            .find(|group| group.contains(&index))
            .ok_or("playback album group is missing")?
            .clone();
        let files = indices
            .iter()
            .map(|&index| loudness::identity(&self.paths[index]))
            .collect::<Result<Vec<_>, _>>()?;
        let metadata = indices
            .iter()
            .map(|&index| {
                let raw = tags::read(&self.paths[index])?;
                normalization::select_raw(&raw, self.mode, normalization::DEFAULT_TARGET_LUFS)
                    .map_err(|error| Box::new(error) as Box<dyn Error>)
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        if metadata
            .iter()
            .all(|gain| gain.is_some_and(|gain| is_requested_scope(gain.source, self.mode)))
        {
            for (&index, gain) in indices.iter().zip(metadata) {
                self.plans[index] = gain.map(metadata_plan);
            }
        } else if let Some(cached) = self
            .cache
            .loudness_programmes
            .iter()
            .find(|cached| cached.files == files)
        {
            let plan = cached
                .measurement
                .map(|value| measured_plan(value, true, false));
            for &index in &indices {
                self.plans[index] = plan;
            }
        } else if self.capture_enabled
            && let Some(&first) = indices.first()
        {
            // Incomplete album metadata must not turn album mode into a series
            // of per-track adjustments. Learn one programme for a later play.
            self.learnable_album_starts[first] = true;
            if first == index {
                self.capture = Some(Capture {
                    indices: indices.clone(),
                    files,
                    next: 0,
                    meter: Some(LoudnessProgramme::new()),
                });
            }
        }
        for index in indices {
            self.prepared[index] = true;
        }
        Ok(())
    }

    /// Whether an active meter is capturing source PCM for a later decision.
    pub fn is_measuring(&self) -> bool {
        self.capture
            .as_ref()
            .is_some_and(|capture| capture.meter.is_some())
    }

    /// Observe decoded source PCM before downmix, rate conversion and effects.
    /// Unsupported layouts retain an unavailable result only after a complete
    /// decode. Other meter failures abandon the optional capture.
    pub fn observe_source(
        &mut self,
        format: PcmFormat,
        samples: &[f32],
    ) -> Result<(), Box<dyn Error>> {
        if let Some(capture) = &mut self.capture
            && let Some(meter) = &mut capture.meter
            && let Err(error) = meter.push(format, samples)
        {
            if matches!(error, LoudnessError::UnsupportedLayout) {
                capture.meter = None;
            } else {
                self.capture = None;
            }
            return Err(error.into());
        }
        Ok(())
    }

    /// Publish only fully decoded tracks/programmes with unchanged sources.
    pub fn finish_track(
        &mut self,
        index: usize,
        complete: bool,
    ) -> Result<Option<LoudnessUpdate>, Box<dyn Error>> {
        let Some(mut capture) = self.capture.take() else {
            return Ok(None);
        };
        if !complete || capture.indices.get(capture.next) != Some(&index) {
            return Ok(None);
        }
        let file = &capture.files[capture.next];
        if loudness::identity(Path::new(&file.path))? != *file {
            return Err("audio file changed while its loudness was measured".into());
        }
        capture.next += 1;
        if capture.next < capture.indices.len() {
            self.capture = Some(capture);
            return Ok(None);
        }
        let measurement = capture
            .meter
            .as_ref()
            .map(LoudnessProgramme::measurement)
            .transpose()?
            .flatten();
        let mut update = LoudnessUpdate {
            track: None,
            programme: None,
            legacy_seed: self.legacy_seed.clone(),
        };
        if self.mode == NormalizationMode::Track {
            let file = capture.files.remove(0);
            let cached = CachedTrack {
                size: file.size,
                mtime: file.mtime,
                mtime_subseconds: file.mtime_subseconds,
                measurement,
            };
            self.cache
                .loudness_tracks
                .insert(file.path.clone(), cached.clone());
            update.track = Some((file, cached));
        } else {
            if let Some(&first) = capture.indices.first() {
                self.learnable_album_starts[first] = false;
            }
            let cached = CachedProgramme {
                files: capture.files,
                measurement,
            };
            replace_programme(&mut self.cache, cached.clone());
            update.programme = Some(cached);
        }
        Ok(Some(update))
    }
}

#[cfg(test)]
#[path = "gain_plan_ready_tests.rs"]
mod tests;
