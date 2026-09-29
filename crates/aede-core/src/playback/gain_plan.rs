//! Normalization policy shared by local and future remote playback.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::conclusions::{self, Conclusions};
use crate::model::Catalog;
use crate::playback::loudness::{self, CachedProgramme, CachedTrack, Measurement, ProgrammeFile};
use crate::playback::normalization::{self, Mode as NormalizationMode, Source};
use crate::store_lock::StoreLock;
use crate::tags;

/// The gain chosen before the DSP enforces output headroom.
#[derive(Clone, Copy)]
pub struct GainPlan {
    /// Requested gain before output headroom is applied.
    pub gain_db: f32,
    /// Linear peak of the unamplified source, when known.
    pub source_peak: Option<f32>,
    /// Origin of the loudness decision.
    pub label: &'static str,
    /// Origin of the peak used for headroom.
    pub peak_label: &'static str,
}

fn metadata_plan(selection: normalization::Selection) -> GainPlan {
    let (label, peak_label) = match selection.source {
        Source::ReplayGainTrack => ("ReplayGain track", "ReplayGain peak"),
        Source::ReplayGainAlbum => ("ReplayGain album", "ReplayGain peak"),
        Source::OpusR128Track => ("Opus R128 track", "assumed full-scale peak"),
        Source::OpusR128Album => ("Opus R128 album", "assumed full-scale peak"),
    };
    GainPlan {
        gain_db: selection.gain_db,
        source_peak: selection.source_peak,
        label,
        peak_label,
    }
}

fn measured_plan(measurement: Measurement, album: bool, imported: bool) -> GainPlan {
    GainPlan {
        gain_db: normalization::DEFAULT_TARGET_LUFS - measurement.integrated_lufs,
        source_peak: measurement.true_peak,
        label: if album {
            "measured album"
        } else if imported {
            "FlacCompagnon track"
        } else {
            "measured track"
        },
        peak_label: if measurement.true_peak.is_some() {
            "measured true peak"
        } else {
            "assumed full-scale peak"
        },
    }
}

fn is_requested_scope(source: Source, mode: NormalizationMode) -> bool {
    matches!(
        (source, mode),
        (
            Source::ReplayGainTrack | Source::OpusR128Track,
            NormalizationMode::Track
        ) | (
            Source::ReplayGainAlbum | Source::OpusR128Album,
            NormalizationMode::Album
        )
    )
}

fn album_groups(paths: &[PathBuf], is_album: bool, catalog: Option<&Catalog>) -> Vec<Vec<usize>> {
    if !is_album {
        return vec![(0..paths.len()).collect()];
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut previous = None;
    for (index, path) in paths.iter().enumerate() {
        let release = catalog.and_then(|catalog| {
            catalog.tracks.iter().find_map(|track| {
                (catalog.file(track.file_id)?.path == path.to_string_lossy())
                    .then_some(track.release_id)
                    .flatten()
            })
        });
        if groups.is_empty() || (index > 0 && release != previous) {
            groups.push(Vec::new());
        }
        if let Some(group) = groups.last_mut() {
            group.push(index);
        }
        previous = release;
    }
    groups
}

/// Select and cache normalization gain for an ordered playback selection.
/// This policy is shared by the terminal and future remote audio handlers.
pub fn plan_normalization(
    paths: &[PathBuf],
    is_album: bool,
    catalog: Option<&Catalog>,
    directory: &Path,
    mode: NormalizationMode,
) -> Result<Vec<Option<GainPlan>>, Box<dyn Error>> {
    let mut plans = vec![None; paths.len()];
    if mode == NormalizationMode::Off {
        return Ok(plans);
    }
    let identities = paths
        .iter()
        .map(|path| loudness::identity(path))
        .collect::<Result<Vec<_>, _>>()?;
    let tags = paths
        .iter()
        .map(|path| tags::read(path))
        .collect::<Result<Vec<_>, _>>()?;
    let metadata = tags
        .iter()
        .map(|raw| normalization::select_raw(raw, mode, normalization::DEFAULT_TARGET_LUFS))
        .collect::<Result<Vec<_>, _>>()?;
    let cache_path = conclusions::conclusions_path(directory);
    let cache = conclusions::load(&cache_path)?.unwrap_or_default();
    let mut new_tracks: BTreeMap<String, CachedTrack> = BTreeMap::new();
    let mut new_programmes = Vec::new();
    if mode == NormalizationMode::Track {
        for (index, file) in identities.iter().enumerate() {
            if let Some(gain) = metadata[index].filter(|gain| is_requested_scope(gain.source, mode))
            {
                plans[index] = Some(metadata_plan(gain));
                continue;
            }
            let imported = loudness::from_flaccompagnon(&cache.analyses, file).or_else(|| {
                catalog.and_then(|catalog| loudness::from_flaccompagnon(&catalog.analyses, file))
            });
            let measurement = if let Some(measurement) = imported {
                Some(measurement)
            } else if let Some(cached) = new_tracks
                .get(&file.path)
                .or_else(|| cache.loudness_tracks.get(&file.path))
                .filter(|cached| cached.size == file.size && cached.mtime == file.mtime)
            {
                cached.measurement
            } else {
                let measured = loudness::measure_track(&paths[index])?;
                new_tracks.insert(
                    file.path.clone(),
                    CachedTrack {
                        size: file.size,
                        mtime: file.mtime,
                        measurement: measured,
                    },
                );
                measured
            };
            plans[index] = measurement
                .map(|value| measured_plan(value, false, imported.is_some()))
                .or_else(|| metadata[index].map(metadata_plan));
        }
    } else {
        for group in album_groups(paths, is_album, catalog) {
            let has_complete_album_tags = group.iter().all(|&index| {
                metadata[index].is_some_and(|gain| is_requested_scope(gain.source, mode))
            });
            if has_complete_album_tags {
                for &index in &group {
                    plans[index] = metadata[index].map(metadata_plan);
                }
                continue;
            }
            let files = group
                .iter()
                .map(|&index| identities[index].clone())
                .collect::<Vec<ProgrammeFile>>();
            let measurement = if let Some(cached) = cache
                .loudness_programmes
                .iter()
                .find(|cached| cached.files == files)
            {
                cached.measurement
            } else {
                let paths = group
                    .iter()
                    .map(|&index| paths[index].as_path())
                    .collect::<Vec<_>>();
                let measured = loudness::measure_programme(&paths)?;
                new_programmes.push(CachedProgramme {
                    files,
                    measurement: measured,
                });
                measured
            };
            for &index in &group {
                plans[index] = measurement
                    .map(|value| measured_plan(value, true, false))
                    .or_else(|| metadata[index].map(metadata_plan));
            }
        }
    }
    if !new_tracks.is_empty() || !new_programmes.is_empty() {
        let _lock = StoreLock::acquire(directory)?;
        let mut current = conclusions::load(&cache_path)?
            .unwrap_or_else(|| catalog.map(Conclusions::from_catalog).unwrap_or_default());
        current.loudness_tracks.extend(new_tracks);
        for item in new_programmes {
            current.loudness_programmes.retain(|cached| {
                cached
                    .files
                    .iter()
                    .map(|file| &file.path)
                    .ne(item.files.iter().map(|file| &file.path))
            });
            current.loudness_programmes.push(item);
        }
        conclusions::save(&current, &cache_path)?;
    }
    Ok(plans)
}
