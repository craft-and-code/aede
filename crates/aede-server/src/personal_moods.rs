//! Owner-private explainable atmosphere projections; ordinary reads stay offline.
use super::*;
use aede_moods::{Rule, TagScope, TagSelector};
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    mood: Option<String>,
    #[serde(default)]
    audio: bool,
    revision: Option<String>,
    offset: Option<String>,
    limit: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleView {
    pub(crate) id: String,
    name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) genres: Vec<String>,
}
impl From<Rule> for RuleView {
    fn from(rule: Rule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            tags: rule.tags,
            genres: rule.genres,
        }
    }
}
pub(crate) fn domain(row: &RuleView) -> Rule {
    Rule {
        id: row.id.clone(),
        name: row.name.clone(),
        tags: row.tags.clone(),
        genres: row.genres.clone(),
    }
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/moods", get(read).put(write))
}
fn rules(data: &UserData, owner: &str) -> Result<Vec<RuleView>, ApiError> {
    resolved_rules(data, owner).map(|(rules, _)| rules)
}
fn resolved_rules(data: &UserData, owner: &str) -> Result<(Vec<RuleView>, Vec<String>), ApiError> {
    let mut result: Vec<RuleView> = aede_moods::presets()
        .into_iter()
        .map(RuleView::from)
        .collect();
    let mut reserved = Vec::new();
    if let Some(entry) = data
        .client_preferences
        .iter()
        .find(|row| row.owner == owner && row.key == "phemios.moods")
    {
        let custom: Vec<RuleView> = serde_json::from_str(&entry.value)
            .map_err(|_| invalid_query("stored mood rules are invalid"))?;
        if custom.len() > 32 {
            return Err(invalid_query("stored mood rules are invalid"));
        }
        for row in custom {
            // Legacy collisions remain in preferences until an explicit replacement.
            if aede_moods::is_audio_recipe_id(&row.id) {
                if !reserved.contains(&row.id) {
                    reserved.push(row.id);
                }
                continue;
            }
            if !domain(&row).valid() {
                return Err(invalid_query("stored mood rules are invalid"));
            }
            if let Some(existing) = result.iter_mut().find(|known| known.id == row.id) {
                *existing = row;
            } else {
                result.push(row);
            }
        }
    }
    Ok((result, reserved))
}
pub(crate) struct Evidence {
    // Preserve the validated graph link instead of repeatedly resolving a path for every recipe.
    pub(crate) file_id: Id,
    pub(crate) reference: EntityRef,
    pub(crate) tags: Vec<String>,
    pub(crate) genres: Vec<String>,
    pub(crate) scope: TagScope,
}
// Build shared indexes once, and expand presentation rows only for the visible page.
pub(crate) fn evidence(
    catalog: &Catalog,
    data: &UserData,
    owner: &str,
) -> Result<Vec<Evidence>, ApiError> {
    let rules: Vec<Rule> = rules(data, owner)?.iter().map(domain).collect();
    let selector = TagSelector::new(&rules);
    let mut tags = BTreeMap::<String, Vec<String>>::new();
    for annotation in data.annotations.iter().filter(|row| row.owner == owner) {
        tags.insert(
            annotation.target.to_token(),
            annotation
                .tags
                .iter()
                .map(|tag| text::normalize(tag))
                .collect(),
        );
    }
    let mut genres = BTreeMap::<String, Vec<String>>::new();
    for link in &catalog.genre_links {
        if let Some(genre) = catalog.genre(link.genre_id)
            && let Some(reference) = EntityRef::of(catalog, link.entity_kind, link.entity_id)
        {
            genres
                .entry(reference.to_token())
                .or_default()
                .push(text::normalize(&genre.name));
        }
    }
    let releases =
        catalog.playback_release_order(catalog.releases.iter().map(|row| row.id).collect());
    let ids: Vec<Id> = releases
        .into_iter()
        .filter_map(|id| catalog.release(id))
        .flat_map(|release| release.track_ids.iter().copied())
        .chain(
            catalog
                .tracks
                .iter()
                .filter(|track| track.release_id.is_none())
                .map(|track| track.id),
        )
        .collect();
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        let track = catalog.track(id).ok_or_else(unavailable)?;
        let reference = EntityRef::of(catalog, EntityKind::Track, id).ok_or_else(unavailable)?;
        let release = track.release_id.and_then(|id| catalog.release(id));
        let artist = release.and_then(|release| release.album_artist_id);
        let refs = std::iter::once(reference.clone())
            .chain(release.and_then(|row| EntityRef::of(catalog, EntityKind::Release, row.id)))
            .chain(artist.and_then(|id| EntityRef::of(catalog, EntityKind::Artist, id)));
        let mut layers: [Vec<String>; 3] = Default::default();
        let mut own_genres = Vec::new();
        for (index, key) in refs.map(|reference| reference.to_token()).enumerate() {
            layers[index].extend(tags.get(&key).into_iter().flatten().cloned());
            own_genres.extend(genres.get(&key).into_iter().flatten().cloned());
        }
        let scope = selector.scope([&layers[0], &layers[1], &layers[2]]);
        let own_tags = match scope {
            TagScope::Track => std::mem::take(&mut layers[0]),
            TagScope::Album => std::mem::take(&mut layers[1]),
            TagScope::Artist => std::mem::take(&mut layers[2]),
            TagScope::Genre => Vec::new(),
        };
        rows.push(Evidence {
            file_id: track.file_id,
            reference,
            tags: own_tags,
            genres: own_genres,
            scope,
        });
    }
    Ok(rows)
}
fn matches(rows: &[Evidence], rule: &Rule) -> Vec<(EntityRef, aede_moods::Match, TagScope)> {
    let mut selected: Vec<_> = rows
        .iter()
        .filter_map(|row| {
            let genres = if row.scope == TagScope::Genre {
                row.genres.as_slice()
            } else {
                &[]
            };
            rule.matches(&row.tags, genres)
                .map(|reason| (row.reference.clone(), reason, row.scope))
        })
        .collect();
    // Stable ties preserve chronological release and track order; rank before pagination.
    selected.sort_by_key(|row| row.2);
    selected
}
fn legacy_projection(
    catalog: &Catalog,
    rows: &[Evidence],
    rules: &[RuleView],
    chosen: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, ApiError> {
    let mut summaries = Vec::new();
    let mut items = Vec::new();
    let mut total = 0;
    for rule in rules {
        let selected = matches(rows, &domain(rule));
        summaries.push(serde_json::json!({"rule":rule,"count":selected.len()}));
        if chosen == Some(rule.id.as_str()) {
            total = selected.len();
            for (reference, reason, scope) in selected.into_iter().skip(offset).take(limit) {
                let mut row =
                    super::super::presentation::playback_rows(catalog, &reference.to_token())?
                        .remove(0);
                let scope = match scope {
                    TagScope::Track => "track",
                    TagScope::Album => "album",
                    TagScope::Artist => "artist",
                    TagScope::Genre => "genre",
                };
                row["reason"] =
                    serde_json::json!({"tag":reason.tag,"genre":reason.genre,"scope":scope});
                items.push(row);
            }
        }
    }
    Ok(
        serde_json::json!({"moods":summaries,"items":items,"total":total,"offset":offset,"scanned_at":catalog.scanned_at}),
    )
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    let query = parsed_query(input)?;
    if query.revision.as_ref().is_some_and(|value| {
        !query.audio
            || value.len() != 16
            || !value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    }) {
        return Err(invalid_query(
            "revision requires an acoustic selection token",
        ));
    }
    if query.audio {
        super::super::audio_features::warm(&state);
    }
    let (offset, limit) = page_bounds(query.offset.as_deref(), query.limit.as_deref())?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_lock, catalog, data) = locked_personal(&state)?;
        owner.recheck(&state)?;
        let (rules, reserved) = resolved_rules(&data, &owner.id)?;
        if query.mood.as_ref().is_some_and(|id| {
            !(rules.iter().any(|row| &row.id == id)
                || query.audio && aede_moods::audio_recipes().iter().any(|row| row.id == id))
        }) {
            return Err(invalid_query("unknown atmosphere"));
        }
        let evidence = evidence(&catalog, &data, &owner.id)?;
        let mut response = if query.audio {
            super::super::audio_features::projection(
                &state,
                &catalog,
                &evidence,
                &rules,
                super::super::audio_features::Window {
                    chosen: query.mood.as_deref(),
                    offset,
                    limit,
                    expected: query.revision.as_deref(),
                },
            )?
        } else {
            legacy_projection(
                &catalog,
                &evidence,
                &rules,
                query.mood.as_deref(),
                offset,
                limit,
            )?
        };
        if !reserved.is_empty() {
            response["reserved_rule_ids"] = serde_json::json!(reserved);
        }
        owner.admit(&state, Json(response))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "atmosphere worker failed",
        )
    })?
}
async fn write(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<RuleView>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    no_query(&request)?;
    let mut row: RuleView = json_body(request).await?;
    row.tags = row
        .tags
        .iter()
        .map(|value| text::normalize(value))
        .collect();
    row.genres = row
        .genres
        .iter()
        .map(|value| text::normalize(value))
        .collect();
    if !domain(&row).valid() {
        return Err(invalid_query("invalid atmosphere criteria"));
    }
    spawn_personal(state.inspection_slots.clone(), move || {
        locked_update(&state.data_dir, &state, &owner, |_, data| {
            let mut custom: Vec<RuleView> = data
                .client_preferences
                .iter()
                .find(|entry| entry.owner == owner.id && entry.key == "phemios.moods")
                .map(|entry| serde_json::from_str(&entry.value))
                .transpose()
                .map_err(|_| invalid_query("invalid stored atmospheres"))?
                .unwrap_or_default();
            if let Some(known) = custom.iter_mut().find(|known| known.id == row.id) {
                *known = row.clone();
            } else {
                custom.push(row.clone());
            }
            let value =
                serde_json::to_string(&custom).map_err(|_| invalid_query("invalid atmospheres"))?;
            if custom.len() > 32 || !user::valid_preference("phemios.moods", &value) {
                return Err(invalid_query("too many atmospheres"));
            }
            if let Some(entry) = data
                .client_preferences
                .iter_mut()
                .find(|entry| entry.owner == owner.id && entry.key == "phemios.moods")
            {
                entry.value = value;
            } else {
                if data.client_preferences.len() >= 8192
                    || data
                        .client_preferences
                        .iter()
                        .filter(|entry| entry.owner == owner.id)
                        .count()
                        >= 64
                {
                    return Err(invalid_query("too many preferences"));
                }
                data.client_preferences.push(user::ClientPreference {
                    owner: owner.id.clone(),
                    key: "phemios.moods".into(),
                    value,
                });
            }
            Ok(Json(row))
        })
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "atmosphere save failed",
        )
    })?
}
#[cfg(test)]
#[path = "personal_moods_tests.rs"]
mod tests;
