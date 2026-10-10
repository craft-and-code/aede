//! Online fallback for a label without a Wikipedia biography.
//!
//! MusicBrainz supplies the Discogs identifier as a URL relationship. Discogs
//! is never searched by name. Failed refreshes keep the last stored profile.

#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{self, Facts, LabelFacts, SourceRecord, Sources};
use aede_core::user::EntityRef;
use aede_core::{clock, discogs, musicbrainz, text};

use super::Res;

/// Explicit batch fallback for identified labels in the requested name/folder scope.
/// Fresh profiles are reused unless `--full` asks again; dry-run performs no I/O.
pub(super) fn run(
    catalog: &Catalog,
    transport: &mut dyn super::fetch::Ask,
    backoff: &[std::time::Duration],
    held: &mut Sources,
    path: &std::path::Path,
    asked: &super::fetch::Asked,
) -> Res {
    let targets: Vec<_> = catalog.labels.iter().filter(|label| {
        let Some(entity)=EntityRef::of(catalog,EntityKind::Label,label.id) else { return false; };
        super::fetch::reaches(asked.names,&[label.name.as_str()])
            && asked.scope.has_label(&label.key)
            && (label.mbid.is_some() || held.get(&entity,sources::MUSICBRAINZ).is_some_and(|r| r.source_id.is_some() && held.is_trusted(catalog,r)))
            && !matches!(held.get(&entity,"wikipedia").map(|r| &r.facts),Some(Facts::Label(f)) if f.summary.is_some())
            && (asked.again || !held.get(&entity,discogs::SOURCE).is_some_and(|r| discogs::fresh(r.fetched_at,clock::now_seconds()) && matches!(&r.facts,Facts::Label(f) if f.summary.is_some())))
    }).collect();
    println!("{}", crate::ui::section("Discogs label biographies"));
    println!("  {} to refresh", crate::ui::plural(targets.len(), "label"));
    if super::fetch::asked_nothing(
        asked,
        &targets.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
    ) {
        return Ok(());
    }
    let transport = std::cell::RefCell::new(transport);
    let mut unavailable = 0usize;
    let rate_limited = std::cell::Cell::new(false);
    for (index, label) in targets.iter().enumerate() {
        println!(
            "  {}/{}: {}",
            index + 1,
            targets.len(),
            crate::ui::literal(&label.name)
        );
        let result = refresh(
            catalog,
            label.id,
            held,
            path,
            &mut |url| {
                super::fetch::ask_with_backoff(*transport.borrow_mut(), url, backoff).map_err(|e| {
                    if matches!(
                        &e,
                        super::fetch::Refusal::RateLimited | super::fetch::Refusal::TooManyRequests
                    ) {
                        rate_limited.set(true);
                    }
                    e.to_string()
                })
            },
            &mut |url| {
                super::fetch::ask_with_backoff(*transport.borrow_mut(), url, backoff).map_err(|e| {
                    if matches!(
                        &e,
                        super::fetch::Refusal::RateLimited | super::fetch::Refusal::TooManyRequests
                    ) {
                        rate_limited.set(true);
                    }
                    e.to_string()
                })
            },
        );
        if let Err(why) = result {
            // A provider throttle exhausts the pass budget; do not turn it into one request per label.
            if rate_limited.get() {
                return Err(format!(
                    "Discogs stopped by provider rate limit: {why}; saved profiles are retained"
                )
                .into());
            }
            unavailable += 1;
            eprintln!(
                "  Discogs label {}: {why}; continuing with the remaining labels.",
                crate::ui::literal(&label.name)
            );
        }
    }
    if unavailable > 0 {
        return Err(format!(
            "Discogs completed with {unavailable} unavailable label(s); saved profiles are retained"
        )
        .into());
    }
    Ok(())
}

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
