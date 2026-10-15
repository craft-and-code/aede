//! Owner-private missing-album decisions reuse the CLI's derived core report.
use super::*;
use aede_core::missing;
use aede_core::sources::Sources;
use serde_json::{Value, json};
#[path = "personal_missing_cover.rs"]
mod cover;
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/me/v1/missing", get(list).put(update))
        .merge(cover::routes())
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissingQuery {
    offset: Option<String>,
    limit: Option<String>,
    include_set_aside: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    release_group: String,
    set_aside: bool,
}
fn rows(
    catalog: &Catalog,
    sources: &Sources,
    data: &UserData,
    owner: &str,
    include_aside: bool,
) -> Result<Vec<Value>, ApiError> {
    let aside: Vec<_> = data
        .set_aside
        .iter()
        .filter(|row| row.owner == owner)
        .cloned()
        .collect();
    missing::absent(catalog, sources, &aside, true).into_iter()
        .filter(|row| row.known.is_studio_album() && (include_aside || !row.set_aside))
        .map(|row| {
            let artist = EntityRef::of(catalog, EntityKind::Artist, row.artist_id).ok_or_else(unavailable)?;
            let year = row.known.first_released.as_deref().and_then(|date| date.get(..4)).and_then(|year| year.parse::<u32>().ok());
            Ok(json!({"artist":artist.to_token(),"artist_name":row.artist,"release_group":row.known.mbid,"title":row.known.title,"year":year,"set_aside":row.set_aside,"source":"musicbrainz","url":format!("https://musicbrainz.org/release-group/{}",row.known.mbid)}))
        }).collect()
}
async fn list(
    State(state): State<ApiState>,
    query: Result<Query<MissingQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<Page<Value>>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let Query(query) = query.map_err(|failure| invalid_query(failure.body_text()))?;
    let (offset, limit) = page_bounds(query.offset.as_deref(), query.limit.as_deref())?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_lock, catalog, data) = locked_personal(&state)?;
        owner.recheck(&state)?;
        let sources = crate::inspection::held_sources(&data_dir)?;
        let all = rows(
            &catalog,
            &sources,
            &data,
            &owner.id,
            query.include_set_aside.unwrap_or(false),
        )?;
        let total = all.len();
        owner.admit(
            &state,
            Json(Page::new(
                all.into_iter().skip(offset).take(limit).collect(),
                total,
                offset,
                limit,
                catalog.scanned_at,
            )),
        )
    })?
    .await
    .map_err(|_| unavailable())?
}
async fn update(State(state): State<ApiState>, request: Request) -> Result<Json<Value>, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let input: Decision = json_body(request).await?;
    if input.release_group.len() != 36
        || !input.release_group.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err(invalid_query("release_group must be a MusicBrainz UUID"));
    }
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        locked_update(&data_dir, &state, &owner, |catalog, data| {
            let sources = crate::inspection::held_sources(&data_dir)?;
            let found = rows(catalog, &sources, data, &owner.id, true)?
                .into_iter()
                .find(|row| row["release_group"] == input.release_group);
            if input.set_aside {
                let row = found.ok_or_else(|| {
                    error(
                        StatusCode::NOT_FOUND,
                        "missing_not_found",
                        "no missing studio album matches this identifier",
                    )
                })?;
                if !data
                    .set_aside
                    .iter()
                    .any(|row| row.owner == owner.id && row.release_group == input.release_group)
                {
                    if data
                        .set_aside
                        .iter()
                        .filter(|row| row.owner == owner.id)
                        .count()
                        >= 4096
                    {
                        return Err(invalid_query("too many set-aside decisions"));
                    }
                    data.set_aside.push(user::SetAside {
                        owner: owner.id.clone(),
                        release_group: input.release_group.clone(),
                        title: row["title"].as_str().ok_or_else(unavailable)?.into(),
                        created_at: clock::now_seconds(),
                    });
                }
            } else {
                data.set_aside.retain(|row| {
                    row.owner != owner.id || row.release_group != input.release_group
                });
            }
            Ok(Json(
                json!({"release_group":input.release_group,"set_aside":input.set_aside}),
            ))
        })
    })?
    .await
    .map_err(|_| unavailable())?
}
#[cfg(test)]
#[path = "personal_missing_tests.rs"]
mod tests;
