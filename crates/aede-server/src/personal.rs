//! Personal reads and writes bound to the authenticated account's owner.
//!
//! Transitional administrative routes retain the explicit local-owner scope.
//! The account routes share these operations and never accept an owner field.

use std::collections::{BTreeMap, BTreeSet};

use aede_core::clock;
use aede_core::user::{self, Annotation, EntityRef, LOCAL_USER, Play, UserData};
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use super::*;

const MAX_BODY: usize = 16 * 1024;
const MAX_TAGS: usize = 100;
const MAX_TAG_BYTES: usize = 256;
const MAX_HISTORY_MS: u64 = 24 * 60 * 60 * 1000;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/api/admin/v1/annotation",
            get(annotation).put(update_annotation),
        )
        .route("/api/admin/v1/history", get(history).post(record_history))
        .route(
            "/api/admin/v1/collection",
            get(collection)
                .put(save_collection)
                .delete(delete_collection),
        )
        .route("/api/admin/v1/collections", get(collections))
}

pub(super) fn account_routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/api/me/v1/annotation",
            get(annotation).put(update_annotation),
        )
        .route("/api/me/v1/history", get(history).post(record_history))
        .route(
            "/api/me/v1/collection",
            get(collection)
                .put(save_collection)
                .delete(delete_collection),
        )
        .route("/api/me/v1/collections", get(collections))
}

struct PersonalOwner {
    id: String,
    principal: Option<auth::Principal>,
}

fn spawn_personal<T: Send + 'static>(
    slots: Arc<Semaphore>,
    work: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<tokio::task::JoinHandle<Result<T, ApiError>>, ApiError> {
    let permit = slots.try_acquire_owned().map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "personal_busy",
            "personal data workers are busy; retry shortly",
        )
    })?;
    Ok(tokio::task::spawn_blocking(move || {
        // Keep the slot until the blocking work finishes even if HTTP disconnects.
        let _permit = permit;
        work()
    }))
}

impl PersonalOwner {
    fn from_request(state: &ApiState, request: &Request) -> Result<Self, ApiError> {
        if request.uri().path().starts_with("/api/admin/v1/") {
            require_admin(state, request)?;
            Ok(Self {
                id: LOCAL_USER.into(),
                principal: request.extensions().get::<auth::Principal>().cloned(),
            })
        } else {
            let principal = auth::request_principal(request)?;
            Ok(Self {
                id: principal.owner.clone(),
                principal: Some(principal),
            })
        }
    }

    fn recheck(&self, state: &ApiState) -> Result<(), ApiError> {
        match &self.principal {
            Some(principal) => auth::recheck(state, principal),
            None => Ok(()),
        }
    }

    /// Administrative-token routes have no account role; session routes must
    /// refuse the explicit read-only role before parsing or changing a body.
    fn require_mutation(&self) -> Result<(), ApiError> {
        match &self.principal {
            Some(principal) => auth::require_mutation(principal),
            None => Ok(()),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefQuery {
    #[serde(rename = "ref")]
    reference: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NameQuery {
    name: String,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    offset: Option<String>,
    limit: Option<String>,
}

/// Distinguishes an omitted field from an explicit JSON null.
#[derive(Default)]
enum Field<T> {
    #[default]
    Missing,
    Value(Option<T>),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(Self::Value)
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotationPatch {
    #[serde(default)]
    loved: Field<bool>,
    #[serde(default)]
    rating: Field<u8>,
    #[serde(default)]
    note: Field<String>,
    #[serde(default)]
    tags: Field<Vec<String>>,
}

impl AnnotationPatch {
    fn validate(&self) -> Result<(), ApiError> {
        if matches!(
            self,
            Self {
                loved: Field::Missing,
                rating: Field::Missing,
                note: Field::Missing,
                tags: Field::Missing,
            }
        ) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "provide at least one annotation field",
            ));
        }
        if matches!(self.loved, Field::Value(None)) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "loved must be true or false",
            ));
        }
        if matches!(self.rating, Field::Value(Some(0 | 6..))) {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "rating must be from 1 to 5, or null to remove it",
            ));
        }
        if let Field::Value(Some(note)) = &self.note
            && note.trim().is_empty()
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "note must contain text, or be null to remove it",
            ));
        }
        if let Field::Value(Some(tags)) = &self.tags {
            validate_tags(tags)?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlayRequest {
    track: String,
    #[serde(default)]
    at: Option<u64>,
    ms_played: u64,
    completed: bool,
}

impl PlayRequest {
    fn validate(&self, now: u64) -> Result<EntityRef, ApiError> {
        let track = EntityRef::parse_token(&self.track)
            .filter(|reference| reference.kind == EntityKind::Track && !reference.key.is_empty())
            .ok_or_else(|| invalid_reference("track must be a nonempty track reference"))?;
        if self.ms_played > MAX_HISTORY_MS {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "ms_played must be at most 24 hours",
            ));
        }
        if self
            .at
            .is_some_and(|at| at > now.saturating_add(24 * 60 * 60))
        {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "invalid_parameters",
                "at cannot be more than one day in the future",
            ));
        }
        Ok(track)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionRequest {
    expression: String,
}

fn parsed_query<T>(input: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    input
        .map(|Query(value)| value)
        .map_err(|rejection| invalid_query(rejection.body_text()))
}

pub(super) fn no_query(request: &Request) -> Result<(), ApiError> {
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(invalid_query("this route accepts no query parameters"));
    }
    Ok(())
}

pub(super) async fn body(request: Request) -> Result<axum::body::Bytes, ApiError> {
    tokio::time::timeout(
        Duration::from_secs(1),
        to_bytes(request.into_body(), MAX_BODY),
    )
    .await
    .map_err(|_| {
        error(
            StatusCode::REQUEST_TIMEOUT,
            "request_timeout",
            "request body did not finish",
        )
    })?
    .map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "request body is malformed or too large",
        )
    })
}

pub(super) async fn json_body<T: DeserializeOwned>(request: Request) -> Result<T, ApiError> {
    let bytes = body(request).await?;
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "expected a JSON object",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "expected a JSON object with supported fields and types",
        )
    })
}

pub(super) fn empty_body(bytes: &[u8]) -> Result<(), ApiError> {
    if bytes.iter().any(|byte| !byte.is_ascii_whitespace()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "this route accepts no request body",
        ));
    }
    Ok(())
}

fn reference(query: RefQuery) -> Result<EntityRef, ApiError> {
    bounded_text(&query.reference, "ref", MAX_REFERENCE_BYTES)?;
    EntityRef::parse_token(&query.reference)
        .filter(|reference| !reference.key.is_empty())
        .ok_or_else(|| invalid_reference("ref must be a nonempty entity reference"))
}

fn existing_target(catalog: &Catalog, reference: &EntityRef) -> Result<(), ApiError> {
    reference.resolve(catalog).map(|_| ()).ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "entity_not_found",
            "no entity matches this reference",
        )
    })
}

fn load_personal(data_dir: &Path, catalog: &Catalog) -> Result<UserData, ApiError> {
    let path = user::user_path(data_dir);
    let mut data = user::load(&path)
        .map_err(|failure| {
            eprintln!("API user data read failed: {failure}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "user_unavailable",
                "personal data could not be read",
            )
        })?
        .unwrap_or_default();
    user::reconcile(&mut data, catalog);
    Ok(data)
}

// Catalog and user data must describe the same completed writer operation.
// A cached catalog can predate user.json and reconcile a new track's note onto
// a different track. Keep the lock through validation and any resulting save.
fn locked_personal(data_dir: &Path) -> Result<(StoreLock, Catalog, UserData), ApiError> {
    let guard = StoreLock::try_acquire(data_dir).map_err(|failure| {
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
                "Aède data could not be locked for the update",
            )
        }
    })?;
    let catalog = store::load(&store::catalog_path(data_dir))
        .map_err(|failure| {
            eprintln!("API personal catalog read failed: {failure}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "the current catalog could not be read",
            )
        })?
        .ok_or_else(unavailable)?;
    let data = load_personal(data_dir, &catalog)?;
    Ok((guard, catalog, data))
}

fn locked_update<T>(
    data_dir: &Path,
    state: &ApiState,
    owner: &PersonalOwner,
    update: impl FnOnce(&Catalog, &mut UserData) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let (_guard, catalog, mut data) = locked_personal(data_dir)?;
    owner.recheck(state)?;
    let result = update(&catalog, &mut data)?;
    data.forget_empty();
    user::save(&data, &user::user_path(data_dir)).map_err(|failure| {
        eprintln!("API user data write failed: {failure}");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "personal data could not be saved",
        )
    })?;
    Ok(result)
}

#[derive(Debug, Serialize)]
struct AnnotationView {
    reference: String,
    loved: bool,
    rating: Option<u8>,
    note: Option<String>,
    tags: Vec<String>,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Serialize)]
struct AnnotationResponse {
    reference: String,
    annotation: Option<AnnotationView>,
}

fn annotation_view(annotation: &Annotation) -> AnnotationView {
    AnnotationView {
        reference: annotation.target.to_token(),
        loved: annotation.loved,
        rating: annotation.rating,
        note: annotation.note.clone(),
        tags: annotation.tags.iter().cloned().collect(),
        created_at: annotation.created_at,
        updated_at: annotation.updated_at,
    }
}

async fn annotation(
    State(state): State<ApiState>,
    input: Result<Query<RefQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<AnnotationResponse>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let reference = reference(parsed_query(input)?)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        existing_target(&catalog, &reference)?;
        Ok(Json(AnnotationResponse {
            reference: reference.to_token(),
            annotation: data.find(&owner.id, &reference).map(annotation_view),
        }))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "personal data read could not finish",
        )
    })?
}

async fn update_annotation(
    State(state): State<ApiState>,
    input: Result<Query<RefQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<AnnotationResponse>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let patch: AnnotationPatch = json_body(request).await?;
    patch.validate()?;
    let reference = reference(parsed_query(input)?)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let now = clock::now_seconds();
        let saved = locked_update(&data_dir, &state, &owner, |catalog, data| {
            existing_target(catalog, &reference)?;
            let entry = data.entry(&owner.id, &reference, now);
            if let Field::Value(Some(loved)) = patch.loved {
                entry.loved = loved;
            }
            if let Field::Value(rating) = patch.rating {
                entry.rating = rating;
            }
            if let Field::Value(note) = patch.note {
                entry.note = note;
            }
            if let Field::Value(tags) = patch.tags {
                entry.tags = tags.unwrap_or_default().into_iter().collect();
            }
            entry.updated_at = now;
            Ok(entry.clone())
        })?;
        Ok(Json(AnnotationResponse {
            reference: reference.to_token(),
            annotation: (!saved.is_empty()).then(|| annotation_view(&saved)),
        }))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "personal data update could not finish",
        )
    })?
}

#[derive(Serialize)]
struct PlayView {
    track: String,
    at: u64,
    ms_played: u64,
    completed: bool,
    play_count: u32,
    last_played: u64,
}

fn play_view(data: &UserData, play: &Play) -> PlayView {
    let count = data
        .counts
        .iter()
        .find(|count| count.owner == play.owner && count.track == play.track);
    play_view_with_count(play, count)
}

fn play_view_with_count(play: &Play, count: Option<&user::PlayCount>) -> PlayView {
    PlayView {
        track: play.track.to_token(),
        at: play.at,
        ms_played: play.ms_played,
        completed: play.completed,
        play_count: count.map_or(0, |count| count.count),
        last_played: count.map_or(0, |count| count.last_played),
    }
}

#[derive(Serialize)]
struct HistoryResponse {
    #[serde(flatten)]
    page: Page<PlayView>,
}

fn history_page(
    data: &UserData,
    catalog: &Catalog,
    owner: &str,
    offset: usize,
    limit: usize,
) -> HistoryResponse {
    let plays = data.plays.iter().filter(|play| play.owner == owner);
    let total = plays.clone().count();
    let mut counts = BTreeMap::new();
    for count in data.counts.iter().filter(|count| count.owner == owner) {
        // Keep the core's first-row behavior for legacy duplicates.
        counts.entry(&count.track).or_insert(count);
    }
    let items = plays
        .rev()
        .skip(offset)
        .take(limit)
        .map(|play| play_view_with_count(play, counts.get(&play.track).copied()))
        .collect();
    HistoryResponse {
        page: Page {
            total,
            items,
            offset,
            limit,
            scanned_at: catalog.scanned_at,
        },
    }
}

fn page(query: PageQuery) -> Result<(usize, usize), ApiError> {
    page_bounds(query.offset.as_deref(), query.limit.as_deref())
}

async fn history(
    State(state): State<ApiState>,
    input: Result<Query<PageQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<HistoryResponse>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let (offset, limit) = page(parsed_query(input)?)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        Ok(Json(history_page(
            &data, &catalog, &owner.id, offset, limit,
        )))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "personal history could not finish",
        )
    })?
}

async fn record_history(
    State(state): State<ApiState>,
    request: Request,
) -> Result<(StatusCode, Json<PlayView>), ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    no_query(&request)?;
    let input: PlayRequest = json_body(request).await?;
    let now = clock::now_seconds();
    let track = input.validate(now)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let at = input.at.unwrap_or(now);
        locked_update(&data_dir, &state, &owner, |catalog, data| {
            existing_target(catalog, &track)?;
            let play = Play {
                owner: owner.id.clone(),
                track: track.clone(),
                at,
                ms_played: input.ms_played,
                completed: input.completed,
            };
            data.record_play(play.clone());
            Ok(play_view(data, &play))
        })
        .map(|view| (StatusCode::CREATED, Json(view)))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "personal history update could not finish",
        )
    })?
}

#[derive(Serialize)]
struct CollectionView {
    name: String,
    expression: String,
    created_at: u64,
    updated_at: u64,
}

fn collection_view(collection: &user::Collection) -> CollectionView {
    CollectionView {
        name: collection.name.clone(),
        expression: collection.expression.clone(),
        created_at: collection.created_at,
        updated_at: collection.updated_at,
    }
}

async fn collection(
    State(state): State<ApiState>,
    input: Result<Query<NameQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<CollectionView>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let query = parsed_query(input)?;
    validate_collection_name(&query.name)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, _catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        data.collection(&owner.id, &query.name)
            .map(collection_view)
            .map(Json)
            .ok_or_else(|| {
                error(
                    StatusCode::NOT_FOUND,
                    "collection_not_found",
                    "no collection has this name",
                )
            })
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "collection read could not finish",
        )
    })?
}

async fn save_collection(
    State(state): State<ApiState>,
    input: Result<Query<NameQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<CollectionView>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let query = parsed_query(input)?;
    validate_collection_name(&query.name)?;
    let input: CollectionRequest = json_body(request).await?;
    if input.expression.trim().is_empty() || input.expression.len() > 2048 {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_parameters",
            "expression must contain 1 to 2048 bytes",
        ));
    }
    aede_core::query::parse(&input.expression).map_err(|failure| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_parameters",
            failure.to_string(),
        )
    })?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let now = clock::now_seconds();
        locked_update(&data_dir, &state, &owner, |_, data| {
            data.save_collection(&owner.id, &query.name, &input.expression, now);
            data.collection(&owner.id, &query.name)
                .map(collection_view)
                .ok_or_else(|| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "personal_failed",
                        "collection was not saved",
                    )
                })
        })
        .map(Json)
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "collection update could not finish",
        )
    })?
}

async fn delete_collection(
    State(state): State<ApiState>,
    input: Result<Query<NameQuery>, QueryRejection>,
    request: Request,
) -> Result<StatusCode, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    empty_body(&body(request).await?)?;
    let query = parsed_query(input)?;
    validate_collection_name(&query.name)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        locked_update(&data_dir, &state, &owner, |_, data| {
            data.forget_collection(&owner.id, &query.name)
                .then_some(StatusCode::NO_CONTENT)
                .ok_or_else(|| {
                    error(
                        StatusCode::NOT_FOUND,
                        "collection_not_found",
                        "no collection has this name",
                    )
                })
        })
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "collection update could not finish",
        )
    })?
}

#[derive(Serialize)]
struct CollectionsResponse {
    #[serde(flatten)]
    page: Page<CollectionView>,
}

async fn collections(
    State(state): State<ApiState>,
    input: Result<Query<PageQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<CollectionsResponse>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let (offset, limit) = page(parsed_query(input)?)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        let mut items: Vec<_> = data
            .collections
            .iter()
            .filter(|collection| collection.owner == owner.id)
            .map(collection_view)
            .collect();
        items.sort_by(|left, right| {
            text::normalize(&left.name)
                .cmp(&text::normalize(&right.name))
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(Json(CollectionsResponse {
            page: Page {
                total: items.len(),
                items: items.into_iter().skip(offset).take(limit).collect(),
                offset,
                limit,
                scanned_at: catalog.scanned_at,
            },
        }))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "collections read could not finish",
        )
    })?
}

fn validate_tags(tags: &[String]) -> Result<(), ApiError> {
    if tags.len() > MAX_TAGS
        || tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.len() > MAX_TAG_BYTES || tag.contains('\0'))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_parameters",
            "tags must contain at most 100 nonempty values of at most 256 bytes without NUL",
        ));
    }
    let unique: BTreeSet<_> = tags.iter().map(|tag| tag.as_str()).collect();
    if unique.len() != tags.len() {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_parameters",
            "tags must not contain duplicates",
        ));
    }
    Ok(())
}

fn validate_collection_name(name: &str) -> Result<(), ApiError> {
    if name.trim().is_empty() || name.len() > 256 {
        return Err(invalid_query("name must contain 1 to 256 bytes"));
    }
    Ok(())
}

fn invalid_reference(message: impl Into<String>) -> ApiError {
    error(StatusCode::BAD_REQUEST, "invalid_reference", message)
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
