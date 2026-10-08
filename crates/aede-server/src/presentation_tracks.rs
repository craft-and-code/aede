//! Lightweight ordered track pages; retained analyses are read on demand.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    #[serde(rename = "ref")]
    reference: Option<String>,
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
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
        return Err(invalid_query("track search is too long"));
    }
    inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        listing(
            guard.as_ref().ok_or_else(unavailable)?,
            query.reference.as_deref(),
            offset,
            limit,
            query.q.as_deref(),
        )
    })
    .await
}
#[cfg(test)]
pub(super) fn page(
    catalog: &Catalog,
    token: &str,
    offset: usize,
    limit: usize,
) -> Result<Value, ApiError> {
    listing(catalog, Some(token), offset, limit, None)
}
fn searchable(c: &Catalog, track: &aede_core::model::Track, query: &str) -> bool {
    let has = |value: &str| aede_core::text::normalize(value).contains(query);
    has(&track.title)
        || c.file(track.file_id).is_some_and(|f| {
            f.tags
                .iter()
                .any(|(key, values)| has(key) || values.iter().any(|v| has(v)))
        })
        || track
            .release_id
            .and_then(|id| c.release(id))
            .is_some_and(|r| {
                has(&r.title)
                    || r.album_artist_id
                        .and_then(|id| c.artist(id))
                        .is_some_and(|a| has(&a.name))
            })
        || c.credits_on(EntityKind::Track, track.id)
            .iter()
            .any(|(a, credit)| has(&a.name) || has(credit))
}
pub(super) fn listing(
    c: &Catalog,
    token: Option<&str>,
    offset: usize,
    limit: usize,
    query: Option<&str>,
) -> Result<Value, ApiError> {
    let ids: Vec<_> = if let Some(token) = token {
        if token.len() > MAX_REFERENCE_BYTES {
            return Err(invalid_query("reference is too long"));
        }
        let id = catalog::filter_reference(c, token, EntityKind::Release)?;
        c.release(id).ok_or_else(unavailable)?.track_ids.clone()
    } else {
        c.tracks.iter().map(|t| t.id).collect()
    };
    let query = query
        .map(aede_core::text::normalize)
        .filter(|q| !q.is_empty());
    let rows: Vec<_> = ids
        .into_iter()
        .filter(|id| {
            c.track(*id)
                .is_some_and(|t| query.as_ref().is_none_or(|q| searchable(c, t, q)))
        })
        .collect();
    let items = rows.iter().skip(offset).take(limit).map(|id| {
        let track = c.track(*id).ok_or_else(unavailable)?;
        let file = c.file(track.file_id).ok_or_else(unavailable)?;
        let release = track.release_id.and_then(|id| c.release(id));
        Ok(json!({
            "reference":reference_of(c,EntityKind::Track,*id)?, "title":track.title,
            "release":release.map(|r| reference_of(c,EntityKind::Release,r.id)).transpose()?,
            "album_title":release.map(|r| r.title.as_str()),
            "disc_no":track.disc_no,"track_no":track.track_no,
            "album_position":release.and_then(|r| r.track_ids.iter().position(|id| *id == track.id)).map(|p| p+1),
            "duration_ms":track.duration_ms,
            "analysis_available":c.analyses_of(file).next().is_some(),
            "spectrum_available":aede_core::spectrum::picture_for(Path::new(&file.path)).is_file()
        }))
    }).collect::<Result<Vec<_>,ApiError>>()?;
    Ok(
        json!({"items":items,"total":rows.len(),"offset":offset,"limit":limit,"scanned_at":c.scanned_at}),
    )
}
