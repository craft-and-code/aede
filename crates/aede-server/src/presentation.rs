//! Paged client projections retain identities, provenance and contributions.
use super::*;
use aede_core::coverart::Kind;
use aede_core::sources::{Facts, Sources};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/presentation/artists", get(artists))
        .route("/api/v1/presentation/albums", get(albums))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Paging {
    offset: Option<usize>,
    limit: Option<usize>,
}
fn bounds(query: Result<Query<Paging>, QueryRejection>) -> Result<(usize, usize), ApiError> {
    let Query(query) = query.map_err(|failure| invalid_query(failure.body_text()))?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(invalid_query("limit must be between 1 and 200"));
    }
    Ok((query.offset.unwrap_or(0), limit))
}
fn reference_of(catalog: &Catalog, kind: EntityKind, id: Id) -> Result<String, ApiError> {
    reference(catalog, kind, id).ok_or_else(unavailable)
}
fn related_releases(catalog: &Catalog, id: Id) -> Vec<Id> {
    let mut releases: BTreeSet<_> = catalog.releases_of_artist(id).into_iter().collect();
    releases.extend(
        catalog
            .releases
            .iter()
            .filter(|release| release.album_artist_id == Some(id))
            .map(|release| release.id),
    );
    releases.into_iter().collect()
}
fn artist_row(
    catalog: &Catalog,
    sources: &Sources,
    data_dir: &Path,
    artist: &aede_core::model::Artist,
) -> Result<Value, ApiError> {
    let entity = EntityRef::of(catalog, EntityKind::Artist, artist.id).ok_or_else(unavailable)?;
    let origin = crate::catalog_commands::artist_origin(catalog, &entity, sources);
    let releases = related_releases(catalog, artist.id);
    let genres: BTreeSet<_> = releases
        .iter()
        .flat_map(|id| catalog.genres_of(EntityKind::Release, *id))
        .map(|genre| genre.name.clone())
        .collect();
    let biography = sources
        .about(&entity)
        .filter(|record| sources.is_trusted(catalog, record))
        .find_map(|record| {
            if let Facts::Artist(facts) = &record.facts {
                facts.summary.as_ref()
            } else {
                None
            }
        });
    let folder = crate::artwork::artist_folder(catalog, artist.id, data_dir);
    let media = |kind| {
        folder
            .as_ref()
            .is_some_and(|folder| crate::artwork::beside(folder.clone(), kind).is_some())
    };
    let relations:Vec<_>=catalog.neighbours_of_artist(artist.id).into_iter().map(|(other,weight,kind)|json!({"reference":reference(catalog,EntityKind::Artist,other.id),"name":other.name,"weight":weight,"kind":kind,"source":"tags"})).collect();
    Ok(
        json!({"reference":entity.to_token(),"name":artist.name,"genre":genres.iter().cloned().collect::<Vec<_>>().join(" · "),"genres":genres,"country":origin.country_code.as_ref().or(origin.area.as_ref()),"country_code":origin.country_code,"origin":origin,"biography":biography.map(|prose|prose.text.as_str()).unwrap_or(""),"biography_credit":biography.map(|prose|json!({"url":prose.url,"lang":prose.lang,"licence":prose.licence})),"artwork":false,"media":{"banner":media(Kind::Banner),"background":media(Kind::Background),"portrait":media(Kind::Artist)},"release_refs":releases.into_iter().map(|id|reference_of(catalog,EntityKind::Release,id)).collect::<Result<Vec<_>,_>>()?,"search_fields":[artist.sort_name,artist.mbid.as_deref().unwrap_or(""),artist.aliases.join(" ")],"relations":relations}),
    )
}
fn album_row(catalog: &Catalog, release: &aede_core::model::Release) -> Result<Value, ApiError> {
    let reference = reference_of(catalog, EntityKind::Release, release.id)?;
    let mut contributors: BTreeSet<_> = catalog
        .credits_on(EntityKind::Release, release.id)
        .into_iter()
        .map(|(artist, _)| artist.id)
        .collect();
    let mut search = Vec::new();
    for id in &release.track_ids {
        if let Some(track) = catalog.track(*id) {
            contributors.extend(
                catalog
                    .credits_on(EntityKind::Track, *id)
                    .into_iter()
                    .map(|(artist, _)| artist.id),
            );
            if let Some(file) = catalog.file(track.file_id) {
                for (name, values) in &file.tags {
                    search.push(name.clone());
                    search.extend(values.iter().cloned());
                }
            }
        }
    }
    if let Some(artist) = release.album_artist_id {
        contributors.insert(artist);
    }
    let genres: Vec<_> = catalog
        .genres_of(EntityKind::Release, release.id)
        .iter()
        .map(|genre| genre.name.clone())
        .collect();
    Ok(
        json!({"reference":reference,"artist":release.album_artist_id.and_then(|id|crate::catalog::reference(catalog,EntityKind::Artist,id)),"title":release.title,"year":release.year,"compilation":release.is_compilation,"tags":genres,"contributors":contributors.into_iter().map(|id|reference_of(catalog,EntityKind::Artist,id)).collect::<Result<Vec<_>,_>>()?,"cover":crate::subsonic::artwork::candidate(release).is_some(),"track_count":release.track_ids.len(),"search_fields":search}),
    )
}
async fn artists(
    State(state): State<ApiState>,
    query: Result<Query<Paging>, QueryRejection>,
) -> Result<Json<Page<Value>>, ApiError> {
    let (offset, limit) = bounds(query)?;
    inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let sources = inspection::held_sources(&state.data_dir)?;
        let items = catalog
            .artists
            .iter()
            .skip(offset)
            .take(limit)
            .map(|artist| artist_row(catalog, &sources, &state.data_dir, artist))
            .collect::<Result<_, _>>()?;
        Ok(Page::new(
            items,
            catalog.artists.len(),
            offset,
            limit,
            catalog.scanned_at,
        ))
    })
    .await
}
async fn albums(
    State(state): State<ApiState>,
    query: Result<Query<Paging>, QueryRejection>,
) -> Result<Json<Page<Value>>, ApiError> {
    let (offset, limit) = bounds(query)?;
    inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let items = catalog
            .releases
            .iter()
            .skip(offset)
            .take(limit)
            .map(|release| album_row(catalog, release))
            .collect::<Result<_, _>>()?;
        Ok(Page::new(
            items,
            catalog.releases.len(),
            offset,
            limit,
            catalog.scanned_at,
        ))
    })
    .await
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
