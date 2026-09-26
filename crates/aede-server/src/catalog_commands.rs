//! CLI-shaped navigation over the catalog, without executing command-line code.

use std::collections::BTreeSet;

use aede_core::query as catalog_query;
use aede_core::sources::{self, Confidence, Facts, Sources};
use aede_core::user::{LOCAL_USER, UserData};
use axum::routing::MethodRouter;

use super::*;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionQuery {
    #[serde(rename = "ref")]
    reference: Option<String>,
    name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowseQuery {
    offset: Option<String>,
    limit: Option<String>,
    q: Option<String>,
    name: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    artist: Option<String>,
    year: Option<String>,
    genre: Option<String>,
    label: Option<String>,
    mbid: Option<String>,
}

#[derive(Serialize)]
struct NavigationDetail {
    #[serde(flatten)]
    entity: EntityDetail,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<ArtistOrigin>,
}

#[derive(Serialize)]
struct ArtistOrigin {
    status: &'static str,
    country_code: Option<String>,
    area: Option<String>,
    message: Option<&'static str>,
    attribution: Option<OriginAttribution>,
}

#[derive(Serialize)]
struct OriginAttribution {
    source: String,
    source_id: Option<String>,
    fetched_at: u64,
    confidence: &'static str,
    match_score: Option<u8>,
    trusted: bool,
}

#[derive(Serialize)]
struct FromResponse {
    reference: String,
    name: String,
    origin: ArtistOrigin,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowseItem {
    Genre {
        reference: String,
        name: String,
        release_count: usize,
        track_count: usize,
    },
    Label {
        reference: String,
        name: String,
        mbid: Option<String>,
        release_count: usize,
    },
    Work {
        reference: String,
        title: String,
        mbid: String,
        recording_count: usize,
    },
    ReleaseGroup {
        reference: String,
        title: String,
        mbid: String,
        release_count: usize,
    },
}

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/albums", get(albums))
        .route("/api/v1/album", detail_route(EntityKind::Release))
        .route("/api/v1/track", detail_route(EntityKind::Track))
        .route("/api/v1/artist", detail_route(EntityKind::Artist))
        .route("/api/v1/from", get(artist_from))
        .route("/api/v1/genres", browse_route(EntityKind::Genre))
        .route("/api/v1/genre", detail_route(EntityKind::Genre))
        .route("/api/v1/labels", browse_route(EntityKind::Label))
        .route("/api/v1/label", detail_route(EntityKind::Label))
        .route("/api/v1/works", browse_route(EntityKind::Work))
        .route("/api/v1/work", detail_route(EntityKind::Work))
        .route(
            "/api/v1/release-groups",
            browse_route(EntityKind::ReleaseGroup),
        )
        .route(
            "/api/v1/release-group",
            detail_route(EntityKind::ReleaseGroup),
        )
        .route("/api/v1/recording", detail_route(EntityKind::Recording))
}

fn detail_route(kind: EntityKind) -> MethodRouter<ApiState> {
    get(move |state, query| detail(state, query, kind))
}

fn browse_route(kind: EntityKind) -> MethodRouter<ApiState> {
    get(move |state, query| browse(state, query, kind))
}

fn selection(
    query: Result<Query<SelectionQuery>, QueryRejection>,
) -> Result<SelectionQuery, ApiError> {
    let Query(query) = query.map_err(|rejection| invalid_query(rejection.body_text()))?;
    if query.reference.is_some() == query.name.is_some() {
        return Err(invalid_query("provide exactly one of ref or name"));
    }
    if query
        .name
        .as_deref()
        .is_some_and(|name| text::normalize(name).is_empty())
    {
        return Err(invalid_query("name must contain searchable text"));
    }
    Ok(query)
}

fn entity_name(catalog: &Catalog, kind: EntityKind, id: Id) -> Option<&str> {
    match kind {
        EntityKind::Artist => catalog.artist(id).map(|row| row.name.as_str()),
        EntityKind::Release => catalog.release(id).map(|row| row.title.as_str()),
        EntityKind::Track => catalog.track(id).map(|row| row.title.as_str()),
        EntityKind::Recording => catalog.recording(id).map(|row| row.title.as_str()),
        EntityKind::Work => catalog.work(id).map(|row| row.title.as_str()),
        EntityKind::ReleaseGroup => catalog.release_group(id).map(|row| row.title.as_str()),
        EntityKind::Label => catalog.label(id).map(|row| row.name.as_str()),
        EntityKind::Genre => catalog.genre(id).map(|row| row.name.as_str()),
    }
}

fn entity_mbid(catalog: &Catalog, kind: EntityKind, id: Id) -> Option<&str> {
    match kind {
        EntityKind::Artist => catalog.artist(id).and_then(|row| row.mbid.as_deref()),
        EntityKind::Release => catalog.release(id).and_then(|row| row.mbid.as_deref()),
        EntityKind::Track => catalog.track(id).and_then(|row| row.mbid.as_deref()),
        EntityKind::Recording => catalog.recording(id).and_then(|row| row.mbid.as_deref()),
        EntityKind::Work => catalog.work(id).map(|row| row.mbid.as_str()),
        EntityKind::ReleaseGroup => catalog.release_group(id).map(|row| row.mbid.as_str()),
        EntityKind::Label => catalog.label(id).and_then(|row| row.mbid.as_deref()),
        EntityKind::Genre => None,
    }
}

fn named_ids(catalog: &Catalog, kind: EntityKind, name: &str) -> Vec<Id> {
    let normalized = text::normalize(name);
    let mut found: Vec<Id> = match kind {
        EntityKind::Artist => {
            let mut found: Vec<_> = catalog
                .find_artists(name)
                .0
                .iter()
                .map(|row| row.id)
                .collect();
            found.extend(
                catalog
                    .artists
                    .iter()
                    .filter(|row| {
                        row.aliases
                            .iter()
                            .any(|alias| text::normalize(alias).contains(&normalized))
                    })
                    .map(|row| row.id),
            );
            found.sort_unstable();
            found.dedup();
            found
        }
        EntityKind::Release => catalog
            .find_releases(name)
            .0
            .iter()
            .map(|row| row.id)
            .collect(),
        EntityKind::Track => catalog
            .find_tracks(name)
            .0
            .iter()
            .map(|row| row.id)
            .collect(),
        EntityKind::Recording => catalog
            .find_recordings(name)
            .iter()
            .map(|row| row.id)
            .collect(),
        EntityKind::Work => catalog.find_works(name).iter().map(|row| row.id).collect(),
        EntityKind::ReleaseGroup => catalog
            .find_release_groups(name)
            .iter()
            .map(|row| row.id)
            .collect(),
        EntityKind::Label => catalog
            .find_labels(name)
            .0
            .iter()
            .map(|row| row.id)
            .collect(),
        EntityKind::Genre => catalog
            .find_genres(name)
            .0
            .iter()
            .map(|row| row.id)
            .collect(),
    };
    // The core finders for recordings/works/groups include partial matches.
    // Singular navigation narrows to exact names or identifiers before ambiguity.
    let exact: Vec<_> = found
        .iter()
        .copied()
        .filter(|&id| {
            entity_mbid(catalog, kind, id) == Some(name)
                || entity_name(catalog, kind, id)
                    .is_some_and(|name| text::normalize(name) == normalized)
                || (kind == EntityKind::Artist
                    && catalog.artist(id).is_some_and(|row| {
                        row.aliases
                            .iter()
                            .any(|alias| text::normalize(alias) == normalized)
                    }))
        })
        .collect();
    if !exact.is_empty() {
        found = exact;
    }
    found
}

fn resolve_selection(
    catalog: &Catalog,
    query: &SelectionQuery,
    kind: EntityKind,
) -> Result<EntityRef, ApiError> {
    if let Some(token) = &query.reference {
        let requested = EntityRef::parse_token(token)
            .filter(|reference| reference.kind == kind && !reference.key.trim().is_empty())
            .ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    "invalid_reference",
                    format!("ref must name a {}", kind.as_str()),
                )
            })?;
        requested.resolve(catalog).ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "entity_not_found",
                "no entity matches this reference",
            )
        })?;
        return Ok(requested);
    }
    let name = query
        .name
        .as_deref()
        .ok_or_else(|| invalid_query("provide ref or name"))?;
    let found = named_ids(catalog, kind, name);
    match found.as_slice() {
        [] => Err(error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            format!("no {} matches this name", kind.as_str()),
        )),
        [id] => EntityRef::of(catalog, kind, *id).ok_or_else(unavailable),
        _ => {
            let candidates = found
                .iter()
                .take(MAX_LIMIT)
                .map(|&id| {
                    Ok(EntityCandidate {
                        reference: reference(catalog, kind, id).ok_or_else(unavailable)?,
                        name: entity_name(catalog, kind, id)
                            .ok_or_else(unavailable)?
                            .to_string(),
                    })
                })
                .collect::<Result<_, ApiError>>()?;
            Err(error(
                StatusCode::CONFLICT,
                "ambiguous_entity",
                format!(
                    "{} entities match; select one by ref (up to {MAX_LIMIT} candidates shown)",
                    found.len()
                ),
            )
            .with_candidates(candidates))
        }
    }
}

fn artist_origin(catalog: &Catalog, requested: &EntityRef, sources: &Sources) -> ArtistOrigin {
    let mut origin = ArtistOrigin {
        status: "unknown",
        country_code: None,
        area: None,
        message: Some(
            "No MusicBrainz information has been fetched for this artist; GET never fetches it automatically.",
        ),
        attribution: None,
    };
    let Some(record) = sources.get(requested, sources::MUSICBRAINZ) else {
        return origin;
    };
    let trusted = sources.is_trusted(catalog, record);
    origin.attribution = Some(OriginAttribution {
        source: record.source.clone(),
        source_id: record.source_id.clone(),
        fetched_at: record.fetched_at,
        confidence: match record.confidence {
            Confidence::Identified => "identified",
            Confidence::Matched(_) => "matched",
        },
        match_score: match record.confidence {
            Confidence::Identified => None,
            Confidence::Matched(score) => Some(score),
        },
        trusted,
    });
    if !trusted {
        origin.message = Some(
            "Fetched artist identity is not trusted; its origin is not used as an established fact.",
        );
        return origin;
    }
    if let Facts::Artist(facts) = &record.facts {
        origin.area = facts.area.clone().filter(|area| !area.trim().is_empty());
        origin.country_code = facts
            .country_code
            .clone()
            .filter(|code| !code.trim().is_empty());
    }
    if origin.area.is_some() || origin.country_code.is_some() {
        origin.status = "known";
        origin.message = None;
    } else {
        origin.message =
            Some("MusicBrainz was queried but supplied no country or area for this artist.");
    }
    origin
}

async fn detail(
    State(state): State<ApiState>,
    query: Result<Query<SelectionQuery>, QueryRejection>,
    kind: EntityKind,
) -> Result<Json<NavigationDetail>, ApiError> {
    let query = selection(query)?;
    crate::inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let requested = resolve_selection(catalog, &query, kind)?;
        let sources = if kind == EntityKind::Artist {
            Some(crate::inspection::held_sources(&state.data_dir)?)
        } else {
            None
        };
        Ok(NavigationDetail {
            entity: entity_detail(catalog, &requested)?,
            origin: sources
                .as_ref()
                .map(|sources| artist_origin(catalog, &requested, sources)),
        })
    })
    .await
}

async fn artist_from(
    State(state): State<ApiState>,
    query: Result<Query<SelectionQuery>, QueryRejection>,
) -> Result<Json<FromResponse>, ApiError> {
    let query = selection(query)?;
    crate::inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let requested = resolve_selection(catalog, &query, EntityKind::Artist)?;
        let id = requested.resolve(catalog).ok_or_else(unavailable)?;
        let sources = crate::inspection::held_sources(&state.data_dir)?;
        Ok(FromResponse {
            reference: requested.to_token(),
            name: catalog.artist(id).ok_or_else(unavailable)?.name.clone(),
            origin: artist_origin(catalog, &requested, &sources),
        })
    })
    .await
}

struct BrowseOptions {
    offset: usize,
    limit: usize,
    name: Option<String>,
    sort: String,
    descending: bool,
}

fn browse_options(query: &BrowseQuery, kind: EntityKind) -> Result<BrowseOptions, ApiError> {
    if query.q.is_some() && query.name.is_some() {
        return Err(invalid_query("q and name are aliases; provide only one"));
    }
    if kind != EntityKind::Release
        && (query.artist.is_some()
            || query.year.is_some()
            || query.genre.is_some()
            || query.label.is_some())
    {
        return Err(invalid_query(
            "artist, year, genre and label filters apply only to albums",
        ));
    }
    if kind == EntityKind::Genre && query.mbid.is_some() {
        return Err(invalid_query("genres have no MusicBrainz identifier"));
    }
    let name = query
        .q
        .as_ref()
        .or(query.name.as_ref())
        .map(|name| text::normalize(name));
    if name.as_deref() == Some("") {
        return Err(invalid_query("name must contain searchable text"));
    }
    let offset = query
        .offset
        .as_deref()
        .map(|value| decimal(value, "offset", true))
        .transpose()?
        .unwrap_or(0);
    let limit = query
        .limit
        .as_deref()
        .map(|value| decimal(value, "limit", true))
        .transpose()?
        .unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_pagination",
            format!("limit must be between 1 and {MAX_LIMIT}"),
        ));
    }
    let sort = query.sort.as_deref().unwrap_or("catalog");
    if !(matches!(sort, "catalog" | "name" | "title")
        || (sort == "year" && kind == EntityKind::Release))
    {
        return Err(invalid_query(
            "sort must be catalog, name, title, or year for albums",
        ));
    }
    if sort == "title" && matches!(kind, EntityKind::Genre | EntityKind::Label) {
        return Err(invalid_query("use sort=name for genres and labels"));
    }
    let descending = match query.order.as_deref().unwrap_or("asc") {
        "asc" => false,
        "desc" => true,
        _ => return Err(invalid_query("order must be asc or desc")),
    };
    if query.mbid.as_deref() == Some("") {
        return Err(invalid_query("mbid cannot be empty"));
    }
    Ok(BrowseOptions {
        offset,
        limit,
        name,
        sort: sort.to_string(),
        descending,
    })
}

fn album_filter(catalog: &Catalog, query: &BrowseQuery) -> Result<Option<BTreeSet<Id>>, ApiError> {
    let mut terms = Vec::new();
    for (value, field, kind) in [
        (
            &query.artist,
            catalog_query::Field::AlbumArtist,
            EntityKind::Artist,
        ),
        (&query.genre, catalog_query::Field::Genre, EntityKind::Genre),
        (&query.label, catalog_query::Field::Label, EntityKind::Label),
    ] {
        if let Some(value) = value {
            let (value, exact) = if EntityRef::parse_token(value).is_some() {
                let id = filter_reference(catalog, value, kind)?;
                (
                    entity_name(catalog, kind, id)
                        .ok_or_else(unavailable)?
                        .to_string(),
                    true,
                )
            } else {
                (value.clone(), false)
            };
            let value = text::normalize(&value);
            if value.is_empty() {
                return Err(invalid_query("filter values cannot be empty"));
            }
            terms.push(catalog_query::Query::Term(catalog_query::Term {
                field,
                test: if exact {
                    catalog_query::Test::Is(value)
                } else {
                    catalog_query::Test::Contains(value)
                },
            }));
        }
    }
    if let Some(year) = &query.year {
        let year = u32::try_from(decimal(year, "year", false)?)
            .map_err(|_| invalid_query("year is too large"))?;
        terms.push(catalog_query::Query::Term(catalog_query::Term {
            field: catalog_query::Field::Year,
            test: catalog_query::Test::Compare(catalog_query::Compare::Equal, f64::from(year)),
        }));
    }
    if terms.is_empty() {
        return Ok(None);
    }
    let expression = catalog_query::Query::And(terms);
    // Only public catalog fields are constructed above. User predicates and
    // private annotations are deliberately absent from this read-only context.
    let empty_user = UserData::default();
    let context = catalog_query::Context::new(catalog, &empty_user, LOCAL_USER);
    if let Some((kind, value)) = catalog_query::unknown_values(&expression, &context).first() {
        return Err(error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            format!("no {kind} matches {value}"),
        ));
    }
    Ok(Some(
        catalog_query::run(&expression, &context)
            .into_iter()
            .filter_map(|id| catalog.track(id).and_then(|track| track.release_id))
            .collect(),
    ))
}

async fn albums(
    State(state): State<ApiState>,
    query: Result<Query<BrowseQuery>, QueryRejection>,
) -> Result<Json<Page<ReleaseItem>>, ApiError> {
    let Query(query) = query.map_err(|rejection| invalid_query(rejection.body_text()))?;
    let options = browse_options(&query, EntityKind::Release)?;
    crate::inspection::inspect(state, move |state| album_page(state, &query, &options)).await
}

fn album_page(
    state: &ApiState,
    query: &BrowseQuery,
    options: &BrowseOptions,
) -> Result<Page<ReleaseItem>, ApiError> {
    let guard = state.catalog.blocking_read();
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let filtered = album_filter(catalog, query)?;
    let mut rows: Vec<_> = catalog
        .releases
        .iter()
        .filter(|release| {
            filtered
                .as_ref()
                .is_none_or(|ids| ids.contains(&release.id))
                && options
                    .name
                    .as_ref()
                    .is_none_or(|name| text::normalize(&release.title).contains(name))
                && query
                    .mbid
                    .as_ref()
                    .is_none_or(|mbid| release.mbid.as_ref() == Some(mbid))
        })
        .collect();
    match options.sort.as_str() {
        "catalog" if options.descending => rows.reverse(),
        "name" | "title" if options.descending => {
            rows.sort_by_cached_key(|release| std::cmp::Reverse(text::normalize(&release.title)))
        }
        "name" | "title" => rows.sort_by_cached_key(|release| text::normalize(&release.title)),
        "year" => rows.sort_by_cached_key(|release| {
            (
                release.year.is_none(),
                if options.descending {
                    u32::MAX - release.year.unwrap_or_default()
                } else {
                    release.year.unwrap_or_default()
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
        .map(|release| {
            Ok(ReleaseItem {
                reference: reference(catalog, EntityKind::Release, release.id)
                    .ok_or_else(unavailable)?,
                title: release.title.clone(),
                year: release.year,
                album_artist: release
                    .album_artist_id
                    .and_then(|id| reference(catalog, EntityKind::Artist, id)),
                track_count: release.track_ids.len(),
                cover_path: release.cover_path.clone(),
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    })
}

fn browse_item(catalog: &Catalog, kind: EntityKind, id: Id) -> Result<BrowseItem, ApiError> {
    let reference = reference(catalog, kind, id).ok_or_else(unavailable)?;
    Ok(match kind {
        EntityKind::Genre => BrowseItem::Genre {
            reference,
            name: catalog.genre(id).ok_or_else(unavailable)?.name.clone(),
            release_count: catalog.releases_of_genre(id).len(),
            track_count: catalog.tracks_of_genre(id).len(),
        },
        EntityKind::Label => {
            let label = catalog.label(id).ok_or_else(unavailable)?;
            BrowseItem::Label {
                reference,
                name: label.name.clone(),
                mbid: label.mbid.clone(),
                release_count: catalog.releases_of_label(id).len(),
            }
        }
        EntityKind::Work => {
            let work = catalog.work(id).ok_or_else(unavailable)?;
            BrowseItem::Work {
                reference,
                title: work.title.clone(),
                mbid: work.mbid.clone(),
                recording_count: work.recording_ids.len(),
            }
        }
        EntityKind::ReleaseGroup => {
            let group = catalog.release_group(id).ok_or_else(unavailable)?;
            BrowseItem::ReleaseGroup {
                reference,
                title: group.title.clone(),
                mbid: group.mbid.clone(),
                release_count: group.release_ids.len(),
            }
        }
        _ => return Err(invalid_query("unsupported browse kind")),
    })
}

async fn browse(
    State(state): State<ApiState>,
    query: Result<Query<BrowseQuery>, QueryRejection>,
    kind: EntityKind,
) -> Result<Json<Page<BrowseItem>>, ApiError> {
    let Query(query) = query.map_err(|rejection| invalid_query(rejection.body_text()))?;
    let options = browse_options(&query, kind)?;
    crate::inspection::inspect(state, move |state| {
        browse_page(state, &query, &options, kind)
    })
    .await
}

fn browse_page(
    state: &ApiState,
    query: &BrowseQuery,
    options: &BrowseOptions,
    kind: EntityKind,
) -> Result<Page<BrowseItem>, ApiError> {
    let guard = state.catalog.blocking_read();
    let catalog = guard.as_ref().ok_or_else(unavailable)?;
    let mut rows: Vec<Id> = match kind {
        EntityKind::Genre => catalog.genres.iter().map(|row| row.id).collect(),
        EntityKind::Label => catalog.labels.iter().map(|row| row.id).collect(),
        EntityKind::Work => catalog.works.iter().map(|row| row.id).collect(),
        EntityKind::ReleaseGroup => catalog.release_groups.iter().map(|row| row.id).collect(),
        _ => return Err(invalid_query("unsupported browse kind")),
    };
    rows.retain(|&id| {
        options.name.as_ref().is_none_or(|name| {
            entity_name(catalog, kind, id)
                .is_some_and(|value| text::normalize(value).contains(name))
        }) && query
            .mbid
            .as_deref()
            .is_none_or(|mbid| entity_mbid(catalog, kind, id) == Some(mbid))
    });
    if options.sort != "catalog" {
        if options.descending {
            rows.sort_by_cached_key(|&id| {
                std::cmp::Reverse(text::normalize(
                    entity_name(catalog, kind, id).unwrap_or_default(),
                ))
            });
        } else {
            rows.sort_by_cached_key(|&id| {
                text::normalize(entity_name(catalog, kind, id).unwrap_or_default())
            });
        }
    } else if options.descending {
        rows.reverse();
    }
    let total = rows.len();
    let items = rows
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .map(|id| browse_item(catalog, kind, id))
        .collect::<Result<_, _>>()?;
    Ok(Page {
        items,
        total,
        offset: options.offset,
        limit: options.limit,
        scanned_at: catalog.scanned_at,
    })
}

#[cfg(test)]
#[path = "catalog_commands_tests.rs"]
mod tests;
