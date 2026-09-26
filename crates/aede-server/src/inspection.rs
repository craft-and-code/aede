//! Structured, read-only diagnostics and searches over the core catalog.

use super::*;
use aede_core::{doctor, query, sources, stats};
use std::collections::BTreeMap;

const MAX_TEXT_BYTES: usize = 2048;
const MAX_QUERY_PARTS: usize = 64;
const ISSUE_FILE_PREVIEW: usize = 20;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/doctor", get(diagnose))
        .route("/api/v1/stats", get(statistics))
        .route("/api/v1/roots", get(roots))
        .route("/api/v1/roles", get(roles))
        .route("/api/v1/search", get(search))
        .route("/api/v1/query", get(run_query))
        .route("/api/v1/countries", get(countries))
        .route("/api/v1/years", get(years))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    offset: Option<String>,
    limit: Option<String>,
    q: Option<String>,
    comments: Option<String>,
    severity: Option<String>,
    sort: Option<String>,
}

#[derive(Clone, Copy)]
struct Window {
    offset: usize,
    limit: usize,
}

impl Window {
    fn page<T>(self, rows: Vec<T>, scanned_at: u64) -> Page<T> {
        Page {
            total: rows.len(),
            items: rows
                .into_iter()
                .skip(self.offset)
                .take(self.limit)
                .collect(),
            offset: self.offset,
            limit: self.limit,
            scanned_at,
        }
    }
}

fn parameters(
    input: Result<Query<Parameters>, QueryRejection>,
    allowed: &[&str],
) -> Result<(Parameters, Window), ApiError> {
    let Query(params) = input.map_err(|rejection| invalid_query(rejection.body_text()))?;
    for (name, present) in [
        ("q", params.q.is_some()),
        ("comments", params.comments.is_some()),
        ("severity", params.severity.is_some()),
        ("sort", params.sort.is_some()),
    ] {
        if present && !allowed.contains(&name) {
            return Err(invalid_query(format!(
                "{name} is not supported for this route"
            )));
        }
    }
    let offset = params
        .offset
        .as_deref()
        .map(|value| decimal(value, "offset", true))
        .transpose()?
        .unwrap_or(0);
    let limit = params
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
    Ok((params, Window { offset, limit }))
}

fn required_text(params: &Parameters) -> Result<&str, ApiError> {
    let value = params
        .q
        .as_deref()
        .ok_or_else(|| invalid_query("q is required"))?;
    if value.len() > MAX_TEXT_BYTES || text::normalize(value).is_empty() {
        return Err(invalid_query(format!(
            "q must contain text and be at most {MAX_TEXT_BYTES} bytes"
        )));
    }
    Ok(value)
}

pub(super) fn held_sources(data_dir: &Path) -> Result<sources::Sources, ApiError> {
    sources::load(&sources::sources_path(data_dir))
        .map(|held| held.unwrap_or_default())
        .map_err(|failure| {
            eprintln!("API source inspection failed: {failure}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "sources_unavailable",
                "stored source claims could not be read",
            )
        })
}

/// CPU-bound graph inspection and local store reads never occupy a runtime
/// worker. Keep the permit until the blocking work finishes, even if its HTTP
/// client disconnects; dropping a handler must not bypass the admission limit.
pub(super) async fn inspect<T, F>(state: ApiState, operation: F) -> Result<Json<T>, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&ApiState) -> Result<T, ApiError> + Send + 'static,
{
    let permit = state
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::TOO_MANY_REQUESTS,
                "inspection_busy",
                "too many catalog inspections; retry later",
            )
        })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation(&state)
    })
    .await
    .map_err(|failure| {
        eprintln!("API catalog inspection failed: {failure}");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "inspection_failed",
            "catalog inspection could not finish",
        )
    })?
    .map(Json)
}

#[derive(Serialize)]
struct IssueItem {
    #[serde(rename = "type")]
    kind: &'static str,
    label: &'static str,
    severity: &'static str,
    detail: String,
    files: Vec<String>,
    file_count: usize,
    files_truncated: bool,
}

fn issue_code(kind: doctor::IssueKind) -> &'static str {
    use doctor::IssueKind::*;
    match kind {
        MissingTitle => "missing_title",
        MissingArtist => "missing_artist",
        MissingAlbum => "missing_album",
        MissingDate => "missing_date",
        MissingTrackNumber => "missing_track_number",
        MissingDuration => "missing_duration",
        DuplicateTrack => "duplicate_track",
        SameAudio => "same_audio",
        DuplicateAlbum => "duplicate_album",
        OtherEdition => "other_edition",
        Md5Mismatch => "md5_mismatch",
        IncompleteAlbum => "incomplete_album",
        MixedQuality => "mixed_quality",
        MissingCover => "missing_cover",
        SuspiciousYear => "suspicious_year",
        DamagedAudio => "damaged_audio",
        SameArtistMaybe => "same_artist_maybe",
        SourceDisagrees => "source_disagrees",
        SourcesDisagree => "sources_disagree",
        SourceNeedsReview => "source_needs_review",
        SourceIdentityConflict => "source_identity_conflict",
        IncompleteSourceCredits => "incomplete_source_credits",
        OrphanedRelationAnnotation => "orphaned_relation_annotation",
    }
}

#[derive(Serialize)]
struct Diagnosis {
    #[serde(flatten)]
    page: Page<IssueItem>,
    summary: BTreeMap<&'static str, usize>,
    unverified_files: usize,
    pending_analyses: usize,
}

fn diagnosis(
    catalog: &Catalog,
    held: &sources::Sources,
    severity: Option<&str>,
    window: Window,
) -> Diagnosis {
    let mut issues = doctor::diagnose(catalog, held);
    if let Some(severity) = severity {
        issues.retain(|issue| issue.severity().label() == severity);
    }
    let mut summary = BTreeMap::from([("error", 0), ("warning", 0), ("info", 0)]);
    for (severity, count) in doctor::summary(&issues) {
        summary.insert(severity.label(), count);
    }
    let items = issues
        .into_iter()
        .map(|issue| {
            let file_count = issue.files.len();
            IssueItem {
                kind: issue_code(issue.kind),
                label: issue.kind.label(),
                severity: issue.severity().label(),
                detail: issue.detail,
                files: issue.files.into_iter().take(ISSUE_FILE_PREVIEW).collect(),
                file_count,
                files_truncated: file_count > ISSUE_FILE_PREVIEW,
            }
        })
        .collect();
    Diagnosis {
        page: window.page(items, catalog.scanned_at),
        summary,
        unverified_files: catalog
            .files
            .iter()
            .filter(|file| file.integrity.is_none())
            .count(),
        pending_analyses: catalog.pending_analyses(),
    }
}

async fn diagnose(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Diagnosis>, ApiError> {
    let (params, window) = parameters(input, &["severity"])?;
    if params
        .severity
        .as_deref()
        .is_some_and(|value| !["error", "warning", "info"].contains(&value))
    {
        return Err(invalid_query("severity must be error, warning or info"));
    }
    inspect(state, move |state| {
        // A check/import can update conclusions without replacing the catalog.
        // Read all diagnostic inputs under the writer lock instead of reporting
        // stale integrity findings from the cached catalog snapshot.
        let _guard = StoreLock::try_acquire(&state.data_dir).map_err(|failure| {
            if failure.kind() == std::io::ErrorKind::WouldBlock {
                error(
                    StatusCode::CONFLICT,
                    "store_busy",
                    "Aède data is busy; retry after the current write",
                )
            } else {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "store_error",
                    "Aède data could not be locked for diagnosis",
                )
            }
        })?;
        let catalog = store::load(&store::catalog_path(&state.data_dir))
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "catalog_read_failed",
                    "catalog or conclusions could not be read",
                )
            })?
            .ok_or_else(unavailable)?;
        let held = held_sources(&state.data_dir)?;
        Ok(diagnosis(
            &catalog,
            &held,
            params.severity.as_deref(),
            window,
        ))
    })
    .await
}

#[derive(Serialize)]
struct Bucket {
    label: String,
    count: usize,
    bytes: u64,
}

fn buckets(rows: Vec<stats::Bucket>, window: Window, scanned_at: u64) -> Page<Bucket> {
    window.page(
        rows.into_iter()
            .map(|row| Bucket {
                label: row.label,
                count: row.count,
                bytes: row.bytes,
            })
            .collect(),
        scanned_at,
    )
}

#[derive(Serialize)]
struct RoleItem {
    role: String,
    artists: usize,
    credits: usize,
}

fn role_items(catalog: &Catalog) -> Vec<RoleItem> {
    catalog
        .roles_in_use()
        .into_iter()
        .map(|role| {
            let artists = catalog.artists_in_role(role);
            RoleItem {
                role: role.to_string(),
                artists: artists.len(),
                credits: artists.iter().map(|(_, count)| count).sum(),
            }
        })
        .collect()
}

#[derive(Serialize)]
struct ArtistCount {
    reference: Option<String>,
    name: String,
    tracks: usize,
}

fn artist_counts(catalog: &Catalog, counts: Vec<(Id, usize)>, window: Window) -> Page<ArtistCount> {
    window.page(
        counts
            .into_iter()
            .filter_map(|(id, tracks)| {
                catalog.artist(id).map(|artist| ArtistCount {
                    reference: reference(catalog, EntityKind::Artist, id),
                    name: artist.name.clone(),
                    tracks,
                })
            })
            .collect(),
        catalog.scanned_at,
    )
}

#[derive(Serialize)]
struct Completeness {
    covers: f64,
    years: f64,
    genres: f64,
    mbid: f64,
}

#[derive(Serialize)]
struct Top {
    artists: Page<ArtistCount>,
    writers: Page<ArtistCount>,
}

#[derive(Serialize)]
struct Statistics {
    scanned_at: u64,
    files: usize,
    tracks: usize,
    albums: usize,
    compilations: usize,
    artists: usize,
    album_artists: usize,
    labels: usize,
    genres: usize,
    duration_ms: u64,
    bytes: u64,
    tracks_without_album: usize,
    completeness: Completeness,
    by_codec: Page<Bucket>,
    by_quality: Page<Bucket>,
    by_sample_rate: Page<Bucket>,
    by_decade: Page<Bucket>,
    by_country: Page<Bucket>,
    roles: Page<RoleItem>,
    top: Top,
}

fn statistics_for(catalog: &Catalog, held: &sources::Sources, window: Window) -> Statistics {
    let computed = stats::compute(catalog);
    let scanned_at = catalog.scanned_at;
    let countries = aede_core::places::countries(catalog, held)
        .into_iter()
        .map(|place| stats::Bucket {
            label: place.name,
            count: place.artists.len(),
            bytes: 0,
        })
        .collect();
    Statistics {
        scanned_at,
        files: computed.files,
        tracks: computed.tracks,
        albums: computed.releases,
        compilations: computed.compilations,
        artists: computed.artists,
        album_artists: computed.album_artists,
        labels: computed.labels,
        genres: computed.genres,
        duration_ms: computed.total_duration_ms,
        bytes: computed.total_bytes,
        tracks_without_album: computed.orphan_tracks,
        completeness: Completeness {
            covers: computed.cover_ratio,
            years: computed.year_ratio,
            genres: computed.genre_ratio,
            mbid: computed.mbid_ratio,
        },
        by_codec: buckets(computed.by_codec, window, scanned_at),
        by_quality: buckets(computed.by_quality, window, scanned_at),
        by_sample_rate: buckets(computed.by_sample_rate, window, scanned_at),
        by_decade: buckets(computed.by_decade, window, scanned_at),
        by_country: buckets(countries, window, scanned_at),
        roles: window.page(role_items(catalog), scanned_at),
        top: Top {
            artists: artist_counts(catalog, stats::top_artists(catalog, usize::MAX), window),
            writers: artist_counts(catalog, stats::top_writers(catalog, usize::MAX), window),
        },
    }
}

async fn statistics(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Statistics>, ApiError> {
    let (_, window) = parameters(input, &[])?;
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        Ok(statistics_for(
            catalog,
            &held_sources(&state.data_dir)?,
            window,
        ))
    })
    .await
}

async fn roles(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Page<RoleItem>>, ApiError> {
    let (_, window) = parameters(input, &[])?;
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        Ok(window.page(role_items(catalog), catalog.scanned_at))
    })
    .await
}

#[derive(Serialize)]
struct RootItem {
    status: &'static str,
    path: Option<String>,
    tracks: usize,
    duration_ms: u64,
    bytes: u64,
}

fn root_item(
    catalog: &Catalog,
    status: &'static str,
    path: Option<String>,
    tracks: impl Iterator<Item = Id>,
) -> RootItem {
    let mut item = RootItem {
        status,
        path,
        tracks: 0,
        duration_ms: 0,
        bytes: 0,
    };
    for track in tracks.filter_map(|id| catalog.track(id)) {
        item.tracks += 1;
        item.duration_ms = item
            .duration_ms
            .saturating_add(track.duration_ms.unwrap_or(0));
        item.bytes = item.bytes.saturating_add(
            catalog
                .file(track.file_id)
                .map(|file| file.size)
                .unwrap_or(0),
        );
    }
    item
}

fn root_items(catalog: &Catalog) -> Vec<RootItem> {
    let tracks_under = |root: &str| {
        catalog
            .tracks
            .iter()
            .filter_map(|track| {
                catalog
                    .file(track.file_id)
                    .filter(|file| text::is_under(&file.path, root))
                    .map(|_| track.id)
            })
            .collect::<Vec<_>>()
    };
    let mut watched = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for root in &catalog.roots {
        let tracks = tracks_under(root);
        watched.extend(tracks.iter().copied());
        items.push(root_item(
            catalog,
            "watched",
            Some(root.clone()),
            tracks.into_iter(),
        ));
    }
    for root in &catalog.excluded {
        items.push(root_item(
            catalog,
            "excluded",
            Some(root.clone()),
            tracks_under(root).into_iter(),
        ));
    }
    let orphaned: Vec<_> = catalog
        .tracks
        .iter()
        .map(|track| track.id)
        .filter(|id| !watched.contains(id))
        .collect();
    if !orphaned.is_empty() {
        items.push(root_item(catalog, "unwatched", None, orphaned.into_iter()));
    }
    items
}

#[derive(Serialize)]
struct RootTotals {
    tracks: usize,
    duration_ms: u64,
    bytes: u64,
}

#[derive(Serialize)]
struct Roots {
    #[serde(flatten)]
    page: Page<RootItem>,
    totals: RootTotals,
}

async fn roots(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Roots>, ApiError> {
    let (_, window) = parameters(input, &[])?;
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let all = root_item(
            catalog,
            "all",
            None,
            catalog.tracks.iter().map(|track| track.id),
        );
        Ok(Roots {
            page: window.page(root_items(catalog), catalog.scanned_at),
            totals: RootTotals {
                tracks: all.tracks,
                duration_ms: all.duration_ms,
                bytes: all.bytes,
            },
        })
    })
    .await
}

#[derive(Serialize)]
struct SearchItem {
    kind: &'static str,
    reference: Option<String>,
    name: String,
    context: String,
    found_in: &'static str,
}

fn search_items(catalog: &Catalog, wanted: &str, comments: bool) -> Vec<SearchItem> {
    let mut items: Vec<_> = catalog
        .search(wanted, usize::MAX)
        .into_iter()
        .map(|hit| SearchItem {
            kind: hit.kind.as_str(),
            reference: reference(catalog, hit.kind, hit.id),
            name: hit.name,
            context: hit.detail,
            found_in: "name",
        })
        .collect();
    if comments {
        items.extend(
            catalog
                .tracks_with_comment(wanted)
                .into_iter()
                .filter_map(|id| {
                    catalog.track(id).map(|track| SearchItem {
                        kind: "track",
                        reference: reference(catalog, EntityKind::Track, id),
                        name: track.title.clone(),
                        context: catalog.comment_of_track(id).unwrap_or_default().to_string(),
                        found_in: "comment",
                    })
                }),
        );
    }
    items
}

async fn search(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Page<SearchItem>>, ApiError> {
    let (params, window) = parameters(input, &["q", "comments"])?;
    let wanted = required_text(&params)?.to_string();
    let comments = match params.comments.as_deref().unwrap_or("false") {
        "true" => true,
        "false" => false,
        _ => return Err(invalid_query("comments must be true or false")),
    };
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        Ok(window.page(search_items(catalog, &wanted, comments), catalog.scanned_at))
    })
    .await
}

fn public_query(expression: &str) -> Result<query::Query, ApiError> {
    // Bound tokens/parentheses/unary negations before the recursive core parser.
    // Quoted values remain intact, using the parser's single/double quote rules.
    let mut quote = None;
    let mut in_word = false;
    let mut parts = 0;
    for ch in expression.chars() {
        match quote {
            Some(current) if current == ch => quote = None,
            Some(_) => {}
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                if !in_word {
                    parts += 1;
                }
                in_word = true;
            }
            None if ch == '(' || ch == ')' || ch == '-' => {
                parts += 1;
                in_word = false;
            }
            None if ch.is_whitespace() => in_word = false,
            None => {
                if !in_word {
                    parts += 1;
                }
                in_word = true;
            }
        }
        if parts > MAX_QUERY_PARTS {
            return Err(invalid_query(
                "query has too many terms, parentheses or negations",
            ));
        }
    }
    let parsed = query::parse(expression).map_err(|failure| invalid_query(failure.to_string()))?;
    let mut pending = vec![&parsed];
    while let Some(part) = pending.pop() {
        match part {
            query::Query::And(parts) | query::Query::Or(parts) => pending.extend(parts),
            query::Query::Not(part) => pending.push(part),
            query::Query::Term(term) => match term.field {
                query::Field::Rating(_)
                | query::Field::Loved(_)
                | query::Field::LovedAnywhere
                | query::Field::Tag(_)
                | query::Field::Note(_)
                | query::Field::Played => {
                    return Err(invalid_query(
                        "personal query fields require authenticated accounts and are not exposed",
                    ));
                }
                query::Field::Lyrics => {
                    return Err(invalid_query(
                        "lyrics queries are not exposed by the catalog API",
                    ));
                }
                query::Field::Title
                | query::Field::Artist
                | query::Field::Album
                | query::Field::Recording
                | query::Field::Work
                | query::Field::ReleaseGroup
                | query::Field::AlbumArtist
                | query::Field::Genre
                | query::Field::Label
                | query::Field::Comment
                | query::Field::Path
                | query::Field::Codec
                | query::Field::Year
                | query::Field::Duration
                | query::Field::Size
                | query::Field::Bitrate
                | query::Field::SampleRate
                | query::Field::Lossless
                | query::Field::Compilation
                | query::Field::Performing
                | query::Field::Credit(_)
                | query::Field::Instrument
                | query::Field::Guest
                | query::Field::CompilationArtist
                | query::Field::Contributor
                | query::Field::Collaborator
                | query::Field::Anything => {}
            },
            query::Query::All => {}
        }
    }
    Ok(parsed)
}

fn query_items(
    catalog: &Catalog,
    held: &sources::Sources,
    parsed: &query::Query,
    sort: query::Sort,
) -> Result<Vec<TrackItem>, ApiError> {
    let empty_private_data = aede_core::user::UserData::default();
    let context = query::Context::new(catalog, &empty_private_data, "").with_sources(held);
    if let Some((field, value)) = query::unknown_values(parsed, &context).first() {
        return Err(invalid_query(format!("no {field} matches {value:?}")));
    }
    let mut tracks = query::run(parsed, &context);
    query::sort(&mut tracks, sort, &context);
    Ok(tracks
        .into_iter()
        .filter_map(|id| {
            catalog.track(id).map(|track| TrackItem {
                reference: reference(catalog, EntityKind::Track, id).unwrap_or_default(),
                title: track.title.clone(),
                release: track
                    .release_id
                    .and_then(|id| reference(catalog, EntityKind::Release, id)),
                recording: reference(catalog, EntityKind::Recording, track.recording_id),
                duration_ms: track.duration_ms,
            })
        })
        .collect())
}

async fn run_query(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Page<TrackItem>>, ApiError> {
    let (params, window) = parameters(input, &["q", "sort"])?;
    let parsed = public_query(required_text(&params)?)?;
    let sort = query::Sort::parse(params.sort.as_deref().unwrap_or("catalog"))
        .map_err(|failure| invalid_query(failure.to_string()))?;
    if matches!(sort.key, query::SortKey::Rating | query::SortKey::Played) {
        return Err(invalid_query("personal sort fields are not exposed"));
    }
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let items = query_items(catalog, &held_sources(&state.data_dir)?, &parsed, sort)?;
        Ok(window.page(items, catalog.scanned_at))
    })
    .await
}

#[derive(Serialize)]
struct CountryItem {
    name: String,
    iso_code: Option<String>,
    derived_initials: Option<String>,
    artists: usize,
    tracks: usize,
    duration_ms: u64,
    bytes: u64,
}

#[derive(Serialize)]
struct CountryCoverage {
    artists_total: usize,
    artists_asked: usize,
    artists_with_area: usize,
    artists_without_area: usize,
    places_without_iso_code: usize,
}

#[derive(Serialize)]
struct Countries {
    #[serde(flatten)]
    page: Page<CountryItem>,
    source: &'static str,
    matched_by: Option<&'static str>,
    coverage: CountryCoverage,
}

fn country_rows(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: Option<&str>,
    window: Window,
) -> Result<Countries, ApiError> {
    let mut places = aede_core::places::countries(catalog, held);
    let known = places.iter().map(|place| place.artists.len()).sum();
    let coverage = CountryCoverage {
        artists_total: catalog.artists.len(),
        artists_asked: aede_core::places::asked_about(catalog, held),
        artists_with_area: known,
        artists_without_area: catalog.artists.len().saturating_sub(known),
        places_without_iso_code: aede_core::places::without_code(&places),
    };
    let matched_by = if let Some(wanted) = wanted {
        let (found, matched) = aede_core::places::find(&places, wanted);
        if found.is_empty() {
            return Err(invalid_query("no known country or area matches q"));
        }
        places = found;
        Some(match matched {
            aede_core::model::TitleMatch::Exact => "exact",
            aede_core::model::TitleMatch::Partial => "partial",
        })
    } else {
        None
    };
    let items = places
        .into_iter()
        .map(|place| {
            let tracks: std::collections::BTreeSet<_> = place
                .artists
                .iter()
                .flat_map(|&id| catalog.tracks_of_artist(id))
                .collect();
            let totals = root_item(catalog, "country", None, tracks.into_iter());
            CountryItem {
                name: place.name,
                iso_code: place.code,
                derived_initials: place.initials,
                artists: place.artists.len(),
                tracks: totals.tracks,
                duration_ms: totals.duration_ms,
                bytes: totals.bytes,
            }
        })
        .collect();
    Ok(Countries {
        page: window.page(items, catalog.scanned_at),
        source: sources::MUSICBRAINZ,
        matched_by,
        coverage,
    })
}

async fn countries(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Countries>, ApiError> {
    let (params, window) = parameters(input, &["q"])?;
    let wanted = params
        .q
        .as_ref()
        .map(|_| required_text(&params).map(str::to_string))
        .transpose()?;
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        country_rows(
            catalog,
            &held_sources(&state.data_dir)?,
            wanted.as_deref(),
            window,
        )
    })
    .await
}

#[derive(Serialize)]
struct YearItem {
    year: u32,
    albums: usize,
    tracks: usize,
    duration_ms: u64,
    bytes: u64,
}

#[derive(Serialize)]
struct Years {
    #[serde(flatten)]
    page: Page<YearItem>,
    albums_total: usize,
    albums_without_year: usize,
    tracks_without_dated_album: usize,
}

fn year_rows(catalog: &Catalog, window: Window) -> Years {
    let mut groups: BTreeMap<u32, (usize, Vec<Id>)> = BTreeMap::new();
    let mut dated_tracks = std::collections::BTreeSet::new();
    let mut albums_without_year = 0;
    for release in &catalog.releases {
        let Some(year) = release.year else {
            albums_without_year += 1;
            continue;
        };
        let group = groups.entry(year).or_default();
        group.0 += 1;
        group.1.extend(release.track_ids.iter().copied());
        dated_tracks.extend(release.track_ids.iter().copied());
    }
    let items = groups
        .into_iter()
        .map(|(year, (albums, tracks))| {
            let totals = root_item(catalog, "year", None, tracks.into_iter());
            YearItem {
                year,
                albums,
                tracks: totals.tracks,
                duration_ms: totals.duration_ms,
                bytes: totals.bytes,
            }
        })
        .collect();
    Years {
        page: window.page(items, catalog.scanned_at),
        albums_total: catalog.releases.len(),
        albums_without_year,
        tracks_without_dated_album: catalog.tracks.len().saturating_sub(dated_tracks.len()),
    }
}

async fn years(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
) -> Result<Json<Years>, ApiError> {
    let (_, window) = parameters(input, &[])?;
    inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        Ok(year_rows(catalog, window))
    })
    .await
}

#[cfg(test)]
#[path = "inspection_tests.rs"]
mod tests;
