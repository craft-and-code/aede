//! `fetch --identify`: ask AcoustID what the fingerprinted files are.
//!
//! The network half of identifying by sound. `aede fingerprint` decodes and
//! computes; this asks and stores. The split is the one `aede extract` and
//! `fetch --covers` already have, for a reason sharper here: decoding is
//! minutes of work over a library and asking is one small request per file, so
//! a network pass that failed halfway would otherwise have to decode
//! everything again to be retried.
//!
//! # It identifies. It does not correct.
//!
//! What comes back is filed **beside** the tags, in the attributed layer, and
//! shown next to them by `aede sources`. Nothing here writes a tag, because
//! nothing in Aède writes into an audio file: a file whose audio and tags
//! disagree is reported, and what to do about it is the reader's decision,
//! made in their own tagger.
//!
//! That is not a limitation to apologise for. A fingerprint match is a strong
//! guess and is wrong in ways that are easy to picture — two masterings of one
//! recording fingerprint alike — so a program that rewrote tags on the
//! strength of one would be trading a library nobody has checked for a library
//! nobody *can* check.
//!
//! # A recording, not a release
//!
//! AcoustID answers what is playing. The same performance sits on the album,
//! the compilation and the reissue, so the answer names the track and says
//! nothing about which pressing this file came from — which the tags answer
//! better than any fingerprint could.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{self, Confidence, Facts, SourceRecord, TrackFacts};
use aede_core::user::EntityRef;
use aede_core::{acoustid, clock};

use crate::ui;

use super::Res;
use super::fetch::{Ask, ask_with_backoff};

/// A file to ask about, and what to ask with.
struct Target {
    entity: EntityRef,
    path: String,
    fingerprint: aede_core::fingerprint::Fingerprint,
}

/// What the pass would do, and why it would leave the rest alone.
struct Survey {
    targets: Vec<Target>,
    /// Skipped: no fingerprint has been computed for this file.
    no_fingerprint: usize,
    /// Skipped: the service was already asked about it.
    asked: usize,
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
    let survey = survey(catalog, held, asked.names, asked.again);
    println!("{}", ui::section("Identify"));
    skipped(&survey);
    if survey.targets.is_empty() {
        println!("  {}", ui::dim("nothing to ask about"));
        return Ok(());
    }

    // Read before anything is asked, so a missing key costs no requests and
    // no waiting — and the message explains rather than orders.
    let key = std::env::var(acoustid::KEY_VARIABLE)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(acoustid::no_key)?;

    let total_ms = survey.targets.len() as u64 * acoustid::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {}, one request each, about {}",
        ui::plural(survey.targets.len(), "file"),
        ui::long_duration(total_ms)
    );
    println!(
        "  {}",
        ui::dim(
            "what comes back is stored beside your tags and never written into \
             a file: aede sources shows the two side by side"
        )
    );
    if asked.dry_run {
        for target in &survey.targets {
            println!("  {}", ui::dim(&target.path));
        }
        println!("  {}", ui::dim("nothing was asked: --dry-run"));
        return Ok(());
    }

    let (mut named, mut unknown, mut failed) = (0usize, 0usize, 0usize);
    for (done, target) in survey.targets.iter().enumerate() {
        print!("\r  asking: {}/{}", done + 1, survey.targets.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let url = acoustid::lookup_url(&key, &target.fingerprint.data, target.fingerprint.seconds);
        let answer = match ask_with_backoff(transport, &url, backoff) {
            Ok(answer) => answer,
            Err(why) => {
                failed += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.path);
                continue;
            }
        };

        // A bad key answers `200 OK` with `"status": "error"`. Left unread,
        // every file in the library would be filed as "asked, and it does not
        // know", and the reader would conclude their music is unidentifiable.
        // So it stops the run: one wrong key is wrong for every file, and
        // carrying on would spend an hour proving it.
        if let Some(refusal) = acoustid::refused(&answer) {
            println!();
            return Err(format!("AcoustID refused: {refusal}").into());
        }

        match acoustid::best(&answer) {
            Some(heard) => {
                named += 1;
                store(held, target, Some(heard));
            }
            // Asked, and it does not know — an answer, and the one that stops
            // a second run asking again about a file nobody has submitted.
            None => {
                unknown += 1;
                store(held, target, None);
            }
        }
        sources::save(held, path)?;
    }
    println!();

    println!(
        "{} {named} identified, {unknown} the service does not know, {failed} failed",
        ui::green("→")
    );
    if named > 0 {
        println!(
            "  {}",
            ui::dim("aede sources shows what it heard beside what your tags say")
        );
    }
    Ok(())
}

/// Files what the service answered, including when the answer was nothing.
///
/// `Matched`, never `Identified`, and that is not a formality: the file was
/// recognised by how it sounds, not named by an identifier its tags carried.
/// The confidence carries the score so that everything downstream can see how
/// good a guess it was.
fn store(held: &mut sources::Sources, target: &Target, heard: Option<acoustid::Heard>) {
    // The service states a fraction; a percentage is what a reader compares
    // and what stores without a float in it.
    let per_cent = heard
        .as_ref()
        .map(|h| (h.score.clamp(0.0, 1.0) * 100.0).round() as u8);
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: acoustid::SOURCE.to_string(),
        source_id: heard.as_ref().map(|h| h.recording.clone()),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::matched(per_cent.unwrap_or(0)),
        facts: Facts::Track(TrackFacts {
            recording: heard.as_ref().map(|h| h.recording.clone()),
            score: per_cent,
            title: heard.as_ref().and_then(|h| h.title.clone()),
            artists: heard
                .as_ref()
                .map(|h| h.artists.clone())
                .unwrap_or_default(),
            album: heard.as_ref().and_then(|h| h.album.clone()),
        }),
    });
}

/// Sorts every file into one of the three.
fn survey(catalog: &Catalog, held: &sources::Sources, wanted: &[String], again: bool) -> Survey {
    let mut out = Survey {
        targets: Vec::new(),
        no_fingerprint: 0,
        asked: 0,
    };
    for track in &catalog.tracks {
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        if !super::fetch::reaches(wanted, &[&track.title, &file.path]) {
            continue;
        }
        let Some(fingerprint) = file.fingerprint.clone() else {
            out.no_fingerprint += 1;
            continue;
        };
        let Some(entity) = EntityRef::of(catalog, EntityKind::Track, track.id) else {
            continue;
        };
        if !again && held.get(&entity, acoustid::SOURCE).is_some() {
            out.asked += 1;
            continue;
        }
        out.targets.push(Target {
            entity,
            path: file.path.clone(),
            fingerprint,
        });
    }
    out
}

/// Says what was left alone and why, whether or not there is work to do.
///
/// The line that matters is the first: a reader who runs this before
/// fingerprinting anything is told exactly which command comes first, on the
/// line where this one gives up.
fn skipped(survey: &Survey) {
    for (count, one, many) in [
        (
            survey.no_fingerprint,
            "has no fingerprint yet: aede fingerprint computes one",
            "have no fingerprint yet: aede fingerprint computes them",
        ),
        (
            survey.asked,
            "was asked about already: --full asks again",
            "were asked about already: --full asks again",
        ),
    ] {
        if count > 0 {
            let rest = match count {
                1 => one,
                _ => many,
            };
            println!(
                "  {}",
                ui::dim(&format!("{} {rest}", ui::plural(count, "file")))
            );
        }
    }
}

/// How many files an `--identify` pass would ask about, if it ran now.
pub fn waiting(catalog: &Catalog, held: &sources::Sources) -> usize {
    survey(catalog, held, &[], false).targets.len()
}

#[cfg(test)]
#[path = "identify_tests.rs"]
mod tests;
