//! Owner-private discoveries derived from retained personal evidence; locked definitions never leave the engine.
use crate::{
    model::Catalog,
    user::{ClientPreference, UserData},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
const KEY: &str = "aede.achievements.v1";
struct Definition {
    id: String,
    fr: String,
    en: String,
    detail_fr: String,
    detail_en: String,
    met: bool,
    icon: &'static str,
}
fn definitions(catalog: &Catalog, data: &UserData, owner: &str, search: bool) -> Vec<Definition> {
    let annotations: Vec<_> = data
        .annotations
        .iter()
        .filter(|r| r.owner == owner)
        .collect();
    let plays: Vec<_> = data.plays.iter().filter(|r| r.owner == owner).collect();
    let completed = crate::listening::completed_tracks(data, owner);
    let albums = crate::listening::completed_albums(catalog, data, owner);
    let mut discographies: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for r in &catalog.releases {
        if let Some(artist) = r.album_artist_id {
            discographies.entry(artist).or_default().push(r.id);
        }
    }
    let tags: BTreeSet<_> = annotations.iter().flat_map(|r| r.tags.iter()).collect();
    let families = [
        (
            "loved",
            "Favoris",
            "Favourites",
            "Ajouter des favoris",
            "Add favourites",
            annotations.iter().filter(|r| r.loved).count() as u64,
            "heart",
            [1, 5, 10, 25, 50, 100],
        ),
        (
            "rated",
            "Étoiles",
            "Stars",
            "Noter des éléments",
            "Rate items",
            annotations.iter().filter(|r| r.rating.is_some()).count() as u64,
            "star",
            [1, 5, 10, 25, 50, 100],
        ),
        (
            "notes",
            "Carnet",
            "Notebook",
            "Écrire des notes personnelles",
            "Write personal notes",
            annotations
                .iter()
                .filter(|r| r.note.as_ref().is_some_and(|n| !n.trim().is_empty()))
                .count() as u64,
            "edit",
            [1, 3, 10, 25, 50, 100],
        ),
        (
            "tags",
            "Repères",
            "Landmarks",
            "Utiliser des tags distincts",
            "Use distinct tags",
            tags.len() as u64,
            "tags",
            [1, 3, 10, 25, 50, 100],
        ),
        (
            "tracks",
            "À l'écoute",
            "Listening",
            "Écouter des pistes distinctes jusqu'au bout",
            "Finish distinct tracks",
            completed.len() as u64,
            "volume",
            [1, 10, 25, 100, 250, 1000],
        ),
        (
            "albums",
            "Du début à la fin",
            "From beginning to end",
            "Terminer des albums du catalogue",
            "Complete catalogue albums",
            albums.len() as u64,
            "disc",
            [1, 5, 10, 25, 50, 100],
        ),
        (
            "artists",
            "En profondeur",
            "In depth",
            "Terminer les discographies présentes dans le catalogue",
            "Complete discographies present in the catalogue",
            discographies
                .values()
                .filter(|rs| rs.iter().all(|r| albums.contains(r)))
                .count() as u64,
            "users",
            [1, 2, 5, 10, 25, 50],
        ),
        (
            "hours",
            "Le temps de la musique",
            "Time for music",
            "Cumuler des heures d'écoute dans l'historique conservé",
            "Accumulate listening hours in retained history",
            plays
                .iter()
                .fold(0_u64, |sum, r| sum.saturating_add(r.ms_played))
                / 3600000,
            "history",
            [1, 5, 10, 25, 50, 100],
        ),
    ];
    let mut result = Vec::new();
    for (key, fr, en, df, de, count, icon, limits) in families {
        for threshold in limits {
            result.push(Definition {
                id: format!("{key}-{threshold}"),
                fr: format!("{fr} · {threshold}"),
                en: format!("{en} · {threshold}"),
                detail_fr: format!("{df} : {threshold}."),
                detail_en: format!("{de}: {threshold}."),
                met: count >= threshold,
                icon,
            });
        }
    }
    let count = data.collections.iter().filter(|r| r.owner == owner).count();
    for threshold in [1, 3, 10] {
        result.push(Definition {
            id: format!("collections-{threshold}"),
            fr: format!("Votre sélection · {threshold}"),
            en: format!("Your selection · {threshold}"),
            detail_fr: format!("Créer {threshold} collection(s)."),
            detail_en: format!("Create {threshold} collection(s)."),
            met: count >= threshold,
            icon: "library",
        });
    }
    result.push(Definition {
        id: "search-1".into(),
        fr: "Au-delà du titre".into(),
        en: "Beyond the title".into(),
        detail_fr: "Effectuer une recherche avancée dans Phémios.".into(),
        detail_en: "Perform an advanced search in Phémios.".into(),
        met: search,
        icon: "search",
    });
    result
}
/// Retain monotonic unlock dates, refusing malformed records rather than erasing private state.
/// Search is a client-declared successful action; completion uses retained Play evidence only.
pub fn refresh(
    catalog: &Catalog,
    data: &mut UserData,
    owner: &str,
    advanced: bool,
    at: u64,
) -> Result<Value, String> {
    let row = data
        .client_preferences
        .iter()
        .position(|r| r.owner == owner && r.key == KEY);
    let previous = match row {
        Some(i) => serde_json::from_str::<Value>(&data.client_preferences[i].value)
            .map_err(|_| "achievement store is unreadable")?,
        None => json!({"version":1,"unlocked":{},"search":false}),
    };
    if previous["version"] != 1 || !previous["search"].is_boolean() {
        return Err("achievement store is unreadable".into());
    }
    let object = previous["unlocked"]
        .as_object()
        .filter(|r| r.len() <= 256)
        .ok_or("achievement store is unreadable")?;
    let mut unlocked = BTreeMap::new();
    for (key, value) in object {
        if key.len() > 64 || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err("achievement store is unreadable".into());
        }
        unlocked.insert(
            key.clone(),
            value.as_u64().ok_or("achievement store is unreadable")?,
        );
    }
    let search = advanced || previous["search"].as_bool().unwrap_or(false);
    let definitions = definitions(catalog, data, owner, search);
    for d in &definitions {
        if d.met {
            unlocked.entry(d.id.clone()).or_insert(at);
        }
    }
    let value = json!({"version":1,"unlocked":unlocked,"search":search}).to_string();
    if !crate::user::valid_preference(KEY, &value) {
        return Err("achievement store exceeds its bound".into());
    }
    match row {
        Some(i) => data.client_preferences[i].value = value,
        None => {
            if data.client_preferences.len() >= 8192
                || data
                    .client_preferences
                    .iter()
                    .filter(|r| r.owner == owner)
                    .count()
                    >= 64
            {
                return Err("personal store is full".into());
            }
            data.client_preferences.push(ClientPreference {
                owner: owner.into(),
                key: KEY.into(),
                value,
            });
        }
    }
    let items:Vec<_>=definitions.iter().enumerate().map(|(index,d)| {
  if let Some(at)=unlocked.get(&d.id) {json!({"slot":index,"unlocked_at":at,"title":{"fr":d.fr,"en":d.en},"description":{"fr":d.detail_fr,"en":d.detail_en},"icon":d.icon})}
  else {json!({"slot":index,"unlocked_at":null})}
 }).collect();
    Ok(
        json!({"items":items,"unlocked":definitions.iter().filter(|d|unlocked.contains_key(&d.id)).count(),"total":definitions.len()}),
    )
}
#[cfg(test)]
#[path = "achievements_tests.rs"]
mod tests;
