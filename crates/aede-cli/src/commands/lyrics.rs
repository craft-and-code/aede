//! `fetch --lyrics`: the words, for tracks that have none.
//!
//! # Never on by default, and that is not a technical decision
//!
//! Lyrics are the **composition's** copyright. Owning a FLAC grants no rights
//! in it — the recording and the song are legally two different objects — and
//! every comparable project keeps online fetching out of its core for exactly
//! that reason: Navidrome and Jellyfin read files and leave the network to
//! plugins, beets ships a `lyrics` plugin, foobar2000 and MusicBee do it
//! through add-ons.
//!
//! So this pass runs only when somebody types `--lyrics`, and it says what it
//! is about to do **before it asks anything** — every run, not once. That is a
//! decision about somebody else's rights, and this program does not get to
//! make it silently on a user's behalf. `docs/design/lyrics.md` has the whole
//! of the reasoning.
//!
//! The typed option is the consent, and it is deliberately not doubled by a
//! prompt on every run: a confirmation asked every time is a confirmation
//! nobody reads, which would leave the caveat *less* read than printing it
//! plainly does. A long run is still agreed to, on the same threshold and for
//! the same reason as the ordinary fetch — that is about the ten minutes, not
//! about the rights.
//!
//! # It never touches a track that already has words
//!
//! The catalog answers that offline and exactly: a file's tags say whether the
//! words are inside it, and [`aede_core::model::AudioFile::lyrics_path`] says
//! whether a `.lrc` sits beside it. Either of them and the track is not asked
//! about at all — no request, no file, nothing.
//!
//! There is deliberately **no `--replace`**. Overwriting a lyrics file somebody
//! wrote or corrected is not a thing this command should be able to do by
//! accident, and it is the same refusal `--covers` makes about artwork.
//!
//! # What is written
//!
//! A `.lrc` beside the track, which is the file every player already looks for
//! and the one the scanner already picks up from the walk. It is registered
//! nowhere: the next scan simply discovers it, exactly as it would one put
//! there by hand.
//!
//! **Nothing is written into an audio file, here or anywhere in this program.**
//!
//! # A track the service has nothing for is asked about again
//!
//! Said plainly rather than hidden, because it is the one cost of this pass
//! that a reader might not expect. `--covers` records that the archive had
//! nothing, so a second run does not ask twice; here a *successful* answer
//! records itself — the `.lrc` is on the disk and the track is skipped from
//! then on — and only the misses come back. Recording those would mean a store
//! of negative claims about a service that gains entries every day, which is a
//! worse trade than one wasted request on a run somebody typed on purpose.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::lrclib::{self, Found};
use aede_core::model::{Catalog, EntityKind};
use aede_core::musicbrainz;

use crate::args::Args;
use crate::ui;

use super::Res;
use super::fetch::{Ask, Refusal, ask_with_backoff, queue, worth_deferring};

/// A track to ask about, and where its words would go.
struct Target {
    /// What to ask.
    url: String,
    /// The track, for a line a reader can recognise.
    title: String,
    artist: String,
    /// Where the `.lrc` goes: beside the audio, same name.
    sidecar: std::path::PathBuf,
}

/// Why a track was left out, so the header can say so rather than silently
/// shortening the list.
#[derive(Default)]
struct Skipped {
    /// The words are in the file's own tags.
    tagged: usize,
    /// A `.lrc` is already beside it.
    sidecar: usize,
    /// No length, no artist or no title.
    ///
    /// Only the first two are the service's requirement — a live answer to the
    /// artist and title alone came back `200`. The **length** is this program's
    /// insistence: a song has a studio take, a live one and three covers, they
    /// share a title, and without the length the answer would be about
    /// whichever of them the service reached first.
    unaskable: usize,
}

/// The pass.
pub fn run(
    args: &Args,
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    asked: &super::fetch::Asked,
) -> Res {
    let (targets, skipped) = survey(catalog, asked.names, asked.scope);

    println!("{}", ui::section("Lyrics"));
    // Before the counts, and before anything is asked. A reader deciding
    // whether to run this needs to know what it is they are deciding.
    println!(
        "  {}",
        ui::dim(
            "lyrics are the song's copyright, not the recording's: owning the \
             file grants no rights in the words. This asks LRCLIB, which \
             permits it, and runs only when you type --lyrics"
        )
    );
    report(&skipped);

    if targets.is_empty() {
        match asked.names.is_empty() && asked.scope.is_empty() {
            true => println!("  {}", ui::dim("no track is missing its words")),
            false => println!(
                "  {}",
                ui::dim(&format!(
                    "no track missing its words answers to {}",
                    super::fetch::narrowing(asked.names, asked.scope)
                ))
            ),
        }
        return Ok(());
    }

    let wait = targets.len() as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {} without words, one request each, about {}",
        ui::plural(targets.len(), "track"),
        ui::long_duration(wait)
    );
    println!(
        "  {}",
        ui::dim(&format!(
            "each answer is written as a .{} beside its track; a track that \
             already has words is never touched, and nothing is ever written \
             into an audio file",
            aede_core::lyrics::EXTENSION
        ))
    );
    // The polite alternative, named where somebody about to spend six hours
    // will read it. LRCLIB publishes its whole database as SQLite dumps for
    // exactly this case, and a program that knew and did not say would be
    // spending a free service's bandwidth on the user's behalf.
    if targets.len() > BULK {
        println!(
            "  {}",
            ui::yellow(&format!(
                "that is {} requests to a free service — for a library this \
                 size the polite way is LRCLIB's published database dump",
                targets.len()
            ))
        );
    }
    if super::fetch::asked_nothing(
        asked,
        &targets.iter().map(|t| t.title.clone()).collect::<Vec<_>>(),
    ) {
        return Ok(());
    }
    // A run of ten minutes is something to agree to, not something to
    // discover — the same threshold the ordinary fetch uses, and about the
    // same thing: the time, not the rights. The caveat above is printed
    // whatever the length.
    if targets.len() > super::fetch::CONFIRM_ABOVE
        && !super::confirmed(args, "ask about all of them")?
    {
        println!("  {}", ui::dim("nothing was asked"));
        return Ok(());
    }

    let (mut written, mut none, mut instrumental, mut failed) = (0usize, 0usize, 0usize, 0usize);
    let mut timed = 0usize;
    let mut pending = queue(&targets);
    let mut done = 0usize;
    let total = targets.len();
    while let Some((target, retried)) = pending.pop_front() {
        print!("\r  asking: {}/{}", done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        match ask_with_backoff(transport, &target.url, backoff) {
            Ok(answer) => match lrclib::read(&answer) {
                Found::Words(words) => match write_beside(&target.sidecar, &words.text) {
                    Ok(()) => {
                        written += 1;
                        if words.synced {
                            timed += 1;
                        }
                    }
                    Err(why) => {
                        failed += 1;
                        eprintln!("\r  {} {}: {why}", ui::red("×"), target.title);
                    }
                },
                Found::Instrumental => instrumental += 1,
                Found::Nothing => none += 1,
            },
            // A track the service does not know is the ordinary case, not a
            // failure: it answers 404, and a library of any size holds plenty
            // it has never seen.
            Err(Refusal::Missing) => none += 1,
            Err(Refusal::RateLimited) => {
                println!();
                return Err(format!(
                    "the service refused {} times in a row, waiting longer each \
                     time; that is a rate limit rather than a hiccup, so nothing \
                     more was asked.\n  it stopped on \"{}\"",
                    backoff.len() + 1,
                    target.title
                )
                .into());
            }
            Err(other) if worth_deferring(&other) && !retried => {
                pending.push_back((target, true));
                continue;
            }
            Err(other) => {
                failed += 1;
                eprintln!(
                    "\r  {} {} — {}: {other}",
                    ui::red("×"),
                    target.artist,
                    target.title
                );
            }
        }
        done += 1;
    }
    println!();

    println!("  {} written", ui::plural(written, "lyrics file"));
    if timed > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} of them timed, which a player can follow",
                timed
            ))
        );
    }
    if instrumental > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} — the service says so, and says it once",
                ui::plural(instrumental, "instrumental")
            ))
        );
    }
    if none > 0 {
        // Named, with what it costs, because it is the one thing here a second
        // run repeats. See the note at the top of this file for why nothing is
        // stored about it.
        println!(
            "  {}",
            ui::dim(&format!(
                "{} the service has nothing for; a later run asks again",
                ui::plural(none, "track")
            ))
        );
    }
    if failed > 0 {
        println!("  {}", ui::yellow(&format!("{failed} could not be asked")));
    }
    if written > 0 {
        println!(
            "  {}",
            ui::dim("aede scan attaches them; aede track \"<title>\" --lyrics shows one")
        );
    }
    Ok(())
}

/// Above this many requests, the run names the published database instead.
///
/// A number rather than a judgement call, and a low one: five hundred requests
/// at a request a second is nine minutes, which is the point where somebody
/// would rather have been told there was another way.
const BULK: usize = 500;

/// Writes a lyrics file beside its track, and never over one.
///
/// The check is here rather than only in [`survey`] because the two are
/// separated by a network: a `.lrc` that appeared while the run was working —
/// a second copy of the program, a file dropped in by hand — must not be
/// overwritten by an answer that was asked for before it existed. The same
/// reasoning as the cover art pass, and the same refusal.
fn write_beside(path: &std::path::Path, text: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "{} appeared while this was running and was left alone",
            path.display()
        ));
    }
    // A trailing newline, because this is a text file somebody will open in an
    // editor and every other text file on their disk has one.
    let text = match text.ends_with('\n') {
        true => text.to_string(),
        false => format!("{text}\n"),
    };
    std::fs::write(path, text).map_err(|why| format!("could not write {}: {why}", path.display()))
}

/// The tracks worth asking about, and what was left out.
///
/// A track is worth asking about when it has **no words already** and can be
/// asked about at all: the service matches on artist, title, album and length,
/// so a track missing any of them cannot be looked up — and saying that is
/// better than sending a request that cannot match.
fn survey(
    catalog: &Catalog,
    wanted: &[String],
    scope: &super::fetch::Scope,
) -> (Vec<Target>, Skipped) {
    let mut targets = Vec::new();
    let mut skipped = Skipped::default();
    for track in &catalog.tracks {
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        // Out of the folders asked about: not skipped, not counted, not here
        // at all. The counts below explain the tracks this run *could* have
        // asked about, and a track on another shelf was never one of them.
        if !scope.has_track(track.id) {
            continue;
        }
        // The main credit, which is what the service matches on. `credits_on`
        // is the one place a track's artists are worked out, and going round
        // it would be a second answer to the same question.
        let artist = catalog
            .credits_on(EntityKind::Track, track.id)
            .into_iter()
            .find(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .unwrap_or_default();
        if !super::fetch::reaches(wanted, &[track.title.as_str(), artist.as_str()]) {
            continue;
        }
        // In the file's own tags: the words are there, and this pass has
        // nothing to add.
        if file.tags.contains_key("lyrics") {
            skipped.tagged += 1;
            continue;
        }
        if file.lyrics_path.is_some() {
            skipped.sidecar += 1;
            continue;
        }
        let album = track
            .release_id
            .and_then(|id| catalog.release(id))
            .map(|r| r.title.clone())
            .unwrap_or_default();
        // The album may be missing and the question still be a good one — it
        // is simply left out of the address. The length may not: see
        // [`Skipped::unaskable`].
        let (Some(duration), false, false) = (
            track.duration_ms,
            artist.trim().is_empty(),
            track.title.trim().is_empty(),
        ) else {
            skipped.unaskable += 1;
            continue;
        };
        targets.push(Target {
            // Seconds, which is what the service matches on.
            url: lrclib::get_url(&artist, &track.title, &album, duration / 1000),
            title: track.title.clone(),
            artist,
            sidecar: aede_core::lyrics::sidecar_of(std::path::Path::new(&file.path)),
        });
    }
    (targets, skipped)
}

/// Says what was left out and why.
///
/// A filter the reader cannot see is a trap: a run that asked about nine tracks
/// on a library of four hundred owes an explanation, and "the other three
/// hundred and ninety-one already have their words" is a good one.
fn report(skipped: &Skipped) {
    // **Participles, not verbs.** `ui::plural` agrees the noun and cannot agree
    // what follows it, so "1 track already have a lyrics file beside them" is
    // what a hard-coded verb produces — and it shipped, in the first run of
    // this pass. A phrase that reads correctly at one and at many is the fix,
    // not a second branch per line.
    for (count, why) in [
        (skipped.tagged, "already carrying the words in the tags"),
        (skipped.sidecar, "already with a lyrics file alongside"),
        (
            skipped.unaskable,
            "with no artist, title or measured length to ask about — a length \
             is what tells a studio take from a live one",
        ),
    ] {
        if count > 0 {
            println!(
                "  {}",
                ui::dim(&format!("{} {why}", ui::plural(count, "track")))
            );
        }
    }
}

#[cfg(test)]
#[path = "lyrics_tests.rs"]
mod tests;
