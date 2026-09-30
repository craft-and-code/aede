//! Online fallback for a label without a Wikipedia biography.
//!
//! MusicBrainz supplies the Discogs identifier as a URL relationship. Discogs
//! is never searched by name. Failed refreshes keep the last stored profile.

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{self, Facts, LabelFacts, SourceRecord, Sources};
use aede_core::user::EntityRef;
use aede_core::{clock, discogs, musicbrainz, text};

use super::Res;

#[cfg(feature = "fetch")]
pub fn refresh_online(
    catalog: &Catalog,
    label: Id,
    held: &mut Sources,
    path: &std::path::Path,
) -> Res {
    use aede_core::http::Client;

    let agent = Client::identify(
        "Aede",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_REPOSITORY"),
    );
    let mut mb = Client::new(agent.clone(), musicbrainz::REQUEST_INTERVAL);
    let mut dc = Client::new(agent, discogs::REQUEST_INTERVAL);
    refresh(
        catalog,
        label,
        held,
        path,
        &mut |url| {
            crate::ui::with_loading("Checking MusicBrainz...", || {
                mb.get_json(url).map_err(|error| error.to_string())
            })
        },
        &mut |url| {
            crate::ui::with_loading("Loading Discogs data...", || {
                dc.get_json(url).map_err(|error| error.to_string())
            })
        },
    )
}

#[cfg(not(feature = "fetch"))]
pub fn refresh_online(
    _catalog: &Catalog,
    _label: Id,
    _held: &mut Sources,
    _path: &std::path::Path,
) -> Res {
    Err("this build has no network support; --online requires the fetch feature".into())
}

#[cfg_attr(not(feature = "fetch"), allow(dead_code))]
fn refresh(
    catalog: &Catalog,
    label_id: Id,
    held: &mut Sources,
    path: &std::path::Path,
    ask_musicbrainz: &mut dyn FnMut(&str) -> Result<Json, String>,
    ask_discogs: &mut dyn FnMut(&str) -> Result<Json, String>,
) -> Res {
    let label = catalog
        .label(label_id)
        .ok_or("label is no longer in the catalog")?;
    let entity = EntityRef::of(catalog, EntityKind::Label, label_id)
        .ok_or("label has no stable source key")?;

    if matches!(held.get(&entity, "wikipedia").map(|row| &row.facts),
        Some(Facts::Label(facts)) if facts.summary.is_some())
    {
        println!("  Wikipedia already has a biography for this label; Discogs was not asked.");
        return Ok(());
    }
    let previous = held.get(&entity, sources::MUSICBRAINZ).cloned();
    let confidence = previous
        .as_ref()
        .map(|record| record.confidence)
        .unwrap_or(sources::Confidence::Identified);
    let mbid = previous
        .as_ref()
        .and_then(|row| row.source_id.clone())
        .or_else(|| label.mbid.clone())
        .ok_or("this label has no MusicBrainz ID; run aede fetch --labels first")?;
    let mut link = previous.as_ref().and_then(|row| match &row.facts {
        Facts::Label(facts) => facts.discogs.clone(),
        _ => None,
    });
    if link.is_none() {
        let url = format!(
            "{}/label/{mbid}?fmt=json&inc={}",
            musicbrainz::WEB_SERVICE,
            musicbrainz::LABEL_INCLUDES
        );
        let answer = ask_musicbrainz(&url).map_err(|why| format!("MusicBrainz: {why}"))?;
        let candidate = musicbrainz::label(&answer)
            .filter(|candidate| {
                candidate.mbid == mbid
                    && text::normalize(&candidate.name) == text::normalize(&label.name)
            })
            .ok_or("MusicBrainz did not confirm this label's identity")?;
        link = candidate.facts.discogs;
        if let Some(url) = &link {
            let mut record = previous.unwrap_or(SourceRecord {
                key: entity.key.clone(),
                source: sources::MUSICBRAINZ.to_string(),
                source_id: Some(mbid),
                fetched_at: clock::now_seconds(),
                confidence: sources::Confidence::Identified,
                facts: Facts::Label(LabelFacts::default()),
            });
            if let Facts::Label(facts) = &mut record.facts {
                facts.discogs = Some(url.clone());
            }
            held.set(record);
            sources::save(held, path)?;
        }
    }
    let url = link.ok_or("MusicBrainz has no Discogs link for this label")?;
    let id = discogs::label_id(&url).ok_or("MusicBrainz gave an unusable Discogs label link")?;
    let answer = ask_discogs(&discogs::api_url(id)).map_err(|why| format!("Discogs: {why}"))?;
    let mut prose = discogs::profile(&answer, id, &label.name)
        .ok_or("Discogs returned no profile matching this label")?;
    let markup = prose.text.clone();
    let cached_text = held.get(&entity, discogs::SOURCE).and_then(|record| {
        let Facts::Label(facts) = &record.facts else {
            return None;
        };
        (facts.summary_markup.as_deref() == Some(markup.as_str())
            && facts.summary.as_ref().is_some_and(|summary| {
                discogs::referenced_labels(&summary.text).is_empty()
                    && discogs::referenced_artists(&summary.text).is_empty()
            }))
        .then(|| facts.summary.as_ref().map(|summary| summary.text.clone()))
        .flatten()
    });
    prose.text = if let Some(text) = cached_text {
        text
    } else {
        let label_ids = discogs::referenced_labels(&markup);
        let artist_ids = discogs::referenced_artists(&markup);
        let names_to_resolve = label_ids.len() + artist_ids.len();
        if names_to_resolve > 0 {
            println!("  Resolving {names_to_resolve} Discogs names...");
        }
        let mut labels = std::collections::BTreeMap::new();
        for linked_id in label_ids {
            let answer = ask_discogs(&discogs::api_url(linked_id))
                .map_err(|why| format!("Discogs linked label {linked_id}: {why}"))?;
            let name = discogs::label_name(&answer, linked_id)
                .ok_or_else(|| format!("Discogs did not identify linked label {linked_id}"))?;
            labels.insert(linked_id, name);
        }
        let mut artists = std::collections::BTreeMap::new();
        for artist_id in artist_ids {
            let answer = ask_discogs(&discogs::artist_api_url(artist_id))
                .map_err(|why| format!("Discogs linked artist {artist_id}: {why}"))?;
            let name = discogs::artist_name(&answer, artist_id)
                .ok_or_else(|| format!("Discogs did not identify linked artist {artist_id}"))?;
            artists.insert(artist_id, name);
        }
        discogs::render_profile(&markup, &labels, &artists)
    };
    held.set(SourceRecord {
        key: entity.key,
        source: discogs::SOURCE.to_string(),
        source_id: Some(id.to_string()),
        fetched_at: clock::now_seconds(),
        confidence,
        facts: Facts::Label(LabelFacts {
            summary: Some(prose),
            summary_markup: Some(markup),
            ..Default::default()
        }),
    });
    sources::save(held, path)?;
    Ok(())
}

#[cfg(test)]
#[path = "discogs_tests.rs"]
mod tests;
