//! `aede fingerprint`: work out what each file's audio *is*.
//!
//! The local half of identifying by sound, and the counterpart of
//! `fetch --identify` the way `aede extract` is the counterpart of
//! `fetch --covers`: **this one computes and touches no network, that one asks
//! and computes nothing.** Two commands, each doing one thing, and the line
//! that gives up in either names the other.
//!
//! Kept apart for a reason beyond symmetry. Fingerprinting decodes the audio,
//! which is minutes of work over a large library; asking AcoustID is one small
//! request per file at three a second. Folded together, a network pass that
//! failed halfway would have to decode everything again to be retried.
//!
//! # Only the files that need it
//!
//! By default, the files whose tags cannot identify them — no title, or no
//! artist. Those are what this feature exists to rescue, and decoding a
//! library that is already correctly tagged is hours of work to confirm what
//! the tags say. A folder or a name given as an argument widens it; `--full`
//! takes everything, including files fingerprinted before.
//!
//! # Where it is kept, and why it is not in `sources.json`
//!
//! On the file entry in the catalog, beside the integrity verdict. Both are
//! Aède's own conclusions about the bytes — worked out, not fetched — and both
//! survive a rescan that leaves the file alone and are dropped by one that
//! finds it changed. `sources.json` is for what somebody *else* said, and
//! nobody said this.

use aede_core::model::{Catalog, Id};
use aede_core::{fingerprint, store};

use super::Res;
use crate::args::Args;
use crate::ui;

/// A file to fingerprint, and why it is in the list.
struct Target {
    id: Id,
    path: String,
    seconds: u32,
}

/// What the command would do, and why it would leave the rest alone.
struct Survey {
    targets: Vec<Target>,
    /// Skipped: a fingerprint is already stored and `--full` was not given.
    done: usize,
    /// Skipped: the tags name this file, so nothing needs rescuing.
    named: usize,
    /// Skipped: the tags already carry the identifier a lookup would return.
    already_identified: usize,
    /// Skipped: the catalog knows no length, and a lookup needs one.
    no_length: usize,
}

pub fn fingerprint(args: &Args) -> Res {
    let mut catalog = super::load(args)?;
    let scope = super::scope_of(args)?;
    let wanted = super::fetch::names_given(args);
    let full = args.has("full");
    let survey = survey(&catalog, &scope, &wanted, full);

    println!("{}", ui::section("Fingerprint"));
    skipped(&survey, full);
    if survey.targets.is_empty() {
        println!("  {}", ui::dim("nothing to fingerprint"));
        return Ok(());
    }

    // Named before the tool is looked for, because "no ffmpeg" is worth
    // knowing even on a run that would have had nothing to do.
    let Some(by) = fingerprint::find() else {
        return Err(fingerprint::missing().into());
    };

    println!(
        "  {} to decode, with {}",
        ui::plural(survey.targets.len(), "file"),
        by.program()
    );
    println!(
        "  {}",
        ui::dim(
            "decoding is the slow part; what comes out is stored in the catalog \
             and never computed twice"
        )
    );
    if args.has("dry-run") {
        for target in &survey.targets {
            println!("  {}", ui::dim(&target.path));
        }
        println!("  {}", ui::dim("nothing was decoded: --dry-run"));
        return Ok(());
    }

    let (mut done, mut failed) = (0usize, 0usize);
    for (nth, target) in survey.targets.iter().enumerate() {
        print!("\r  decoding: {}/{}", nth + 1, survey.targets.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());
        match fingerprint::of(by, std::path::Path::new(&target.path), target.seconds) {
            Ok(print) => {
                done += 1;
                if let Some(file) = catalog.files.get_mut(target.id as usize) {
                    file.fingerprint = Some(print);
                }
            }
            Err(why) => {
                failed += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.path);
            }
        }
    }
    println!();

    // Written once at the end rather than per file: the catalog is one
    // document, and rewriting it a thousand times would cost more than the
    // decoding did.
    if done > 0 {
        store::save(&catalog, &store::catalog_path(&super::data_dir(args)))?;
    }
    println!("{} {done} fingerprinted, {failed} failed", ui::green("→"));
    if done > 0 {
        println!(
            "  {}",
            ui::dim("aede fetch --identify asks AcoustID what they are")
        );
    }
    Ok(())
}

/// Sorts every file into one of the four.
///
/// `full` widens twice over: it takes files that already have a fingerprint,
/// and files the tags already name. Those are two different reasons to skip,
/// and one flag lifting both is what somebody means by "do it all".
fn survey(catalog: &Catalog, scope: &[String], wanted: &[String], full: bool) -> Survey {
    let mut out = Survey {
        targets: Vec::new(),
        done: 0,
        named: 0,
        already_identified: 0,
        no_length: 0,
    };
    for file in &catalog.files {
        if !super::in_scope(&file.path, scope) {
            continue;
        }
        // A name reaches a file by what its tags call it — which for the files
        // this command is about is often nothing, so a name given here mostly
        // means "this folder", and the scope is the better tool.
        let title = first(file, "title");
        let artist = first(file, "artist");
        if !super::fetch::reaches(wanted, &[&title, &artist, &file.path]) {
            continue;
        }
        if file.fingerprint.is_some() && !full {
            out.done += 1;
            continue;
        }
        // The sharpest skip of the four, and the one worth naming: a file
        // tagged by Picard already carries `musicbrainz_recordingid`, which is
        // **exactly what a lookup would answer with**. Decoding it would spend
        // minutes and a request to be told something the file already says.
        //
        // The same lesson the Discogs note ends on: before reaching for a
        // service, check whether the identifier you want is already in hand.
        // `--full` lifts it, because a *wrong* recording identifier is the one
        // thing nothing else in this program can catch.
        if !full && identified(catalog, file) {
            out.already_identified += 1;
            continue;
        }
        // The point of the whole thing: a file whose tags identify it does not
        // need rescuing, and decoding it costs the same as decoding one that
        // does.
        if !full && !title.trim().is_empty() && !artist.trim().is_empty() {
            out.named += 1;
            continue;
        }
        // A lookup is a fingerprint *and* a length, and the length comes from
        // the file's own header at scan time. Without one there is nothing to
        // ask, so there is nothing to decode for.
        let Some(seconds) = length(file) else {
            out.no_length += 1;
            continue;
        };
        out.targets.push(Target {
            id: file.id,
            path: file.path.clone(),
            seconds,
        });
    }
    out
}

/// `true` when the file's tags already carry a MusicBrainz recording id.
///
/// Read from the track rather than from the raw tags, because the tag reader
/// has already done the work of knowing that ID3 calls it `MUSICBRAINZ_TRACKID`
/// and Vorbis calls it something else — a second spelling table here would be
/// the first one's copy, right until it was not.
fn identified(catalog: &Catalog, file: &aede_core::model::AudioFile) -> bool {
    catalog
        .tracks
        .iter()
        .filter(|track| track.file_id == file.id)
        .any(|track| {
            track
                .mbid
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty())
        })
}

/// The file's length in whole seconds, when the catalog knows one.
fn length(file: &aede_core::model::AudioFile) -> Option<u32> {
    let ms = file.properties.duration_ms?;
    let seconds = (ms / 1000) as u32;
    (seconds > 0).then_some(seconds)
}

/// The first value of a tag, or the empty string.
fn first(file: &aede_core::model::AudioFile, key: &str) -> String {
    file.tags
        .get(key)
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_default()
}

/// Says what was left alone and why, whether or not there is work to do.
///
/// The question a reader has when nothing happens — *why is my file not in
/// there* — is the same one they have when something does, and this command
/// skips for three quite different reasons.
///
/// Both forms of every sentence are written out because [`ui::plural`] puts
/// the count *in* the sentence, and a count that opens a sentence takes the
/// verb with it.
fn skipped(survey: &Survey, full: bool) {
    for (count, one, many) in [
        (
            survey.done,
            "has one already: --full computes it again",
            "have one already: --full computes them again",
        ),
        (
            survey.already_identified,
            "already carries a MusicBrainz recording id — what a lookup would \
             answer: --full fingerprints it anyway",
            "already carry a MusicBrainz recording id — what a lookup would \
             answer: --full fingerprints them anyway",
        ),
        (
            survey.named,
            "is named by its own tags: --full fingerprints it anyway",
            "are named by their own tags: --full fingerprints them anyway",
        ),
        (
            survey.no_length,
            "has no length on record, and a lookup needs one: aede scan reads it",
            "have no length on record, and a lookup needs one: aede scan reads them",
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
    let _ = full;
}

#[cfg(test)]
#[path = "fingerprint_tests.rs"]
mod tests;
