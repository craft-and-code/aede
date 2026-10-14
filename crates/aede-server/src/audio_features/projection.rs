use super::*;
use crate::personal::moods::{Evidence, RuleView, domain};
use aede_moods::{AcousticModel, TagScope};
struct Selected<'a> {
    reference: &'a aede_core::user::EntityRef,
    reason: serde_json::Value,
    priority: u8,
    distance: f32,
}
fn descriptor<'a>(
    facts: &'a Facts,
    catalog: &Catalog,
    row: &Evidence,
) -> Option<&'a aede_moods::Features> {
    let id = row.reference.resolve(catalog)?;
    let track = catalog.track(id)?;
    current(facts, catalog, track.file_id)
}
fn personal<'a>(
    facts: &Facts,
    catalog: &Catalog,
    rows: &'a [Evidence],
    rule: &RuleView,
) -> (Vec<Selected<'a>>, Option<AcousticModel>) {
    let rule = domain(rule);
    let mut seen = std::collections::BTreeSet::new();
    let seeds = rows
        .iter()
        .filter(|row| row.scope <= TagScope::Album && rule.matches(&row.tags, &[]).is_some())
        .filter(|row| {
            row.reference
                .resolve(catalog)
                .and_then(|id| catalog.track(id))
                .is_some_and(|track| seen.insert(track.file_id))
        })
        .filter_map(|row| descriptor(facts, catalog, row))
        .filter(|row| row.usable())
        .take(16)
        .collect::<Vec<_>>();
    let model = AcousticModel::new(&seeds);
    let mut selected = Vec::new();
    for row in rows {
        let tag = if row.scope == TagScope::Genre {
            None
        } else {
            rule.matches(&row.tags, &[])
        };
        if let Some(reason) = tag {
            let scope = match row.scope {
                TagScope::Track => "track",
                TagScope::Album => "album",
                TagScope::Artist => "artist",
                TagScope::Genre => "genre",
            };
            selected.push(Selected {
                reference: &row.reference,
                reason: serde_json::json!({"tag":reason.tag,"genre":null,"scope":scope}),
                priority: row.scope as u8,
                distance: 0.,
            });
            continue;
        }
        if row.scope != TagScope::Genre {
            continue;
        }
        let distance = model.as_ref().and_then(|model| {
            descriptor(facts, catalog, row).and_then(|value| model.distance(value))
        });
        if let (Some(model), Some(distance)) = (&model, distance) {
            if distance <= model.radius() {
                selected.push(Selected{reference:&row.reference,reason:serde_json::json!({"tag":null,"genre":null,"scope":"acoustic","audio":{"kind":"neighbor","distance":distance,"examples":model.examples()}}),priority:3,distance});
            }
        } else if let Some(reason) = rule.matches(&[], &row.genres) {
            selected.push(Selected {
                reference: &row.reference,
                reason: serde_json::json!({"tag":null,"genre":reason.genre,"scope":"genre"}),
                priority: 4,
                distance: 0.,
            });
        }
    }
    selected.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.distance.total_cmp(&b.distance))
    });
    (selected, model)
}
fn expand(
    catalog: &Catalog,
    selected: Vec<Selected<'_>>,
    offset: usize,
    limit: usize,
) -> Result<Vec<serde_json::Value>, ApiError> {
    selected
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|entry| {
            let mut row =
                crate::presentation::playback_rows(catalog, &entry.reference.to_token())?.remove(0);
            row["reason"] = entry.reason;
            Ok(row)
        })
        .collect()
}
pub(crate) struct Window<'a> {
    pub(crate) chosen: Option<&'a str>,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) expected: Option<&'a str>,
}
pub(crate) fn projection(
    state: &ApiState,
    catalog: &Catalog,
    rows: &[Evidence],
    rules: &[RuleView],
    window: Window<'_>,
) -> Result<serde_json::Value, ApiError> {
    let Window {
        chosen,
        offset,
        limit,
        expected,
    } = window;
    let facts = facts(state)?;
    let revision = selection_revision(catalog, rows, rules, facts.revision);
    if expected.is_some_and(|token| token != revision) {
        return Err(error(
            StatusCode::CONFLICT,
            "selection_changed",
            "acoustic selection changed; restart paging",
        ));
    }
    let mut summaries = Vec::new();
    let mut items = Vec::new();
    let mut total = 0;
    for rule in rules {
        let (selected, model) = personal(&facts, catalog, rows, rule);
        summaries.push(serde_json::json!({"rule":rule,"count":selected.len(),"model":model.as_ref().map(|m|serde_json::json!({"examples":m.examples(),"radius":m.radius()}))}));
        if chosen == Some(rule.id.as_str()) {
            total = selected.len();
            items = expand(catalog, selected, offset, limit)?;
        }
    }
    for recipe in aede_moods::audio_recipes() {
        if rules.iter().any(|rule| rule.id == recipe.id) {
            continue;
        }
        let selected=rows.iter().filter_map(|row|{let feature=descriptor(&facts,catalog,row)?;let (value,unit)=recipe.measure(feature)?;Some(Selected{reference:&row.reference,reason:serde_json::json!({"tag":null,"genre":null,"scope":"acoustic","audio":{"kind":"descriptor","value":value,"unit":unit}}),priority:0,distance:0.})}).collect::<Vec<_>>();
        summaries.push(serde_json::json!({"rule":{"id":recipe.id,"name":recipe.name,"tags":[],"genres":[],"acoustic":recipe.kind},"count":selected.len()}));
        if chosen == Some(recipe.id) {
            total = selected.len();
            items = expand(catalog, selected, offset, limit)?;
        }
    }
    Ok(
        serde_json::json!({"moods":summaries,"items":items,"total":total,"offset":offset,"scanned_at":catalog.scanned_at,"acoustic":coverage(state,catalog,&facts),"selection_revision":revision}),
    )
}

fn selection_revision(
    catalog: &Catalog,
    rows: &[Evidence],
    rules: &[RuleView],
    acoustic: u64,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    signature(catalog).hash(&mut hash);
    acoustic.hash(&mut hash);
    for row in rows {
        (&row.reference.key, &row.tags, &row.genres, row.scope as u8).hash(&mut hash);
    }
    for row in rules {
        (&row.id, &row.tags, &row.genres).hash(&mut hash);
    }
    format!("{:016x}", hash.finish())
}
#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
