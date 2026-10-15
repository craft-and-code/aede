//! Current-owner policy status and bounded external-media playback leases.
use super::*;
use aede_core::accounts;
use aede_core::parental::{LeaseGrant, Status};

pub(crate) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/me/v1/parental", get(status))
        .route("/api/me/v1/parental/lease", post(lease))
}
async fn status(State(state): State<ApiState>, request: Request) -> Result<Json<Status>, ApiError> {
    personal::no_query(&request)?;
    let owner = auth::request_principal(&request)?;
    personal::empty_body(&personal::body(request).await?)?;
    let for_work = state.clone();
    blocking(&state, move || {
        auth::recheck(&for_work, &owner)?;
        let answer = media_status(&for_work, &owner.owner, &Media::Music { track: None })?;
        auth::recheck(&for_work, &owner)?;
        Ok(Json(answer))
    })
    .await
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Reserve {
        media: Media,
        seconds: u32,
    },
    Renew {
        lease_id: String,
        consumed_ms: u64,
        seconds: u32,
    },
    Release {
        lease_id: String,
        consumed_ms: u64,
    },
}
pub(crate) fn canonical_feed(feed: &str) -> Result<String, ApiError> {
    aede_core::parental::canonical_feed(feed.trim())
        .map_err(|_| invalid_query("invalid HTTP or HTTPS podcast feed"))
}

fn media(input: Media) -> Result<Media, ApiError> {
    match input {
        Media::Radio => Ok(Media::Radio),
        Media::Podcast { feed } => Ok(Media::Podcast {
            feed: canonical_feed(&feed)?,
        }),
        Media::Music { .. } => Err(invalid_query(
            "music leases belong to the server PCM transport",
        )),
    }
}
fn identity(id: &str) -> Result<(), ApiError> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid_query("invalid playback lease"));
    }
    Ok(())
}
#[derive(Serialize)]
struct LeaseResponse {
    #[serde(flatten)]
    grant: LeaseGrant,
    server_now_ms: u64,
}
impl LeaseResponse {
    fn new(grant: LeaseGrant) -> Self {
        let milliseconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self {
            grant,
            server_now_ms: u64::try_from(milliseconds).unwrap_or(u64::MAX),
        }
    }
}
async fn lease(State(state): State<ApiState>, request: Request) -> Result<Response, ApiError> {
    personal::no_query(&request)?;
    let captured = auth::request_principal(&request)?;
    auth::require_mutation(&captured)?;
    let action: Action = personal::json_body(request).await?;
    if matches!(&action, Action::Reserve { seconds, .. } | Action::Renew { seconds, .. } if !(1..=10).contains(seconds))
    {
        return Err(invalid_query(
            "playback grants must last one through ten seconds",
        ));
    }
    let for_work = state.clone();
    blocking(&state, move || {
        let state = for_work;
        let _lock = lock(&state)?;
        auth::recheck(&state, &captured)?;
        let mut data = load(&state)?;
        let now = aede_core::clock::now_seconds();
        let grant: Option<LeaseGrant> = match action {
            Action::Reserve {
                media: input,
                seconds,
            } => {
                let media = media(input)?;
                let id = accounts::random_token().map_err(|_| unavailable())?;
                Some(
                    data.reserve(&captured.owner, &id, media, seconds, now)
                        .map_err(|reason| refused(&reason))?,
                )
            }
            Action::Renew {
                lease_id,
                consumed_ms,
                seconds,
            } => {
                identity(&lease_id)?;
                Some(
                    data.renew(&captured.owner, &lease_id, consumed_ms, seconds, now)
                        .map_err(|reason| refused(&reason))?,
                )
            }
            Action::Release {
                lease_id,
                consumed_ms,
            } => {
                identity(&lease_id)?;
                data.settle(&captured.owner, &lease_id, consumed_ms, now, true)
                    .map_err(|_| unavailable())?;
                None
            }
        };
        auth::recheck(&state, &captured)?;
        save(&state, &data)?;
        Ok(match grant {
            Some(grant) => Json(LeaseResponse::new(grant)).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })
    .await
}
