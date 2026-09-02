//! `fetch --covers`: the front image, for albums that have none.
//!
//! # This command downloads. It does not extract.
//!
//! For one release it did both: a local extraction pass ran first, so that a
//! picture already inside the files was never downloaded. It was reverted —
//! see the note in `CLAUDE.md`. Two commands doing the same writing, one of
//! them without saying so in its name, was harder to hold in the head than the
//! duplicated request it saved. `aede extract` extracts, this downloads, and
//! the line that skips an album whose files carry the image names the other.
//!
//! # It never touches an album that already has artwork
//!
//! The catalog answers that offline and exactly: a file says whether the image
//! is inside it ([`aede_core::model::AudioFile::has_embedded_art`]) and a
//! release says whether one sits beside it
//! ([`aede_core::model::Release::cover_path`], which the scanner fills from
//! `cover.jpg`, `folder.jpg` and the rest). Either of them and the album is not
//! asked about at all — no request, no file, nothing.
//!
//! There is deliberately **no `--replace`**. Overwriting artwork somebody chose
//! is not a thing this command should be able to do by accident, and the one
//! way to change a cover stays what it always was: put the file there yourself.
//!
//! # Two requests, and the first one is small
//!
//! The archive answers a record with an *index* — a small JSON document naming
//! every image it holds and, for each, the thumbnail widths it has generated.
//! So the sizes are read out of the answer rather than guessed at, and a record
//! with no artwork costs one small request instead of a failed download.
//!
//! # What is written, and what is refused
//!
//! `cover.jpg` beside the music, which is the first name the scanner looks for:
//! the file is registered nowhere, the next scan simply discovers it, exactly
//! as it would one put there by hand. Bytes that are not a JPEG or a PNG are
//! **not written** — see [`aede_core::coverart::image_kind`] for why that guard
//! is the important one.
//!
//! # `--images`: the back, the booklet, the disc
//!
//! The archive usually holds more than the front. With `--images` the rest is
//! downloaded too, into an `artwork/` subfolder — never beside the music, for
//! the reason set out on [`aede_core::coverart::EXTRAS`].
//!
//! It widens what is asked about, and the header says so: an album that has a
//! cover already is skipped by the ordinary pass and is not skipped by this
//! one, because the cover is not the question. **The existence of `artwork/`
//! is the record that it has been done** — the same way `cover.jpg` is the
//! record for the front. Nothing is written into a file and nothing is written
//! into a database to remember it; the folder is the answer.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::coverart::{self, Size};
use aede_core::model::{Catalog, EntityKind};
use aede_core::sources::{self, Facts, ReleaseFacts, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, musicbrainz};

use crate::ui;

use super::Res;
use super::fetch::{Ask, Refusal, ask_with_backoff};

/// The width used when `--size` is not given.
///
/// Large enough to look right full-screen on a phone, small enough that a few
/// hundred of them are a few hundred megabytes rather than a few gigabytes.
/// A default rather than a required option because a value that is almost
/// always the same, demanded on every run, is a value nobody reads before
/// typing.
pub const DEFAULT_SIZE: Size = Size::Thumbnail(1200);

/// An album to ask about, and where its image would go.
struct Target {
    entity: EntityRef,
    title: String,
    folder: String,
    /// Where to ask: the edition when the tags name one, the album otherwise.
    url: String,
    /// `true` when [`Target::url`] is already the image itself.
    ///
    /// The archive was asked about this album before and answered with an
    /// address, which was stored. If the picture is gone from the folder since
    /// — deleted, or a folder restored from a backup without it — that address
    /// is still the answer. Fetching it directly costs **one** request instead
    /// of two and asks the service nothing it has already been asked.
    known: bool,
    /// Whether the front image is wanted: `false` for an album that has a cover
    /// already and is only in the list because `--images` wants the rest.
    cover: bool,
}

/// The pass.
pub fn run(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    size: Size,
    images: bool,
    dry_run: bool,
) -> Res {
    let survey = survey(catalog, held, images);
    let targets = &survey.targets;
    println!("{}", ui::section("Cover art"));
    if targets.is_empty() {
        println!("  {}", ui::dim("nothing to ask about"));
        skipped(&survey);
        return Ok(());
    }

    // Two requests for an album that has artwork, one for an album that has
    // none, so the estimate is a range rather than a number and says which.
    let least = targets.len() as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    match images {
        // With `--images` an album that has a cover is still worth asking
        // about, so the list is most of the library rather than the gaps in
        // it. Saying "album" here would understate an hour as a minute.
        true => println!(
            "  {} to ask about, cover or no cover, {} to {}",
            ui::plural(targets.len(), "album"),
            ui::long_duration(least),
            ui::long_duration(least * 2)
        ),
        false => println!(
            "  {} without a cover, one or two requests each, {} to {}",
            ui::plural(targets.len(), "album"),
            ui::long_duration(least),
            ui::long_duration(least * 2)
        ),
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "images are written as cover.jpg beside the music, at {}; \
             an album that already has one is never touched",
            size.as_str()
        ))
    );
    if images {
        println!(
            "  {}",
            ui::dim(&format!(
                "--images: the back, the booklet and the rest go into {}/ in each \
                 album's folder, and a folder that has one is not asked about again",
                coverart::EXTRAS
            ))
        );
    }
    skipped(&survey);
    if dry_run {
        for target in targets {
            println!("  {}", ui::dim(&target.title));
        }
        println!("  {}", ui::dim("nothing was asked: --dry-run"));
        return Ok(());
    }

    let (mut written, mut none, mut failed) = (0usize, 0usize, 0usize);
    let mut extras = 0usize;
    for (done, target) in targets.iter().enumerate() {
        print!("\r  asking: {}/{}", done + 1, targets.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // A known address is the image, so there is nothing to look up.
        if target.known {
            match download(transport, backoff, &target.url, &target.folder) {
                Ok(()) => written += 1,
                Err(why) => {
                    failed += 1;
                    eprintln!("\r  {} {}: {why}", ui::red("×"), target.title);
                }
            }
            continue;
        }

        let index = match ask_with_backoff(transport, &target.url, backoff) {
            Ok(index) => index,
            // The archive answers `404` for a record it holds no image of, so
            // a failure here is nearly always that: a real answer, recorded so
            // the next run does not ask again.
            Err(why) => {
                match why {
                    Refusal::Failed(ref detail) if detail.contains("404") => {
                        none += 1;
                        store(held, target, None);
                        sources::save(held, path)?;
                    }
                    other => {
                        failed += 1;
                        eprintln!("\r  {} {}: {other}", ui::red("×"), target.title);
                    }
                }
                continue;
            }
        };

        // The other images first, while the index is in hand: an album whose
        // front download fails still has a booklet worth keeping, and asking
        // the archive twice for the same document to avoid that would be rude.
        if images {
            match download_others(transport, backoff, &index, size, &target.folder) {
                Ok(count) => extras += count,
                Err(why) => {
                    failed += 1;
                    eprintln!("\r  {} {}: {why}", ui::red("×"), target.title);
                }
            }
        }

        if !target.cover {
            continue;
        }

        let Some(front) = coverart::front(&index, size) else {
            none += 1;
            store(held, target, None);
            sources::save(held, path)?;
            continue;
        };

        match download(transport, backoff, &front.url, &target.folder) {
            Ok(()) => {
                written += 1;
                store(held, target, Some(front.url));
                sources::save(held, path)?;
            }
            Err(why) => {
                failed += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.title);
            }
        }
    }
    println!();

    match images {
        true => println!(
            "{} {written} covers, {extras} other images, \
             {none} with no cover in the archive, {failed} failed",
            ui::green("→")
        ),
        false => println!(
            "{} {written} written, {none} with no cover in the archive, {failed} failed",
            ui::green("→")
        ),
    }
    if written > 0 {
        // The file is not in the catalog until something reads the disk again,
        // and a reader who is not told that will conclude the write failed.
        println!(
            "  {}",
            ui::dim("aede scan picks them up — nothing registers a file, the scan finds it")
        );
    }
    Ok(())
}

/// Downloads one image and writes it beside the music, or says why it did not.
fn download(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    url: &str,
    folder: &str,
) -> Result<(), String> {
    let bytes = ask_bytes(transport, url, backoff).map_err(|why| why.to_string())?;
    // Both guards — the bytes are an image, and nothing is overwritten — live
    // in one place, shared with `aede artwork`. Written twice they would
    // eventually differ, and the difference would only show as a corrupt file
    // in somebody's library.
    coverart::write_beside(std::path::Path::new(folder), &bytes).map(|_| ())
}

/// Downloads every image of an index that is not the front, and says how many.
///
/// Into `artwork/`, under names that say what each one is. A picture already on
/// disk is not downloaded twice and is not counted: the folder existing is what
/// stops a second run asking at all, and this is the guard behind that one.
fn download_others(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    index: &aede_core::json::Json,
    size: Size,
    folder: &str,
) -> Result<usize, String> {
    let all = coverart::images(index, size);
    let rest: Vec<(coverart::Kind, coverart::Front)> = all
        .into_iter()
        .filter(|(kind, _)| *kind != coverart::Kind::Front)
        .collect();
    let kinds: Vec<coverart::Kind> = rest.iter().map(|(kind, _)| *kind).collect();
    let places = coverart::positions(&kinds);
    let into = coverart::extras_in(std::path::Path::new(folder));

    let mut written = 0;
    for ((kind, image), where_) in rest.iter().zip(places) {
        let bytes = ask_bytes(transport, &image.url, backoff).map_err(|why| why.to_string())?;
        match coverart::write_image(&into, *kind, where_, &bytes)? {
            coverart::Written::New(_) => written += 1,
            coverart::Written::Already(_) => {}
        }
    }
    Ok(written)
}

/// [`ask_with_backoff`], for bytes.
fn ask_bytes(
    transport: &mut dyn Ask,
    url: &str,
    backoff: &[std::time::Duration],
) -> Result<Vec<u8>, Refusal> {
    let mut attempt = 0;
    loop {
        match transport.get_bytes(url) {
            Err(Refusal::RateLimited) if attempt < backoff.len() => {
                std::thread::sleep(backoff[attempt]);
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// Files what the archive answered, including when the answer was nothing.
fn store(held: &mut sources::Sources, target: &Target, cover: Option<String>) {
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: coverart::SOURCE.to_string(),
        source_id: None,
        fetched_at: clock::now_seconds(),
        // Asked by identifier, through a link the catalog or MusicBrainz
        // supplied: nothing here was matched by name.
        confidence: sources::Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            cover_art: cover,
            ..Default::default()
        }),
    });
}

/// What the pass would do, and why it would leave the rest alone.
///
/// The counts are not decoration. The first version printed one sentence for
/// four quite different states — "every album already has a cover, or none has
/// been identified yet" — and a reader who deleted a cover to see what would
/// happen had no way to learn that the image was inside the files all along.
/// **A filter the reader cannot see is a trap**, and this one was four filters
/// wearing a single word.
struct Survey {
    /// Albums with no artwork anywhere, that something has identified.
    targets: Vec<Target>,
    /// Skipped: the image is inside the files.
    embedded: usize,
    /// Skipped: an image sits in the folder already.
    beside: usize,
    /// Skipped: no cover, but nothing names this album to the archive.
    unidentified: usize,
    /// Skipped: the archive was already asked, and answered.
    asked: usize,
}

/// Looks at every album and sorts it into one of the five.
///
/// `images` changes what counts as finished. Without it the question is "has
/// this album a cover", and an album that has one is done. With it there is a
/// second question — "has this album's other artwork been fetched" — whose
/// answer is on the disk: an `artwork/` folder means yes. Nothing records it
/// anywhere else, for the same reason nothing records `cover.jpg`.
fn survey(catalog: &Catalog, held: &sources::Sources, images: bool) -> Survey {
    let mut out = Survey {
        targets: Vec::new(),
        embedded: 0,
        beside: 0,
        unidentified: 0,
        asked: 0,
    };
    for release in &catalog.releases {
        // Embedded first, because it is the answer that surprises people: an
        // album can have no `cover.jpg` at all and still not want one.
        let embedded = has_embedded_art(catalog, release);
        let cover = !embedded && release.cover_path.is_none();
        let extras = images && !coverart::extras_in(std::path::Path::new(&release.folder)).exists();
        if !cover && !extras {
            match embedded {
                true => out.embedded += 1,
                false => out.beside += 1,
            }
            continue;
        }
        let Some(entity) = EntityRef::of(catalog, EntityKind::Release, release.id) else {
            out.unidentified += 1;
            continue;
        };
        // Asked before — but *what* the answer was decides what happens now.
        //
        // "Asked, and the archive holds nothing" is a finished question, and
        // skipping it is what stops a second run costing an hour again. That
        // holds for `--images` too: an archive record with no front image
        // almost never has a back one either, and `aede sources --forget` is
        // there for the album where it does.
        //
        // "Asked, and here is the image" is not finished when the image is no
        // longer in the folder. The address is still good, so the picture is
        // fetched again straight from it — one request, and the reader does not
        // have to discover `sources --forget` to get back a file they deleted.
        // With `--images` that shortcut is no use: the stored address is the
        // front and says nothing about the rest, so the index is asked for.
        if let Some(record) = held.get(&entity, coverart::SOURCE) {
            let stored = match &record.facts {
                Facts::Release(ReleaseFacts { cover_art, .. }) => cover_art.clone(),
                _ => None,
            };
            match (stored, extras) {
                (Some(url), false) => {
                    out.targets.push(Target {
                        entity,
                        title: release.title.clone(),
                        folder: release.folder.clone(),
                        url,
                        known: true,
                        cover: true,
                    });
                    continue;
                }
                (None, _) => {
                    out.asked += 1;
                    continue;
                }
                (Some(_), true) => {}
            }
        }
        // The edition the shelf actually holds is a better question than the
        // one somebody else thought representative: a reissue often has other
        // artwork. Falling back to the album when the tags name no edition.
        let url = match release.mbid.as_deref() {
            Some(edition) => coverart::release_index_url(edition),
            None => match held
                .get(&entity, sources::MUSICBRAINZ)
                .and_then(|r| r.source_id.as_deref())
            {
                Some(group) => coverart::index_url(group),
                None => {
                    out.unidentified += 1;
                    continue;
                }
            },
        };
        out.targets.push(Target {
            entity,
            title: release.title.clone(),
            folder: release.folder.clone(),
            url,
            known: false,
            cover,
        });
    }
    out
}

/// The albums this pass would ask about.
fn targets(catalog: &Catalog, held: &sources::Sources) -> Vec<Target> {
    survey(catalog, held, false).targets
}

/// What was left alone and why, one line per reason that applies.
///
/// Returned rather than printed so that the wording is testable: the line that
/// matters most here is the one naming `aede extract`, and it was missing for a
/// release — a reader who deletes a cover reaches for this command, is told the
/// image is inside the files, and has been handed a fact with no way forward.
/// **A command named only where nobody is looking is a command nobody has.**
///
/// Both forms of every sentence are written out because [`ui::plural`] puts the
/// count *in* the sentence, and a count that opens a sentence takes the verb
/// with it.
fn reasons(survey: &Survey) -> Vec<String> {
    let mut lines = Vec::new();
    for (count, one, many) in [
        (
            survey.embedded,
            "carries the image inside its files: aede extract writes it out beside them",
            "carry the image inside their files: aede extract writes it out beside them",
        ),
        (
            survey.beside,
            "already has an image in its folder",
            "already have an image in their folders",
        ),
        (
            survey.unidentified,
            "has no cover and nothing identifying it: aede fetch names it first",
            "have no cover and nothing identifying them: aede fetch names them first",
        ),
        // Only "asked, and the archive had nothing" reaches this line now: an
        // answer that named an image is re-fetched from the address it gave
        // rather than skipped, so nobody has to find `--forget` to get back a
        // file they deleted.
        (
            survey.asked,
            "was asked about already and the archive had no front image for it",
            "were asked about already and the archive had no front image for them",
        ),
    ] {
        if count > 0 {
            let rest = match count {
                1 => one,
                _ => many,
            };
            lines.push(format!("{} {rest}", ui::plural(count, "album")));
        }
    }
    lines
}

/// Prints what [`reasons`] worked out, whether or not there is anything to do:
/// the question a reader has when nothing happens — *why is my album not in
/// there* — is the same one they have when something does.
fn skipped(survey: &Survey) {
    for line in reasons(survey) {
        println!("  {}", ui::dim(&line));
    }
}

/// `true` when any file of the release carries its own image.
fn has_embedded_art(catalog: &Catalog, release: &aede_core::model::Release) -> bool {
    release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|track| catalog.file(track.file_id))
        .any(|file| file.has_embedded_art)
}

/// How many albums a `--covers` pass would ask about, if it ran now.
pub fn waiting(catalog: &Catalog, held: &sources::Sources) -> usize {
    targets(catalog, held).len()
}

#[cfg(test)]
#[path = "covers_tests.rs"]
mod tests;
