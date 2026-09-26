//! Frozen v1 catalog queries and entity representations.

use super::*;

pub(super) fn filter_reference(
    catalog: &Catalog,
    token: &str,
    kind: EntityKind,
) -> Result<Id, ApiError> {
    let requested = EntityRef::parse_token(token)
        .filter(|reference| reference.kind == kind && !reference.key.trim().is_empty())
        .ok_or_else(|| invalid_query(format!("filter must be a {} reference", kind.as_str())))?;
    requested.resolve(catalog).ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            "no entity matches this reference in the current catalog",
        )
    })
}

pub(super) fn reference(catalog: &Catalog, kind: EntityKind, id: u32) -> Option<String> {
    EntityRef::of(catalog, kind, id).map(|reference| reference.to_token())
}

pub(super) async fn status(State(state): State<ApiState>) -> Json<Status> {
    Json(Status {
        status: "ok",
        api_version: 1,
        catalog_loaded: state.catalog.read().await.is_some(),
    })
}

pub(super) async fn library(State(state): State<ApiState>) -> Result<Json<Library>, ApiError> {
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    Ok(Json(Library {
        scanned_at: catalog.scanned_at,
        files: catalog.files.len(),
        artists: catalog.artists.len(),
        releases: catalog.releases.len(),
        recordings: catalog.recordings.len(),
        tracks: catalog.tracks.len(),
    }))
}

pub(super) async fn artists(
    State(state): State<ApiState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ArtistItem>>, ApiError> {
    let options = list_query(query, ListKind::Artist)?;
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let mut rows: Vec<_> = catalog
        .artists
        .iter()
        .filter(|artist| {
            options
                .mbid
                .as_deref()
                .is_none_or(|mbid| artist.mbid.as_deref() == Some(mbid))
                && options.q.as_deref().is_none_or(|q| {
                    text::normalize(&artist.name).contains(q)
                        || artist
                            .aliases
                            .iter()
                            .any(|alias| text::normalize(alias).contains(q))
                })
        })
        .collect();
    match options.sort {
        SortMode::Catalog if options.descending => rows.reverse(),
        SortMode::Name if options.descending => {
            rows.sort_by_cached_key(|artist| std::cmp::Reverse(text::normalize(&artist.sort_name)));
        }
        SortMode::Name => rows.sort_by_cached_key(|artist| text::normalize(&artist.sort_name)),
        _ => {}
    }
    let total = rows.len();
    let items = rows
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .filter_map(|artist| {
            Some(ArtistItem {
                reference: reference(catalog, EntityKind::Artist, artist.id)?,
                name: artist.name.clone(),
                sort_name: artist.sort_name.clone(),
                mbid: artist.mbid.clone(),
                aliases: artist.aliases.clone(),
            })
        })
        .collect();
    Ok(Json(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    }))
}

pub(super) async fn releases(
    State(state): State<ApiState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ReleaseItem>>, ApiError> {
    let options = list_query(query, ListKind::Release)?;
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let artist_id = options
        .artist
        .as_deref()
        .map(|token| filter_reference(catalog, token, EntityKind::Artist))
        .transpose()?;
    let mut rows: Vec<_> = catalog
        .releases
        .iter()
        .filter(|release| {
            options.year.is_none_or(|year| release.year == Some(year))
                && artist_id.is_none_or(|id| release.album_artist_id == Some(id))
                && options
                    .q
                    .as_deref()
                    .is_none_or(|q| text::normalize(&release.title).contains(q))
        })
        .collect();
    match options.sort {
        SortMode::Catalog if options.descending => rows.reverse(),
        SortMode::Title if options.descending => {
            rows.sort_by_cached_key(|release| std::cmp::Reverse(text::normalize(&release.title)));
        }
        SortMode::Title => rows.sort_by_cached_key(|release| text::normalize(&release.title)),
        SortMode::Year => rows.sort_by_cached_key(|release| {
            let year = release.year.unwrap_or_default();
            (
                release.year.is_none(),
                if options.descending {
                    u32::MAX - year
                } else {
                    year
                },
                text::normalize(&release.title),
            )
        }),
        _ => {}
    }
    let total = rows.len();
    let items = rows
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .filter_map(|release| {
            Some(ReleaseItem {
                reference: reference(catalog, EntityKind::Release, release.id)?,
                title: release.title.clone(),
                year: release.year,
                album_artist: release
                    .album_artist_id
                    .and_then(|id| reference(catalog, EntityKind::Artist, id)),
                track_count: release.track_ids.len(),
                cover_path: release.cover_path.clone(),
            })
        })
        .collect();
    Ok(Json(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    }))
}

pub(super) async fn tracks(
    State(state): State<ApiState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<TrackItem>>, ApiError> {
    let options = list_query(query, ListKind::Track)?;
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let release_id = options
        .release
        .as_deref()
        .map(|token| filter_reference(catalog, token, EntityKind::Release))
        .transpose()?;
    let mut rows: Vec<_> = catalog
        .tracks
        .iter()
        .filter(|track| {
            release_id.is_none_or(|id| track.release_id == Some(id))
                && options
                    .q
                    .as_deref()
                    .is_none_or(|q| text::normalize(&track.title).contains(q))
        })
        .collect();
    match options.sort {
        SortMode::Catalog if options.descending => rows.reverse(),
        SortMode::Title if options.descending => {
            rows.sort_by_cached_key(|track| std::cmp::Reverse(text::normalize(&track.title)));
        }
        SortMode::Title => rows.sort_by_cached_key(|track| text::normalize(&track.title)),
        _ => {}
    }
    let total = rows.len();
    let items = rows
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .filter_map(|track| {
            Some(TrackItem {
                reference: reference(catalog, EntityKind::Track, track.id)?,
                title: track.title.clone(),
                release: track
                    .release_id
                    .and_then(|id| reference(catalog, EntityKind::Release, id)),
                recording: reference(catalog, EntityKind::Recording, track.recording_id),
                duration_ms: track.duration_ms,
            })
        })
        .collect();
    Ok(Json(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    }))
}

pub(super) async fn recordings(
    State(state): State<ApiState>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<RecordingItem>>, ApiError> {
    let options = list_query(query, ListKind::Recording)?;
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let work_id = options
        .work
        .as_deref()
        .map(|token| filter_reference(catalog, token, EntityKind::Work))
        .transpose()?;
    let mut rows: Vec<_> = catalog
        .recordings
        .iter()
        .filter(|recording| {
            work_id.is_none_or(|id| recording.work_ids.contains(&id))
                && options
                    .q
                    .as_deref()
                    .is_none_or(|q| text::normalize(&recording.title).contains(q))
        })
        .collect();
    match options.sort {
        SortMode::Catalog if options.descending => rows.reverse(),
        SortMode::Title if options.descending => rows
            .sort_by_cached_key(|recording| std::cmp::Reverse(text::normalize(&recording.title))),
        SortMode::Title => rows.sort_by_cached_key(|recording| text::normalize(&recording.title)),
        _ => {}
    }
    let total = rows.len();
    let items = rows
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .filter_map(|recording| {
            Some(RecordingItem {
                reference: reference(catalog, EntityKind::Recording, recording.id)?,
                title: recording.title.clone(),
                mbid: recording.mbid.clone(),
                track_count: recording.track_ids.len(),
                work_count: recording.work_ids.len(),
            })
        })
        .collect();
    Ok(Json(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    }))
}

pub(super) async fn entity(
    State(state): State<ApiState>,
    query: Result<Query<EntityQuery>, QueryRejection>,
) -> Result<Json<EntityDetail>, ApiError> {
    let Query(query) = query.map_err(|rejection| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_reference",
            rejection.body_text(),
        )
    })?;
    let requested = EntityRef::parse_token(&query.reference)
        .filter(|reference| !reference.key.trim().is_empty())
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_reference",
                "ref must be a stable entity reference such as artist:miles davis",
            )
        })?;
    let guard = state.catalog.read().await;
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    entity_detail(catalog, &requested).map(Json)
}

pub(super) fn entity_detail(
    catalog: &Catalog,
    requested: &EntityRef,
) -> Result<EntityDetail, ApiError> {
    let id = requested.resolve(catalog).ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            "no entity matches this reference in the current catalog",
        )
    })?;
    let detail = match requested.kind {
        EntityKind::Artist => {
            let artist = catalog.artist(id).ok_or_else(unavailable)?;
            EntityDetail::Artist {
                reference: requested.to_token(),
                name: artist.name.clone(),
                sort_name: artist.sort_name.clone(),
                mbid: artist.mbid.clone(),
                aliases: artist.aliases.clone(),
                releases: catalog
                    .releases_as_album_artist(id)
                    .into_iter()
                    .filter_map(|id| reference(catalog, EntityKind::Release, id))
                    .collect(),
            }
        }
        EntityKind::Release => {
            let release = catalog.release(id).ok_or_else(unavailable)?;
            EntityDetail::Release {
                reference: requested.to_token(),
                title: release.title.clone(),
                year: release.year,
                album_artist: release
                    .album_artist_id
                    .and_then(|id| reference(catalog, EntityKind::Artist, id)),
                tracks: release
                    .track_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Track, id))
                    .collect(),
                release_group: release
                    .release_group_id
                    .and_then(|id| reference(catalog, EntityKind::ReleaseGroup, id)),
                labels: release
                    .label_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Label, id))
                    .collect(),
                cover_path: release.cover_path.clone(),
            }
        }
        EntityKind::Track => {
            let track = catalog.track(id).ok_or_else(unavailable)?;
            let file = catalog.file(track.file_id).ok_or_else(unavailable)?;
            EntityDetail::Track {
                reference: requested.to_token(),
                title: track.title.clone(),
                release: track
                    .release_id
                    .and_then(|id| reference(catalog, EntityKind::Release, id)),
                recording: reference(catalog, EntityKind::Recording, track.recording_id),
                duration_ms: track.duration_ms,
                path: file.path.clone(),
                size: file.size,
            }
        }
        EntityKind::Recording => {
            let recording = catalog.recording(id).ok_or_else(unavailable)?;
            EntityDetail::Recording {
                reference: requested.to_token(),
                title: recording.title.clone(),
                isrc: recording.isrc.clone(),
                mbid: recording.mbid.clone(),
                tracks: recording
                    .track_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Track, id))
                    .collect(),
                works: recording
                    .work_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Work, id))
                    .collect(),
            }
        }
        EntityKind::Work => {
            let work = catalog.work(id).ok_or_else(unavailable)?;
            EntityDetail::Work {
                reference: requested.to_token(),
                title: work.title.clone(),
                mbid: work.mbid.clone(),
                recordings: work
                    .recording_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Recording, id))
                    .collect(),
            }
        }
        EntityKind::ReleaseGroup => {
            let group = catalog.release_group(id).ok_or_else(unavailable)?;
            EntityDetail::ReleaseGroup {
                reference: requested.to_token(),
                title: group.title.clone(),
                mbid: group.mbid.clone(),
                releases: group
                    .release_ids
                    .iter()
                    .filter_map(|&id| reference(catalog, EntityKind::Release, id))
                    .collect(),
            }
        }
        EntityKind::Label => {
            let label = catalog.label(id).ok_or_else(unavailable)?;
            EntityDetail::Label {
                reference: requested.to_token(),
                name: label.name.clone(),
                mbid: label.mbid.clone(),
                releases: catalog
                    .releases_of_label(id)
                    .into_iter()
                    .filter_map(|id| reference(catalog, EntityKind::Release, id))
                    .collect(),
            }
        }
        EntityKind::Genre => {
            let genre = catalog.genre(id).ok_or_else(unavailable)?;
            EntityDetail::Genre {
                reference: requested.to_token(),
                name: genre.name.clone(),
                releases: catalog
                    .releases_of_genre(id)
                    .into_iter()
                    .filter_map(|id| reference(catalog, EntityKind::Release, id))
                    .collect(),
                tracks: catalog
                    .tracks_of_genre(id)
                    .into_iter()
                    .filter_map(|id| reference(catalog, EntityKind::Track, id))
                    .collect(),
            }
        }
    };
    Ok(detail)
}
