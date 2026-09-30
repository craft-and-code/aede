//! Costly conclusions about audio bytes, independent of the rebuildable catalog.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::analysis::FileAnalysis;
use crate::fingerprint::Fingerprint;
use crate::json::{self, Json};
use crate::model::{Catalog, IntegrityRecord};
use crate::playback::loudness::{CachedProgramme, CachedTrack, Measurement, ProgrammeFile};
use crate::store::{self, StoreError};

/// Independent on-disk format version for the conclusions store.
pub const FORMAT_VERSION: u32 = 1;
// Version 4 includes corrected native Vorbis bounds; older derived values may
// measure truncated audio or padding. Imported analyses have their own provenance.
const LOUDNESS_METHOD_VERSION: u32 = 4;
/// File name inside Aède's data directory.
pub const CONCLUSIONS_FILE: &str = "conclusions.json";

/// Conclusions keyed by stable paths rather than scan-local identifiers.
#[derive(Debug, Clone, Default)]
pub struct Conclusions {
    /// Path-keyed results, including results waiting for a file to return.
    pub files: BTreeMap<String, FileConclusion>,
    /// Imported measurements, including those waiting for a file to appear.
    pub analyses: Vec<FileAnalysis>,
    /// Playback loudness measurements keyed by file path and byte identity.
    pub loudness_tracks: BTreeMap<String, CachedTrack>,
    /// Album programme measurements keyed by their ordered file identities.
    pub loudness_programmes: Vec<CachedProgramme>,
}

/// Results tied to a particular version of a file's bytes.
#[derive(Debug, Clone)]
pub struct FileConclusion {
    /// File size when these results were produced.
    pub size: u64,
    /// Modification time when these results were produced.
    pub mtime: u64,
    /// The most recent integrity check.
    pub integrity: Option<IntegrityRecord>,
    /// The computed acoustic fingerprint.
    pub fingerprint: Option<Fingerprint>,
}

/// Returns the conclusions file inside a data directory.
pub fn conclusions_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CONCLUSIONS_FILE)
}

impl Conclusions {
    /// Extracts the non-derivable portion of a legacy catalog or a backup.
    pub fn from_catalog(catalog: &Catalog) -> Self {
        let mut result = Self::default();
        result.update_from_catalog(catalog);
        result
    }

    /// Current files are authoritative; absent paths are kept for later scans.
    pub fn update_from_catalog(&mut self, catalog: &Catalog) {
        for file in &catalog.files {
            self.files.remove(&file.path);
            if file.integrity.is_some() || file.fingerprint.is_some() {
                self.files.insert(
                    file.path.clone(),
                    FileConclusion {
                        size: file.size,
                        mtime: file.mtime,
                        integrity: file.integrity.clone(),
                        fingerprint: file.fingerprint.clone(),
                    },
                );
            }
        }
        self.analyses = catalog.analyses.clone();
    }

    /// Imports legacy fields only when no independent store exists yet.
    pub fn merge_legacy(&mut self, catalog: &Catalog) -> bool {
        let legacy = Self::from_catalog(catalog);
        let changed = !legacy.files.is_empty() || !legacy.analyses.is_empty();
        if changed {
            *self = legacy;
        }
        changed
    }

    /// Matches results to bytes, never to scan-local numeric identifiers.
    pub fn attach(&self, catalog: &mut Catalog) {
        for file in &mut catalog.files {
            file.integrity = None;
            file.fingerprint = None;
            if let Some(record) = self.files.get(&file.path)
                && record.size == file.size
                && record.mtime == file.mtime
            {
                file.integrity = record.integrity.clone();
                file.fingerprint = record.fingerprint.clone();
            }
        }
        catalog.analyses = self.analyses.clone();
    }
}

/// Serializes the independent store.
pub fn to_json(conclusions: &Conclusions) -> Json {
    let mut root = Json::obj();
    root.set("format_version", FORMAT_VERSION.into());
    root.set("loudness_method_version", LOUDNESS_METHOD_VERSION.into());
    let mut files = Vec::with_capacity(conclusions.files.len());
    for (path, record) in &conclusions.files {
        let mut row = Json::obj();
        row.set("path", path.clone().into());
        row.set("size", record.size.into());
        row.set("mtime", record.mtime.into());
        if let Some(verdict) = &record.integrity {
            let mut value = Json::obj();
            value.set("state", verdict.verdict.key().into());
            value.set("method", verdict.method.clone().into());
            value.set("checked_at", verdict.checked_at.into());
            if let crate::audit::integrity::Verdict::Damaged { detail } = &verdict.verdict {
                value.set("detail", detail.clone().into());
            }
            row.set("integrity", value);
        }
        if let Some(print) = &record.fingerprint {
            let mut value = Json::obj();
            value.set("data", print.data.clone().into());
            value.set("seconds", u64::from(print.seconds).into());
            row.set("fingerprint", value);
        }
        files.push(row);
    }
    root.set("file", Json::Arr(files));
    root.set(
        "analysis",
        Json::Arr(
            conclusions
                .analyses
                .iter()
                .map(store::analysis_to_json)
                .collect(),
        ),
    );
    root.set(
        "loudness_track",
        Json::Arr(
            conclusions
                .loudness_tracks
                .iter()
                .map(|(path, cached)| {
                    let mut row = Json::obj();
                    row.set("path", path.clone().into());
                    row.set("size", cached.size.into());
                    row.set("mtime", cached.mtime.into());
                    row.set("measurement", measurement_json(cached.measurement));
                    row
                })
                .collect(),
        ),
    );
    root.set(
        "loudness_programme",
        Json::Arr(
            conclusions
                .loudness_programmes
                .iter()
                .map(|cached| {
                    let mut row = Json::obj();
                    row.set(
                        "file",
                        Json::Arr(
                            cached
                                .files
                                .iter()
                                .map(|file| {
                                    let mut value = Json::obj();
                                    value.set("path", file.path.clone().into());
                                    value.set("size", file.size.into());
                                    value.set("mtime", file.mtime.into());
                                    value
                                })
                                .collect(),
                        ),
                    );
                    row.set("measurement", measurement_json(cached.measurement));
                    row
                })
                .collect(),
        ),
    );
    root
}

fn measurement_json(measurement: Option<Measurement>) -> Json {
    let Some(measurement) = measurement else {
        return Json::Null;
    };
    let mut value = Json::obj();
    value.set(
        "integrated_lufs",
        (measurement.integrated_lufs as f64).into(),
    );
    if let Some(peak) = measurement.true_peak {
        value.set("true_peak", (peak as f64).into());
    }
    value
}

fn measurement_from_json(value: Option<&Json>) -> Result<Option<Measurement>, StoreError> {
    let Some(value) = value else {
        return Err(StoreError::ConclusionsInvalid(
            "missing loudness measurement",
        ));
    };
    if *value == Json::Null {
        return Ok(None);
    }
    let loudness = value
        .field_f64("integrated_lufs")
        .ok_or(StoreError::ConclusionsInvalid("invalid loudness"))?;
    let peak = value.field_f64("true_peak");
    if value.get("true_peak").is_some() && peak.is_none() {
        return Err(StoreError::ConclusionsInvalid("invalid true peak"));
    }
    if !loudness.is_finite()
        || peak.is_some_and(|peak| !peak.is_finite() || peak < 0.0 || peak > f32::MAX as f64)
        || !(-100.0..=20.0).contains(&loudness)
    {
        return Err(StoreError::ConclusionsInvalid(
            "out-of-range loudness measurement",
        ));
    }
    Ok(Some(Measurement {
        integrated_lufs: loudness as f32,
        true_peak: peak.map(|peak| peak as f32),
    }))
}

/// Parses the independent store, refusing an incompatible version.
pub fn from_json(value: &Json) -> Result<Conclusions, StoreError> {
    let found = value.field_u32("format_version").unwrap_or(0);
    if found != FORMAT_VERSION {
        return Err(StoreError::ConclusionsVersion {
            found,
            expected: FORMAT_VERSION,
        });
    }
    let mut result = Conclusions::default();
    for row in value.get("file").and_then(Json::as_arr).unwrap_or(&[]) {
        let path = row
            .field_str("path")
            .ok_or(StoreError::ConclusionsInvalid("conclusion without path"))?;
        let size = row
            .field_u64("size")
            .ok_or(StoreError::ConclusionsInvalid("conclusion without size"))?;
        let mtime = row
            .field_u64("mtime")
            .ok_or(StoreError::ConclusionsInvalid("conclusion without mtime"))?;
        let integrity = store::integrity_from_json(row.get("integrity"));
        let fingerprint = store::fingerprint_from_json(row.get("fingerprint"));
        if row
            .get("integrity")
            .is_some_and(|value| *value != Json::Null)
            && integrity.is_none()
        {
            return Err(StoreError::ConclusionsInvalid(
                "unreadable integrity verdict",
            ));
        }
        if row
            .get("fingerprint")
            .is_some_and(|value| *value != Json::Null)
            && fingerprint.is_none()
        {
            return Err(StoreError::ConclusionsInvalid("unreadable fingerprint"));
        }
        let record = FileConclusion {
            size,
            mtime,
            integrity,
            fingerprint,
        };
        if result.files.insert(path, record).is_some() {
            return Err(StoreError::ConclusionsInvalid("duplicate conclusion path"));
        }
    }
    for row in value.get("analysis").and_then(Json::as_arr).unwrap_or(&[]) {
        result.analyses.push(store::analysis_from_json(row));
    }
    if value.field_u32("loudness_method_version") == Some(LOUDNESS_METHOD_VERSION) {
        for row in value
            .get("loudness_track")
            .and_then(Json::as_arr)
            .unwrap_or(&[])
        {
            let path = row
                .field_str("path")
                .ok_or(StoreError::ConclusionsInvalid(
                    "loudness track without path",
                ))?
                .to_owned();
            let cached = CachedTrack {
                size: row.field_u64("size").ok_or(StoreError::ConclusionsInvalid(
                    "loudness track without size",
                ))?,
                mtime: row
                    .field_u64("mtime")
                    .ok_or(StoreError::ConclusionsInvalid(
                        "loudness track without mtime",
                    ))?,
                measurement: measurement_from_json(row.get("measurement"))?,
            };
            if result.loudness_tracks.insert(path, cached).is_some() {
                return Err(StoreError::ConclusionsInvalid("duplicate loudness track"));
            }
        }
        for row in value
            .get("loudness_programme")
            .and_then(Json::as_arr)
            .unwrap_or(&[])
        {
            let files =
                row.get("file")
                    .and_then(Json::as_arr)
                    .ok_or(StoreError::ConclusionsInvalid("programme without files"))?
                    .iter()
                    .map(|file| {
                        Ok(ProgrammeFile {
                            path: file
                                .field_str("path")
                                .ok_or(StoreError::ConclusionsInvalid(
                                    "programme file without path",
                                ))?
                                .to_owned(),
                            size: file
                                .field_u64("size")
                                .ok_or(StoreError::ConclusionsInvalid(
                                    "programme file without size",
                                ))?,
                            mtime: file.field_u64("mtime").ok_or(
                                StoreError::ConclusionsInvalid("programme file without mtime"),
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, StoreError>>()?;
            if files.is_empty() {
                return Err(StoreError::ConclusionsInvalid("empty loudness programme"));
            }
            result.loudness_programmes.push(CachedProgramme {
                files,
                measurement: measurement_from_json(row.get("measurement"))?,
            });
        }
    }
    Ok(result)
}

/// Atomically saves the independent store.
pub fn save(conclusions: &Conclusions, path: &Path) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, to_json(conclusions).to_string_compact())?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Loads the independent store, or returns `None` when it does not exist.
pub fn load(path: &Path) -> Result<Option<Conclusions>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    let value = json::parse(&content).map_err(StoreError::ConclusionsParse)?;
    from_json(&value).map(Some)
}

#[cfg(test)]
#[path = "conclusions_tests.rs"]
mod tests;
