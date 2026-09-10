//! `fetch --labels`: a record label's own MusicBrainz identifier.
//!
//! # The gap this closes
//!
//! Fetching an album's release already reads a label's identifier off the
//! answer, when the very `label-info` entry that names it also carries one —
//! see [`aede_core::sources::ReleaseFacts::label_mbid`]. That is passive: it
//! only ever reaches a label through a release this catalog happened to look
//! up, and by the first entry that named one on a release crediting several.
//! A label credited only on releases this build never reached — a compilation
//! nobody has fetched, an edition MusicBrainz itself lists with no address for
//! its own label — is passed over, silently, for ever.
//!
//! This pass asks about the label directly, closing that gap the same way
//! `fetch` itself closes it for artists and albums: by asking MusicBrainz.
//!
//! # A lookup where one is free, a search otherwise
//!
//! No tag carries a label's MusicBrainz identifier — nothing like
//! `MUSICBRAINZ_ARTISTID` exists for a label — so there is never one to read
//! out of the files the way an artist or an edition lookup can be. But by the
//! time this pass runs, the passive capture above may already have found one
//! *for this very label*, on some other release. [`known_mbid`] looks for
//! that first: when it finds one, this asks a **lookup**, the same certainty
//! an edition lookup gives an album. Only when nothing has named this label's
//! identifier yet does it fall back to a **search** by name, scored and
//! refused below the floor like every other guess in this layer.
//!
//! # What is stored
//!
//! The identifier, in [`aede_core::sources::SourceRecord::source_id`] —
//! [`aede_core::sources::LabelFacts`] carries nothing else, and says why.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{self, Confidence, Facts, LabelFacts, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, musicbrainz, text};

use crate::ui;

use super::Res;
use super::fetch::{Ask, ask_with_backoff, queue, worth_deferring};

/// A label to ask about, and how to ask.
struct Target {
    entity: EntityRef,
    name: String,
    /// A MusicBrainz identifier already known for this label, from a release
    /// this catalog looked up — see [`known_mbid`]. `Some` asks a lookup, a
    /// certainty; `None` asks a search, a guess.
    known_mbid: Option<String>,
}

impl Target {
    /// Where to ask, which is decided entirely by whether an identifier is
    /// already known.
    fn url(&self) -> String {
        match &self.known_mbid {
            Some(mbid) => format!("{}/label/{mbid}?fmt=json", musicbrainz::WEB_SERVICE),
            None => format!(
                "{}/label/?query={}&fmt=json&limit=5",
                musicbrainz::WEB_SERVICE,
                super::fetch::encode(&musicbrainz::escape_query(&self.name))
            ),
        }
    }

    /// What the answer means, which depends on how it was asked for.
    fn read(
        &self,
        answer: &Json,
    ) -> Result<(musicbrainz::Candidate<LabelFacts>, Confidence), musicbrainz::NoMatch> {
        match &self.known_mbid {
            Some(_) => musicbrainz::label(answer)
                .map(|c| (c, Confidence::Identified))
                .ok_or(musicbrainz::NoMatch::Nothing),
            None => musicbrainz::best_match(&musicbrainz::labels(answer), &self.name),
        }
    }
}

/// The pass.
pub fn run(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    asked: &super::fetch::Asked,
) -> Res {
    let targets = targets(catalog, held, asked.names, asked.scope, asked.again);
    println!("{}", ui::section("Labels"));
    if targets.is_empty() {
        let narrowed = super::fetch::narrowing(asked.names, asked.scope);
        match narrowed.is_empty() {
            true => println!("  {}", ui::dim("nothing to ask about")),
            false => println!(
                "  {}",
                ui::dim(&format!("nothing to ask about for {narrowed}"))
            ),
        }
        return Ok(());
    }

    let total_ms = targets.len() as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {}, one request each, about {}",
        ui::plural(targets.len(), "label"),
        ui::long_duration(total_ms)
    );
    let known = targets.iter().filter(|t| t.known_mbid.is_some()).count();
    if known > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} already named on a release this catalog looked up — asked by identifier, not by name",
                ui::plural(known, "label")
            ))
        );
    }

    if super::fetch::asked_nothing(
        asked,
        &targets.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
    ) {
        return Ok(());
    }

    let (mut stored, mut refused, mut failed) = (0usize, 0usize, 0usize);
    let mut pending = queue(&targets);
    let mut done = 0usize;
    let total = targets.len();
    while let Some((target, retried)) = pending.pop_front() {
        print!("\r  asking: {}/{}", done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let answer = match ask_with_backoff(transport, &target.url(), backoff) {
            Ok(answer) => answer,
            Err(other) if worth_deferring(&other) && !retried => {
                pending.push_back((target, true));
                continue;
            }
            Err(other) => {
                failed += 1;
                done += 1;
                eprintln!("\r  {} {}: {other}", ui::red("×"), target.name);
                continue;
            }
        };

        match target.read(&answer) {
            Ok((candidate, confidence)) => {
                held.set(SourceRecord {
                    key: target.entity.key.clone(),
                    source: sources::MUSICBRAINZ.to_string(),
                    source_id: Some(candidate.mbid),
                    fetched_at: clock::now_seconds(),
                    confidence,
                    facts: Facts::Label(candidate.facts),
                });
                stored += 1;
                sources::save(held, path)?;
            }
            Err(why) => {
                refused += 1;
                eprintln!(
                    "\r  {} {}: {}",
                    ui::yellow("?"),
                    target.name,
                    super::fetch::refusal(&why)
                );
            }
        }
        done += 1;
    }
    println!();

    println!(
        "{} {stored} stored, {refused} left alone, {failed} failed",
        ui::green("→")
    );
    Ok(())
}

/// How many labels this pass would ask about, if it ran now.
pub fn waiting(catalog: &Catalog, held: &sources::Sources) -> usize {
    targets(catalog, held, &[], &super::fetch::EVERYTHING, false).len()
}

/// Labels worth asking about: named, in scope, and without a MusicBrainz
/// identifier of their own already on record.
fn targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    scope: &super::fetch::Scope,
    again: bool,
) -> Vec<Target> {
    let mut targets = Vec::new();
    for label in &catalog.labels {
        if label.name.trim().is_empty() {
            continue;
        }
        if !super::fetch::reaches(wanted, &[label.name.as_str()]) {
            continue;
        }
        if !scope.has_label(&label.key) {
            continue;
        }
        let Some(entity) = EntityRef::of(catalog, EntityKind::Label, label.id) else {
            continue;
        };
        if !again && held.get(&entity, sources::MUSICBRAINZ).is_some() {
            continue;
        }
        targets.push(Target {
            entity,
            name: label.name.clone(),
            known_mbid: known_mbid(held, &label.key),
        });
    }
    targets
}

/// A label's own identifier, when a release lookup already named it.
///
/// Every release this catalog has looked up that named *this* label — by its
/// normalized name, [`text::normalize`], the same comparison the whole
/// program uses to decide two spellings are one thing — is searched for the
/// first one that also carried an identifier. Passive capture (see the module
/// doc) may have found it on any of them; which release does not matter, only
/// that one did.
fn known_mbid(held: &sources::Sources, key: &str) -> Option<String> {
    held.records.iter().find_map(|record| {
        if record.source != sources::MUSICBRAINZ {
            return None;
        }
        let Facts::Release(facts) = &record.facts else {
            return None;
        };
        let name = facts.label.as_deref()?;
        let mbid = facts.label_mbid.as_deref()?;
        (text::normalize(name) == key).then(|| mbid.to_string())
    })
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
