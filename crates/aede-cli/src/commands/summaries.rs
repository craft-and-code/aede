//! `fetch --summaries`: the second pass, which asks Wikipedia for prose.
//!
//! It is a second pass rather than part of the first because it depends on the
//! first. MusicBrainz is what supplies the `wikidata` link, and that link is
//! the only reliable way from an artist in a library to an article about them:
//! searching Wikipedia by name would put "Manson" in front of a reader and
//! call it a description.
//!
//! **Two requests per artist, on top of one.** The Wikidata entity holds a
//! page *title* per language; the title then has to be turned into a summary.
//! For six hundred artists that is twenty minutes added to ten, which is why
//! it is asked for rather than assumed — and why this pass, like the first,
//! saves after every answer.
//!
//! What is decided here is only the walk. Which article, in which language,
//! and what may be stored from it are [`aede_core::wikipedia`]'s, and are
//! proven there without a network.

// Same reason as `fetch`: the walk is tested in every build, only the reaching
// of a network needs the feature.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::sources::{self, Facts, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, wikipedia};

use crate::ui;

use super::Res;
use super::fetch::{Ask, ask_with_backoff};

/// An artist to ask about: where to file the answer, what to call them, and
/// the Wikidata id MusicBrainz gave.
struct Target {
    entity: EntityRef,
    name: String,
    id: String,
}

/// The pass, with everything it cannot decide handed in.
///
/// `held` is taken by value and given back so that the caller keeps one copy
/// of the layer: two passes writing two copies of `sources.json` is how the
/// second silently discards the first.
pub fn run(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    langs: &[String],
    held: &mut sources::Sources,
    path: &std::path::Path,
    again: bool,
) -> Res {
    let targets = targets(held, again);
    println!("{}", ui::section("Summaries"));
    if targets.is_empty() {
        println!(
            "  {}",
            ui::dim(
                "nothing to ask about: run fetch first, and note that MusicBrainz \
                 has no wikidata link for every artist"
            )
        );
        return Ok(());
    }

    // Two requests each, at the same polite rate, so the estimate is as
    // honest as the one `fetch` prints — and it has to be, because this is
    // the pass a reader did not have to run.
    let total_ms = targets.len() as u64 * 2 * wikipedia::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {} with a wikidata link, two requests each, about {}",
        ui::plural(targets.len(), "artist"),
        ui::long_duration(total_ms)
    );
    println!(
        "  {}",
        ui::dim(&format!(
            "articles are looked for in {}, in that order",
            langs.join(", ")
        ))
    );

    let langs: Vec<&str> = langs.iter().map(String::as_str).collect();
    let (mut stored, mut empty, mut failed) = (0usize, 0usize, 0usize);
    for (done, target) in targets.iter().enumerate() {
        print!("\r  asking: {}/{}", done + 1, targets.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let entity_doc =
            match ask_with_backoff(transport, &wikipedia::entity_data_url(&target.id), backoff) {
                Ok(doc) => doc,
                Err(why) => {
                    failed += 1;
                    eprintln!("\r  {} {}: {why}", ui::red("×"), target.name);
                    continue;
                }
            };
        let Some(article) = wikipedia::article(&entity_doc, &target.id, &langs) else {
            // An entity with no article in any language asked for is a real
            // answer, and recording it is what stops the next run asking again.
            empty += 1;
            store(held, target, None);
            sources::save(held, path)?;
            continue;
        };

        let summary_doc =
            match ask_with_backoff(transport, &wikipedia::summary_url(&article), backoff) {
                Ok(doc) => doc,
                Err(why) => {
                    failed += 1;
                    eprintln!("\r  {} {}: {why}", ui::red("×"), target.name);
                    continue;
                }
            };
        match wikipedia::prose(&summary_doc, &article) {
            Some(prose) => {
                stored += 1;
                store(held, target, Some(prose));
            }
            None => {
                empty += 1;
                store(held, target, None);
            }
        }
        sources::save(held, path)?;
    }
    println!();

    println!(
        "{} {stored} stored, {empty} with nothing to say, {failed} failed",
        ui::green("→")
    );
    println!(
        "  {}",
        ui::dim(&format!(
            "wikipedia text is {LICENCE}: it is stored with the page it came from, \
             and shown with it",
            LICENCE = wikipedia::LICENCE
        ))
    );
    Ok(())
}

/// Files what Wikipedia said, including when that was nothing.
///
/// A record with an empty summary is not a wasted row: "asked, and there is no
/// article" is exactly what the layer exists to keep apart from "never asked",
/// and without it every run would ask about the same artists forever.
fn store(held: &mut sources::Sources, target: &Target, prose: Option<aede_core::sources::Prose>) {
    let facts = aede_core::sources::ArtistFacts {
        summary: prose,
        ..Default::default()
    };
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: wikipedia::SOURCE.to_string(),
        // The Wikidata id, which is what makes a second run an update: the
        // article title can change, the entity does not.
        source_id: Some(target.id.clone()),
        fetched_at: clock::now_seconds(),
        // Reached by identifier, through a link MusicBrainz asserts. Nothing
        // was matched by name anywhere in this pass, which is the whole reason
        // it goes through Wikidata.
        confidence: aede_core::sources::Confidence::Identified,
        facts: Facts::Artist(facts),
    });
}

/// How many artists this pass would ask about, if it ran now.
///
/// Deliberately the same function as the walk, counted rather than re-derived.
/// `fetch` prints this to offer the second pass at the moment it becomes
/// possible, and an offer that counted differently from the run it offers is
/// worse than no offer at all.
pub fn waiting(held: &sources::Sources) -> usize {
    targets(held, false).len()
}

/// Who to ask about: artists MusicBrainz gave a Wikidata link for.
///
/// Reads the layer rather than the catalog, because the link is the input and
/// the catalog does not hold it. An artist whose record has already been
/// fetched is skipped unless `again`, for the reason `fetch` skips them: a
/// second run should cost what changed.
fn targets(held: &sources::Sources, again: bool) -> Vec<Target> {
    let mut targets = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        let Facts::Artist(artist) = &record.facts else {
            continue;
        };
        let Some(id) = artist.wikidata.as_deref().and_then(wikipedia::entity_id) else {
            continue;
        };
        let entity = record.entity();
        if !again && held.get(&entity, wikipedia::SOURCE).is_some() {
            continue;
        }
        targets.push(Target {
            // The key is the artist's normalised name, which is also the only
            // name this pass has: it never reads the catalog.
            name: entity.key.clone(),
            entity,
            id,
        });
    }
    targets
}

/// The languages to look for an article in, best first.
///
/// Taken from the environment rather than from an option: a French reader
/// wants the French article, and asking them to say so on every run is a way
/// of making sure they get the English one. `fr_FR.UTF-8` and `fr` both mean
/// French; anything unrecognisable is ignored rather than sent out as a
/// hostname that does not exist.
///
/// English is always last, because for a great many artists it is the only
/// article there is — and it is only *last*, so it never displaces a reader's
/// own language when both exist.
pub fn preferred_langs(locale: Option<&str>) -> Vec<String> {
    let mut langs: Vec<String> = Vec::new();
    if let Some(code) = locale
        .map(|l| l.split(['_', '.', '@']).next().unwrap_or("").to_lowercase())
        .filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_lowercase()))
    {
        langs.push(code);
    }
    for fallback in wikipedia::FALLBACK_LANGS {
        if !langs.iter().any(|l| l == fallback) {
            langs.push(fallback.to_string());
        }
    }
    langs
}

#[cfg(test)]
#[path = "summaries_tests.rs"]
mod tests;
