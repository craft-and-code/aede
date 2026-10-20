//! Shared catalogue identity review, using the same durable decisions as the CLI.
use super::*;
use aede_core::sources::{
    self, Confidence, Facts, ReviewDecision, ReviewItem, SourceRecord, Sources,
};
use std::hash::{Hash, Hasher};

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/admin/v1/source-review", get(list).patch(decide))
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Parameters {
    offset: Option<String>,
    limit: Option<String>,
    q: String,
    all: bool,
}
#[derive(Serialize)]
struct Item {
    id: String,
    revision: String,
    reference: String,
    name: String,
    kind: &'static str,
    source_id: Option<String>,
    url: Option<String>,
    score: Option<u8>,
    decision: Option<&'static str>,
    local_mbid: Option<String>,
    details: Vec<(&'static str, String)>,
}
fn decision_name(value: ReviewDecision) -> &'static str {
    match value {
        ReviewDecision::Accepted => "accepted",
        ReviewDecision::Rejected => "rejected",
    }
}
fn record<'a>(held: &'a Sources, item: &ReviewItem) -> Result<&'a SourceRecord, ApiError> {
    held.get(&item.entity, &item.source).ok_or_else(|| {
        error(
            StatusCode::CONFLICT,
            "review_changed",
            "source claim changed; refresh the review list",
        )
    })
}
fn view(catalog: &Catalog, held: &Sources, item: &ReviewItem) -> Result<Item, ApiError> {
    let record = record(held, item)?;
    let name = item.entity.display_name(catalog);
    let local_mbid = record.local_musicbrainz_id(catalog).map(str::to_owned);
    // An optimistic content stamp, not an authentication or identity primitive.
    let mut stamp = std::collections::hash_map::DefaultHasher::new();
    format!("{record:?}|{item:?}|{local_mbid:?}|{name}").hash(&mut stamp);
    let namespace = match &record.facts {
        Facts::Artist(_) => "artist",
        Facts::Label(_) => "label",
        Facts::Release(_) => match record.musicbrainz_release_identity() {
            Some(sources::MusicBrainzReleaseIdentity::Edition(_)) => "release",
            _ => "release-group",
        },
        _ => "recording",
    };
    let url = item
        .source_id
        .as_deref()
        .filter(|id| aede_core::musicbrainz::valid_id(id))
        .map(|id| format!("https://musicbrainz.org/{namespace}/{id}"));
    let mut details = Vec::new();
    let mut add = |label, value: &Option<String>| {
        if let Some(value) = value {
            details.push((label, value.chars().take(512).collect()));
        }
    };
    match &record.facts {
        Facts::Artist(facts) => {
            add("type", &facts.kind);
            add("area", &facts.area);
            add("country", &facts.country_code);
            add("began", &facts.began);
            add("ended", &facts.ended);
            add("disambiguation", &facts.disambiguation);
        }
        Facts::Release(facts) => {
            add("type", &facts.primary_type);
            add("date", &facts.first_released);
        }
        _ => {}
    }
    Ok(Item {
        id: item.id.clone(),
        revision: format!("{:016x}", stamp.finish()),
        reference: item.entity.to_token(),
        name,
        kind: item.entity.kind.as_str(),
        source_id: item.source_id.clone(),
        url,
        local_mbid,
        details,
        score: match record.confidence {
            Confidence::Matched(score) => Some(score),
            _ => None,
        },
        decision: item.decision.map(decision_name),
    })
}
fn check(state: &ApiState, principal: Option<&auth::Principal>) -> Result<(), ApiError> {
    if let Some(principal) = principal {
        auth::recheck(state, principal)?;
    }
    Ok(())
}
fn load(path: &Path) -> Result<Sources, ApiError> {
    sources::load_all(path)
        .map(|value| value.unwrap_or_default())
        .map_err(|failure| {
            eprintln!("Source review read failed: {failure}");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "sources_unavailable",
                "stored source claims could not be read",
            )
        })
}
async fn list(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&state, &request)?;
    let principal = request.extensions().get::<auth::Principal>().cloned();
    let Query(params) = input.map_err(|failure| invalid_query(failure.body_text()))?;
    if params.q.len() > 1024 {
        return Err(invalid_query("q is too long"));
    }
    let (offset, limit) = page_bounds(params.offset.as_deref(), params.limit.as_deref())?;
    inspection::inspect(state, move |state| {
        check(state, principal.as_ref())?;
        let catalog = state.catalog.blocking_read();
        let catalog = catalog.as_ref().ok_or_else(unavailable)?;
        let held = load(&sources::sources_path(&state.data_dir))?;
        let wanted = text::normalize(&params.q);
        let rows: Vec<_> = held
            .review_items(catalog)
            .into_iter()
            .filter(|item| {
                item.source == sources::MUSICBRAINZ
                    && item.entity.resolve(catalog).is_some()
                    && (params.all || item.decision.is_none())
                    && (wanted.is_empty()
                        || text::normalize(&item.entity.display_name(catalog)).contains(&wanted))
            })
            .collect();
        let total = rows.len();
        let items = rows
            .iter()
            .skip(offset)
            .take(limit)
            .map(|item| view(catalog, &held, item))
            .collect::<Result<Vec<_>, _>>()?;
        check(state, principal.as_ref())?;
        Ok(serde_json::json!({"items":items,"total":total,"offset":offset,"limit":limit}))
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    id: String,
    revision: String,
    decision: Action,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Accept,
    Reject,
    Undo,
}
async fn decide(State(state): State<ApiState>, request: Request) -> Result<Json<Item>, ApiError> {
    require_admin(&state, &request)?;
    personal::no_query(&request)?;
    let principal = request.extensions().get::<auth::Principal>().cloned();
    let input: Decision = personal::json_body(request).await?;
    if [&input.id, &input.revision]
        .iter()
        .any(|value| value.len() != 16 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(invalid_query("review requires full id and revision"));
    }
    inspection::inspect(state, move |state| {
        let _lock = StoreLock::try_acquire(&state.data_dir).map_err(|failure| {
            error(
                if failure.kind() == std::io::ErrorKind::WouldBlock {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                "store_busy",
                "Aède data is busy; retry later",
            )
        })?;
        check(state, principal.as_ref())?;
        let catalog = store::load(&store::catalog_path(&state.data_dir))
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        let path = sources::sources_path(&state.data_dir);
        let mut held = load(&path)?;
        let item = held
            .review_items(&catalog)
            .into_iter()
            .find(|item| {
                item.id == input.id
                    && item.source == sources::MUSICBRAINZ
                    && item.entity.resolve(&catalog).is_some()
            })
            .ok_or_else(|| {
                error(
                    StatusCode::CONFLICT,
                    "review_changed",
                    "source claim changed; refresh the review list",
                )
            })?;
        if view(&catalog, &held, &item)?.revision != input.revision {
            return Err(error(
                StatusCode::CONFLICT,
                "review_changed",
                "source claim changed; refresh the review list",
            ));
        }
        let result = match input.decision {
            Action::Accept => held.decide(
                &catalog,
                &input.id,
                ReviewDecision::Accepted,
                aede_core::clock::now_seconds(),
            ),
            Action::Reject => held.decide(
                &catalog,
                &input.id,
                ReviewDecision::Rejected,
                aede_core::clock::now_seconds(),
            ),
            Action::Undo => held.clear_review(&catalog, &input.id),
        }
        .map_err(|message| error(StatusCode::CONFLICT, "review_changed", message))?;
        check(state, principal.as_ref())?;
        sources::save(&held, &path).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "sources_unavailable",
                "source decision could not be saved",
            )
        })?;
        state.parental.invalidate();
        view(&catalog, &held, &result)
    })
    .await
}
#[cfg(test)]
#[path = "source_review_tests.rs"]
mod tests;
