//! Lightweight retained-result coverage, with no decoding, enrichment or source-file reads.
use super::*;
use aede_core::analysis::FileAnalysis;
use aede_core::model::AudioFile;
use std::collections::HashMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    offset: Option<usize>,
    limit: Option<usize>,
    q: Option<String>,
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
        page(
            held.as_ref().ok_or_else(unavailable)?,
            offset,
            limit,
            query.q.as_deref(),
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
pub(super) fn page(
    c: &Catalog,
    offset: usize,
    limit: usize,
    query: Option<&str>,
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
    let albums: Vec<_> = c
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
        .collect();
    let items=albums.iter().skip(offset).take(limit).map(|r|{
  let mut counts=Counts::default();let mut spectra=0;
  for id in &r.track_ids{let track=c.track(*id).ok_or_else(unavailable)?;counts.add(states.get(&track.file_id).copied().ok_or_else(unavailable)?);let file=c.file(track.file_id).ok_or_else(unavailable)?;spectra+=usize::from(aede_core::spectrum::picture_for(Path::new(&file.path)).is_file());}
  Ok(json!({"reference":reference_of(c,EntityKind::Release,r.id)?,"title":r.title,"artist":r.album_artist_id.and_then(|id|c.artist(id)).map(|a|a.name.as_str()),"tracks":r.track_ids.len(),"current":counts.current,"stale":counts.stale,"failed":counts.failed,"missing":counts.missing,"spectra":spectra}))
 }).collect::<Result<Vec<_>,ApiError>>()?;
    Ok(
        json!({"items":items,"total":albums.len(),"offset":offset,"limit":limit,"scanned_at":c.scanned_at,"summary":{"files":c.files.len(),"current":summary.current,"stale":summary.stale,"failed":summary.failed,"missing":summary.missing}}),
    )
}
