//! The album half of `fetch`: what MusicBrainz says about the records.
//!
//! **This is where the layer earns its name.** An artist's country, formation
//! date and aliases have no counterpart in a file — there is no tag for where a
//! musician is from — so a fetched artist can only ever be *added* beside the
//! catalog, never compared with it. A release is different: Picard writes
//! `RELEASETYPE`, `DATE` and `LABEL`, so an album is the one entity where "what
//! your files say" and "what MusicBrainz says" are two answers to the same
//! question, and the disagreement between them is the thing `sources` and
//! `doctor` exist to report.
//!
//! # One request per album, whatever the tags carry
//!
//! Three routes in, and they cost the same:
//!
//! - the tags carry the **edition** identifier — a release lookup with
//!   [`musicbrainz::RELEASE_INCLUDES`] returns the label *and* the album folded
//!   into the same answer;
//! - they carry only the **release group** — a group lookup: type and date, no
//!   label, because a group has none;
//! - they carry neither — a search on artist and title, scored, and refused
//!   below the floor rather than guessed.
//!
//! The first two are lookups and produce a certainty. The third is the only
//! place a confidence score appears, and the only place an answer can be
//! declined.

// Compiled in every build, for the reason `fetch` is: the tests below prove
// this walk in every build, and only reaching the network needs the feature.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{self, Facts, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, musicbrainz, text};

use crate::ui;

use super::fetch::{Ask, Refusal, ask_with_backoff};

/// An album to ask about, and what the tags already know about it.
pub struct Target {
    entity: EntityRef,
    /// Album title as the tags spell it, which is what a search is judged on.
    title: String,
    /// The album artist's name, absent on a compilation.
    artist: Option<String>,
    /// `MUSICBRAINZ_ALBUMID`: this precise edition.
    edition: Option<String>,
    /// `MUSICBRAINZ_RELEASEGROUPID`: the album every edition shares.
    group: Option<String>,
}

impl Target {
    /// Where to ask, which is decided entirely by what the tags carry.
    fn url(&self) -> String {
        match (&self.edition, &self.group) {
            (Some(edition), _) => format!(
                "{}/release/{edition}?fmt=json&inc={}",
                musicbrainz::WEB_SERVICE,
                musicbrainz::RELEASE_INCLUDES
            ),
            (None, Some(group)) => {
                format!(
                    "{}/release-group/{group}?fmt=json",
                    musicbrainz::WEB_SERVICE
                )
            }
            // A search, narrowed by the album artist when there is one. A
            // compilation has none, and searching its title alone is exactly
            // the case the floor and the ambiguity refusal exist for.
            (None, None) => {
                let query = match &self.artist {
                    Some(artist) => format!(
                        "releasegroup:\"{}\" AND artist:\"{}\"",
                        musicbrainz::escape_query(&self.title),
                        musicbrainz::escape_query(artist)
                    ),
                    None => format!(
                        "releasegroup:\"{}\"",
                        musicbrainz::escape_query(&self.title)
                    ),
                };
                format!(
                    "{}/release-group/?query={}&fmt=json&limit=5",
                    musicbrainz::WEB_SERVICE,
                    super::fetch::encode(&query)
                )
            }
        }
    }

    /// What the answer means, which depends on how it was asked for.
    fn read(
        &self,
        answer: &aede_core::json::Json,
    ) -> Result<
        (
            musicbrainz::Candidate<sources::ReleaseFacts>,
            sources::Confidence,
        ),
        musicbrainz::NoMatch,
    > {
        match (&self.edition, &self.group) {
            (Some(_), _) => musicbrainz::release(answer)
                .map(|c| (c, sources::Confidence::Identified))
                .ok_or(musicbrainz::NoMatch::Nothing),
            (None, Some(_)) => musicbrainz::release_group(answer)
                .map(|c| (c, sources::Confidence::Identified))
                .ok_or(musicbrainz::NoMatch::Nothing),
            (None, None) => {
                musicbrainz::best_match(&musicbrainz::release_groups(answer), &self.title)
            }
        }
    }
}

/// The albums worth asking about.
///
/// `wanted` narrows by album artist *or* title, so `aede fetch manson` reaches
/// the records as well as the man — one word, one meaning, whichever half of
/// the library it lands in.
pub fn targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    again: bool,
) -> Vec<Target> {
    let mut targets = Vec::new();
    for release in &catalog.releases {
        if release.title.trim().is_empty() {
            continue;
        }
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone());
        if !wanted.is_empty() {
            let title = text::normalize(&release.title);
            let by = artist.as_deref().map(text::normalize).unwrap_or_default();
            if !wanted
                .iter()
                .any(|w| title.contains(w.as_str()) || by.contains(w.as_str()))
            {
                continue;
            }
        }
        let Some(entity) = EntityRef::of(catalog, EntityKind::Release, release.id) else {
            continue;
        };
        if !again && held.get(&entity, sources::MUSICBRAINZ).is_some() {
            continue;
        }
        targets.push(Target {
            entity,
            title: release.title.clone(),
            artist,
            edition: release.mbid.clone(),
            group: release.release_group_mbid.clone(),
        });
    }
    targets
}

/// Asks about each album and files the answer, saving as it goes.
///
/// Returns `(stored, refused, failed)` so the caller can print one report for
/// both halves of the run rather than two that have to be added up by eye.
pub fn run(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    targets: &[Target],
    held: &mut sources::Sources,
    path: &std::path::Path,
    done_already: usize,
    total: usize,
) -> Result<(usize, usize, usize), Box<dyn std::error::Error>> {
    let (mut stored, mut refused, mut failed) = (0usize, 0usize, 0usize);
    for (done, target) in targets.iter().enumerate() {
        print!("\r  asking: {}/{}", done_already + done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let url = target.url();
        let answer = match ask_with_backoff(transport, &url, backoff) {
            Ok(answer) => answer,
            Err(Refusal::RateLimited) => {
                println!();
                sources::save(held, path)?;
                return Err(format!(
                    "the service refused {} times in a row, waiting longer each \
                     time; that is a rate limit rather than a hiccup, so nothing \
                     more was asked.\n  it stopped on \"{}\", asking:\n  {url}",
                    backoff.len() + 1,
                    target.title
                )
                .into());
            }
            Err(other) => {
                failed += 1;
                eprintln!("\r  {} {}: {other}", ui::red("×"), target.title);
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
                    facts: Facts::Release(candidate.facts),
                });
                stored += 1;
                sources::save(held, path)?;
            }
            Err(why) => {
                refused += 1;
                eprintln!(
                    "\r  {} {}: {}",
                    ui::yellow("?"),
                    target.title,
                    super::fetch::refusal(&why)
                );
            }
        }
    }
    Ok((stored, refused, failed))
}

/// The titles a `--dry-run` lists.
pub fn names(targets: &[Target]) -> impl Iterator<Item = &str> {
    targets.iter().map(|t| t.title.as_str())
}

#[cfg(test)]
#[path = "releases_tests.rs"]
mod tests;
