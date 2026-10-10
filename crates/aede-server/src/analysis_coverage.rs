//! Lightweight retained-result coverage, with no decoding, enrichment or source-file reads.
use super::*;
#[path = "analysis_integrity.rs"]
mod integrity;
use aede_core::analysis::FileAnalysis;
use aede_core::model::AudioFile;
use std::collections::HashMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    offset: Option<usize>,
    limit: Option<usize>,
    q: Option<String>,
    sort: Option<SortColumn>,
    direction: Option<Direction>,
}
#[derive(Clone, Copy, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum SortColumn {
    Title,
    Current,
    #[default]
    Missing,
    Stale,
    Failed,
    Spectra,
    Integrity,
}
#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub(super) enum Direction {
    Asc,
    #[default]
    Desc,
}
pub(super) async fn list(
    State(state): State<ApiState>,
    query: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(query) = query.map_err(|e| invalid_query(e.body_text()))?;
    let (offset, limit) = bounds(Ok(Query(Paging {
        offset: query.offset,
        limit: query.limit,
    })))?;
    if query.q.as_ref().is_some_and(|q| q.len() > 1024) {
        return Err(invalid_query("analysis search is too long"));
    }
    inspection::inspect(state, move |state| {
        let held = state.catalog.blocking_read();
        sorted_page(
            held.as_ref().ok_or_else(unavailable)?,
            offset,
            limit,
            query.q.as_deref(),
            query.sort.unwrap_or_default(),
            query.direction.unwrap_or_default(),
        )
    })
    .await
}
#[derive(Default, Serialize)]
struct Counts {
    current: usize,
    stale: usize,
    failed: usize,
    missing: usize,
}
impl Counts {
    fn add(&mut self, status: &str) {
        match status {
            "current" => self.current += 1,
            "stale" => self.stale += 1,
            "failed" => self.failed += 1,
            _ => self.missing += 1,
        }
    }
}
fn status(file: &AudioFile, records: Option<&Vec<&FileAnalysis>>) -> &'static str {
    let Some(records) = records else {
        return "missing";
    };
    let Some(record) = records
        .iter()
        .filter(|a| a.still_applies(file.size, file.mtime))
        .max_by_key(|a| (a.result_at_ns, a.imported_at))
    else {
        return "stale";
    };
    if record.error.is_some() {
        "failed"
    } else {
        "current"
    }
}
#[cfg(test)]
pub(super) fn page(
    c: &Catalog,
    offset: usize,
    limit: usize,
    query: Option<&str>,
) -> Result<Value, ApiError> {
    sorted_page(
        c,
        offset,
        limit,
        query,
        SortColumn::Missing,
        Direction::Desc,
    )
}
pub(super) fn sorted_page(
    c: &Catalog,
    offset: usize,
    limit: usize,
    query: Option<&str>,
    sort: SortColumn,
    direction: Direction,
) -> Result<Value, ApiError> {
    let mut records: HashMap<&str, Vec<&FileAnalysis>> = HashMap::new();
    for record in &c.analyses {
        if record.source == "flaccompagnon" {
            records.entry(&record.path).or_default().push(record);
        }
    }
    let mut summary = Counts::default();
    let states: HashMap<_, _> = c
        .files
        .iter()
        .map(|file| {
            let state = status(file, records.get(file.path.as_str()));
            summary.add(state);
            (file.id, state)
        })
        .collect();
    let query = query
        .map(aede_core::text::normalize)
        .filter(|q| !q.is_empty());
    let mut albums = c
        .releases
        .iter()
        .filter(|r| {
            query.as_ref().is_none_or(|q| {
                aede_core::text::normalize(&r.title).contains(q)
                    || r.album_artist_id
                        .and_then(|id| c.artist(id))
                        .is_some_and(|a| aede_core::text::normalize(&a.name).contains(q))
            })
        })
        .map(|r| {
            let mut counts = Counts::default();
            let mut spectra = 0;
            for id in &r.track_ids {
                let track = c.track(*id).ok_or_else(unavailable)?;
                counts.add(
                    states
                        .get(&track.file_id)
                        .copied()
                        .ok_or_else(unavailable)?,
                );
                if sort == SortColumn::Spectra {
                    let file = c.file(track.file_id).ok_or_else(unavailable)?;
                    spectra += usize::from(
                        aede_core::spectrum::picture_for(Path::new(&file.path)).is_file(),
                    );
                }
            }
            let mut integrity = integrity::Counts::default();
            for id in &r.track_ids {
                let track = c.track(*id).ok_or_else(unavailable)?;
                let file = c.file(track.file_id).ok_or_else(unavailable)?;
                integrity.add(file, records.get(file.path.as_str()));
            }
            Ok((
                r,
                counts,
                spectra,
                aede_core::text::normalize(&r.title),
                integrity,
            ))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    albums.sort_by(|a, b| {
        let order = match sort {
            SortColumn::Title => a.3.cmp(&b.3),
            SortColumn::Current => a.1.current.cmp(&b.1.current),
            SortColumn::Missing => a.1.missing.cmp(&b.1.missing),
            SortColumn::Stale => a.1.stale.cmp(&b.1.stale),
            SortColumn::Failed => a.1.failed.cmp(&b.1.failed),
            SortColumn::Spectra => a.2.cmp(&b.2),
            SortColumn::Integrity => {
                a.4.ko
                    .cmp(&b.4.ko)
                    .then_with(|| (a.4.unchecked + a.4.stale).cmp(&(b.4.unchecked + b.4.stale)))
            }
        };
        let order = match direction {
            Direction::Asc => order,
            Direction::Desc => order.reverse(),
        };
        // Equal counts keep deterministic pages regardless of direction or catalog insertion order.
        order
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.0.key.cmp(&b.0.key))
            .then_with(|| a.0.id.cmp(&b.0.id))
    });
    let items = albums.iter().skip(offset).take(limit).map(|(r, counts, stored_spectra, _, integrity)| {
        let spectra = if sort == SortColumn::Spectra { *stored_spectra } else {
            let mut count = 0;
            for id in &r.track_ids {
                let track = c.track(*id).ok_or_else(unavailable)?;
                let file = c.file(track.file_id).ok_or_else(unavailable)?;
                count += usize::from(aede_core::spectrum::picture_for(Path::new(&file.path)).is_file());
            }
            count
        };
        Ok(json!({"integrity":integrity,"reference":reference_of(c,EntityKind::Release,r.id)?,"title":r.title,"artist":r.album_artist_id.and_then(|id|c.artist(id)).map(|a|a.name.as_str()),"tracks":r.track_ids.len(),"current":counts.current,"stale":counts.stale,"failed":counts.failed,"missing":counts.missing,"spectra":spectra}))
    }).collect::<Result<Vec<_>,ApiError>>()?;
    Ok(
        json!({"items":items,"total":albums.len(),"offset":offset,"limit":limit,"scanned_at":c.scanned_at,"summary":{"files":c.files.len(),"current":summary.current,"stale":summary.stale,"failed":summary.failed,"missing":summary.missing}}),
    )
}
