//! `aede extract` (`aede artwork`): write out the picture your files carry.
//!
//! The counterpart of `fetch --covers`, and the one to reach for first. Where
//! that command downloads a cover for an album that has none anywhere, this one
//! writes out an image the library **already holds** — so it needs no network,
//! cannot fetch the wrong edition's artwork, and is instant.
//!
//! # It is named for what it does
//!
//! `artwork` named the subject and not the act, and a reader who ran it and saw
//! `nothing to write` above a list of counts took it for a report — something
//! `doctor` would print — rather than a command that had just declined to write
//! because there was nothing to write. `extract` is the name now; `artwork`
//! stays as an alias, so nothing anybody typed before stops working. **A
//! command that writes must be named for the writing.**
//!
//! It exists because of a question that has an unobvious answer: an album can
//! have no `cover.jpg` in its folder and still not want one, because the
//! picture is inside the audio files. Players and file managers differ on which
//! they read, and a library where the artwork is only ever in one of the two
//! places will look empty in half of them.
//!
//! # It works on every format, not on FLAC alone
//!
//! Extraction goes through [`aede_core::artwork`], which reads the picture the
//! same way whatever carries it — a FLAC metadata block, an ID3 `APIC` frame,
//! an MP4 `covr` atom, a base64-wrapped block inside an Ogg comment. The
//! reasoning for that single path, rather than four hand-written extractors,
//! is written on that module.
//!
//! # Per folder, not per album
//!
//! A double album in `CD1` and `CD2` is one release and two folders, and a
//! cover image belongs to a folder — that is where every player looks for it.
//! So this walks folders, and each one that lacks an image gets the picture its
//! own files carry.
//!
//! # `--images`: everything else the files carry
//!
//! A tagged file often holds more than the front cover — the back of the
//! sleeve, the pages of a booklet, the label printed on the disc. Without
//! `--images` those are ignored, because the cover is the one image anything
//! else in the library reads. With it, they are written into an `artwork/`
//! subfolder — **not** beside the music, for the reason set out on
//! [`aede_core::coverart::EXTRAS`]: any image next to the tracks is taken for
//! the album's cover, and a back sleeve promoted to cover is worse than none.
//!
//! # It never writes into an audio file
//!
//! Not with `--images`, not with anything. Extraction reads a file and writes
//! next to it. Putting a cover *into* files that lack one is the obvious
//! symmetrical feature and it does not exist here on purpose: a program that
//! rewrites a music library's audio files is a program that can destroy one.

use aede_core::coverart::{self, Written};
use aede_core::model::Catalog;
use aede_core::{artwork, scan, text};

use super::Res;
use crate::args::Args;
use crate::ui;

/// A folder to write into, and the file the pictures come out of.
struct Target {
    folder: String,
    source: String,
    /// Whether the cover is to be written: `false` when the folder has an image
    /// already and only the other pictures are wanted.
    cover: bool,
}

/// What the command would do, and why it would leave the rest alone.
struct Survey {
    targets: Vec<Target>,
    /// Skipped: an image is already in the folder.
    has_image: usize,
    /// Skipped: no file in the folder carries a picture.
    nothing_inside: usize,
}

pub fn artwork(args: &Args) -> Res {
    let catalog = super::load(args)?;
    let scope = super::scope_of(args)?;
    let images = args.has("images");
    let survey = survey(&catalog, &scope, images);

    println!("{}", ui::section("Extract"));
    if survey.targets.is_empty() {
        println!("  {}", ui::dim("nothing to extract"));
        skipped(&survey, images);
        return Ok(());
    }
    let covers = survey.targets.iter().filter(|t| t.cover).count();
    if covers > 0 {
        // What is about to happen, before it happens. The old wording named a
        // state and left the reader to infer the action from it.
        println!(
            "  {} whose files carry a picture and whose folder has none: \
             the picture is written into each",
            ui::plural(covers, "folder")
        );
    }
    if images {
        println!(
            "  {} to look through for images other than the cover",
            ui::plural(survey.targets.len(), "folder")
        );
        println!(
            "  {}",
            ui::dim(&format!(
                "--images: the back, the booklet and the rest go into {}/ \
                 in each folder, so that nothing but the cover sits beside the music",
                coverart::EXTRAS
            ))
        );
    }
    skipped(&survey, images);

    if args.has("dry-run") {
        for target in &survey.targets {
            println!("  {}", ui::dim(&target.folder));
        }
        println!("  {}", ui::dim("nothing was written: --dry-run"));
        return Ok(());
    }

    let (mut written, mut extras, mut failed) = (0usize, 0usize, 0usize);
    for target in &survey.targets {
        if target.cover {
            match write_one(target) {
                Ok(path) => {
                    written += 1;
                    println!("  {} {path}", ui::green("→"));
                }
                Err(why) => {
                    failed += 1;
                    eprintln!("  {} {}: {why}", ui::red("×"), target.folder);
                }
            }
        }
        if !images {
            continue;
        }
        match write_others(target) {
            Ok(paths) => {
                extras += paths.len();
                for path in paths {
                    println!("  {} {path}", ui::green("→"));
                }
            }
            Err(why) => {
                failed += 1;
                eprintln!("  {} {}: {why}", ui::red("×"), target.folder);
            }
        }
    }

    // `plural` puts the count in the phrase, which keeps the summary out of
    // the agreement trap the skipped lines have to write both ways around.
    match images {
        true => println!(
            "{} {} and {} extracted, {failed} failed",
            ui::green("→"),
            ui::plural(written, "cover"),
            ui::plural(extras, "other image")
        ),
        false => println!(
            "{} {} extracted, {failed} failed",
            ui::green("→"),
            ui::plural(written, "cover")
        ),
    }
    if written > 0 {
        println!(
            "  {}",
            ui::dim("aede scan picks them up — nothing registers a file, the scan finds it")
        );
    }
    Ok(())
}

/// Pulls the picture out of one file and writes it beside the music.
///
/// The work itself is [`artwork::extract_into`], which is where the guards live
/// and where real containers are exercised. What is here is the command's
/// wording for the result.
fn write_one(target: &Target) -> Result<String, String> {
    artwork::extract_into(
        std::path::Path::new(&target.source),
        std::path::Path::new(&target.folder),
    )
    .map(|path| path.display().to_string())
}

/// Writes out the pictures that are not the cover, and names the ones it wrote.
///
/// A picture already on disk is not in the answer and not a failure: running
/// this twice over a library should say nothing the second time, not report
/// every folder as an error for having worked.
fn write_others(target: &Target) -> Result<Vec<String>, String> {
    let out = artwork::extract_extras_into(
        std::path::Path::new(&target.source),
        std::path::Path::new(&target.folder),
    )
    .map_err(|why| why.to_string())?;
    let mut wrote = Vec::new();
    for one in out {
        match one.wrote {
            Ok(Written::New(path)) => wrote.push(path.display().to_string()),
            Ok(Written::Already(_)) => {}
            Err(why) => return Err(format!("{}: {why}", one.kind.stem())),
        }
    }
    Ok(wrote)
}

/// Sorts every folder of the library into one of the three.
///
/// `images` widens the net: without it a folder that already has a cover is
/// finished, with it that folder may still hold a booklet nobody has seen.
///
/// Unlike `fetch --covers --images`, an `artwork/` folder that is already there
/// does **not** take a folder out of the list. Nothing here costs a request, so
/// opening a folder again is free — and it is how a picture added to the files
/// since the last run gets written out. Nothing is overwritten either way.
///
/// The pictures are read out of **one** file per folder — the first that
/// carries any. A tagger writes the same set into every track of a release, so
/// reading all of them would be a hundred times the work for the same images,
/// and the folders where that assumption is wrong are ones where the extra
/// pictures differ per track, which is not a thing a sleeve does.
fn survey(catalog: &Catalog, scope: &[String], images: bool) -> Survey {
    let mut out = Survey {
        targets: Vec::new(),
        has_image: 0,
        nothing_inside: 0,
    };
    // Folders in the order the catalog holds their files, so a run reads as a
    // walk through the library rather than in whatever order a map returns.
    let mut seen: Vec<&str> = Vec::new();
    for file in &catalog.files {
        if !super::in_scope(&file.path, scope) {
            continue;
        }
        let folder = text::folder(&file.path);
        if folder.is_empty() || seen.contains(&folder) {
            continue;
        }
        seen.push(folder);

        // The disk, not the catalog: `cover_path` answers for a release, this
        // question is about a folder, and the two differ for a double album.
        let has_image = scan::cover_in(std::path::Path::new(folder)).is_some();
        if has_image && !images {
            out.has_image += 1;
            continue;
        }
        match catalog
            .files
            .iter()
            .find(|f| f.has_embedded_art && text::folder(&f.path) == folder)
        {
            Some(source) => out.targets.push(Target {
                folder: folder.to_string(),
                source: source.path.clone(),
                cover: !has_image,
            }),
            None => out.nothing_inside += 1,
        }
    }
    out
}

/// Says what was left alone and why — the lesson `fetch --covers` had to learn
/// twice: a filter the reader cannot see is a trap, and "nothing happened" is
/// exactly when they most need to know which filter caught their album.
///
/// Both forms of every sentence are written out because [`ui::plural`] puts the
/// count *in* the sentence, and a count that opens a sentence takes the verb
/// with it: "1 folder have an image" is what happens otherwise, and it has now
/// happened three times in this program.
fn skipped(survey: &Survey, images: bool) {
    let already = match images {
        // With `--images` a folder that has a cover is still worth opening, so
        // it was not skipped and must not be reported as though it had been.
        true => (
            "already has an image in it",
            "already have an image in them",
        ),
        false => (
            "already has an image in it: --images writes out the back and the booklet too",
            "already have an image in them: --images writes out the back and the booklet too",
        ),
    };
    for (count, one, many) in [
        (survey.has_image, already.0, already.1),
        (
            survey.nothing_inside,
            "holds no picture inside its files: aede fetch --covers downloads one",
            "hold no picture inside their files: aede fetch --covers downloads one",
        ),
    ] {
        if count > 0 {
            let rest = match count {
                1 => one,
                _ => many,
            };
            println!(
                "  {}",
                ui::dim(&format!("{} {rest}", ui::plural(count, "folder")))
            );
        }
    }
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;
