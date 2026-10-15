//! Reconstruct scoped FlacCompagnon reports from retained source entries.
use crate::json::Json;
use crate::model::{Catalog, EntityKind, Id};
use std::collections::BTreeSet;
/// Why the selected catalog entity cannot supply a complete current report.
#[derive(Debug, PartialEq, Eq)]
pub enum ExportError {
    /// Only existing tracks or releases are supported.
    InvalidSelection,
    /// No retained FlacCompagnon measurement exists for a selected file.
    MissingAnalysis,
    /// Stored measurements describe an older file state.
    StaleAnalysis,
    /// The store predates complete source-entry retention.
    IncompleteAnalysis,
    /// A retained entry uses a newer report schema.
    UnsupportedVersion,
    /// The file count or serialized response exceeds the export budget.
    TooLarge,
}
/// Export the selected entity only; never read sidecars, re-analyze, or modify source audio.
pub fn report(catalog: &Catalog, kind: EntityKind, id: Id) -> Result<String, ExportError> {
    let (root, tracks) = match kind {
        EntityKind::Track => {
            let track = catalog.track(id).ok_or(ExportError::InvalidSelection)?;
            let file = catalog
                .file(track.file_id)
                .ok_or(ExportError::InvalidSelection)?;
            let root = std::path::Path::new(&file.path)
                .parent()
                .ok_or(ExportError::InvalidSelection)?;
            (root.to_string_lossy().into_owned(), vec![id])
        }
        EntityKind::Release => {
            let release = catalog.release(id).ok_or(ExportError::InvalidSelection)?;
            (release.folder.clone(), release.track_ids.clone())
        }
        _ => return Err(ExportError::InvalidSelection),
    };
    if tracks.is_empty() {
        return Err(ExportError::MissingAnalysis);
    }
    if tracks.len() > 2048 {
        return Err(ExportError::TooLarge);
    }
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut version = 0;
    let mut total_bytes = 0;
    for id in tracks {
        let track = catalog.track(id).ok_or(ExportError::InvalidSelection)?;
        let file = catalog
            .file(track.file_id)
            .ok_or(ExportError::InvalidSelection)?;
        if !seen.insert(&file.path) {
            continue;
        }
        let records: Vec<_> = catalog
            .analyses
            .iter()
            .filter(|record| record.source == "flaccompagnon" && record.path == file.path)
            .collect();
        if records.is_empty() {
            return Err(ExportError::MissingAnalysis);
        }
        let record = records
            .into_iter()
            .filter(|record| record.still_applies(file.size, file.mtime))
            .max_by_key(|record| (record.result_at_ns, record.imported_at))
            .ok_or(ExportError::StaleAnalysis)?;
        if record.source_version > super::SUPPORTED_VERSION {
            return Err(ExportError::UnsupportedVersion);
        }
        let entry = record
            .source_data
            .as_ref()
            .filter(|entry| {
                matches!(entry, Json::Obj(_)) && entry.get("path").and_then(Json::as_str).is_some()
            })
            .ok_or(ExportError::IncompleteAnalysis)?;
        total_bytes += entry.to_string_pretty().len();
        if total_bytes > 16 * 1024 * 1024 {
            return Err(ExportError::TooLarge);
        }
        version = version.max(record.source_version);
        entries.push(entry.clone());
    }
    let has_flac = entries.iter().any(|entry| {
        entry
            .get("flac_md5")
            .is_some_and(|value| !matches!(value, Json::Null))
    });
    let mut inner = Json::obj();
    inner.set("root", Json::Str(root));
    inner.set("files", Json::Arr(entries));
    inner.set("has_flac", Json::Bool(has_flac));
    let mut outer = Json::obj();
    outer.set("format", Json::Str(super::FLACCOMPAGNON_FORMAT.into()));
    outer.set("version", Json::Num(f64::from(version)));
    outer.set("report", inner);
    let content = outer.to_string_pretty();
    if content.len() > 16 * 1024 * 1024 {
        return Err(ExportError::TooLarge);
    }
    Ok(content)
}
#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
