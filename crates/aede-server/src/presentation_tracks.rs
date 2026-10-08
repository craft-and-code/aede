//! Lightweight album-order pages; retained analysis bodies are read on demand.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    #[serde(rename = "ref")]
    reference: String,
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
    inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        page(
            guard.as_ref().ok_or_else(unavailable)?,
            &query.reference,
            offset,
            limit,
        )
    })
    .await
}

pub(super) fn page(
    catalog: &Catalog,
    token: &str,
    offset: usize,
    limit: usize,
) -> Result<Value, ApiError> {
    if token.len() > MAX_REFERENCE_BYTES {
        return Err(invalid_query("reference is too long"));
    }
    let id = catalog::filter_reference(catalog, token, EntityKind::Release)?;
    let album = catalog.release(id).ok_or_else(unavailable)?;
    let items = album.track_ids.iter().enumerate().skip(offset).take(limit).map(|(position, id)| {
        let track = catalog.track(*id).ok_or_else(unavailable)?;
        let file = catalog.file(track.file_id).ok_or_else(unavailable)?;
        Ok(json!({
            "reference": reference_of(catalog, EntityKind::Track, *id)?,
            "title": track.title,
            "disc_no": track.disc_no,
            "track_no": track.track_no,
            "album_position": position + 1,
            "duration_ms": track.duration_ms,
            "analysis_available": catalog.analyses_of(file).next().is_some(),
            "spectrum_available": aede_core::spectrum::picture_for(Path::new(&file.path)).is_file()
        }))
    }).collect::<Result<Vec<_>, ApiError>>()?;
    Ok(
        json!({"items":items,"total":album.track_ids.len(),"offset":offset,"limit":limit,"scanned_at":catalog.scanned_at}),
    )
}
