//! The `copy` command: put a selection somewhere that is not the library.
//!
//! A player, a card, an external drive. The destination is not a catalog and
//! will never be scanned, which is what separates this from everything else the
//! program does: it is the only command that writes files, and it writes them
//! **outside**. Nothing it does touches the library, the catalog, or what the
//! user wrote.
//!
//! The selection is the grammar's, not a set of filters of its own — the same
//! rule the listings follow. `aede copy /Volumes/USB --query "loved
//! rating:>=4"` reuses, exactly, what `aede query` would have shown.

use std::path::{Path, PathBuf};

use aede_core::copy::transcode::{self, Quality, Target};
use aede_core::copy::{self, Extras, Item, ItemKind, Plan, Recipe};
use aede_core::model::Catalog;
use aede_core::text;

use super::{Res, canonical, load, user_data};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// Files copied between two redraws of the progress line.
const REDRAW_EVERY: usize = 4;

pub fn copy(args: &Args) -> Res {
    let destination = destination(args)?;
    let catalog = load(args)?;

    // Copying a library into itself is not a mistake to notice afterwards: the
    // next scan reads the copies as new files, the catalog doubles, and
    // `doctor` reports every album as a duplicate of itself.
    if let Some(root) = copy::inside_a_watched_root(&catalog, &destination) {
        return Err(format!(
            "\"{}\" is inside the watched folder \"{root}\".\n\
             A copy is not a library: the next scan would read it back in and \
             every album would become its own duplicate.\n\
             Choose a destination outside your watched folders.",
            destination.display()
        )
        .into());
    }

    let extras = match args.value("extras") {
        None => Extras::default(),
        Some(word) => Extras::parse(word)
            .ok_or_else(|| format!("--extras takes {}: got \"{word}\"", Extras::NAMES))?,
    };
    let tracks = selection(args, &catalog)?;
    if tracks.is_empty() {
        return Err("nothing to copy: that selection matches no track".into());
    }

    // Asked of the volume rather than inferred from a filesystem name, and
    // overridable in both directions because a probe can only answer for the
    // folder it was run in.
    let restrict = match (args.has("safe-names"), args.has("raw-names")) {
        (true, true) => return Err("--safe-names and --raw-names ask for opposite things".into()),
        (true, false) => true,
        (false, true) => false,
        (false, false) => copy::names::restricts_names(&destination),
    };

    let target = convert(args)?;
    let recipe = Recipe {
        extras,
        restrict_names: restrict,
        convert: target,
        quality: quality(args, target)?,
    };

    let plan = copy::plan(&catalog, &tracks, &recipe);
    announce(&plan, &destination, restrict, &recipe);
    if args.has("dry-run") {
        println!(
            "  {}",
            ui::dim("--dry-run: nothing was written. Drop it to copy.")
        );
        return Ok(());
    }
    if plan.items.is_empty() {
        return Err("nothing to copy".into());
    }
    // Looked for once, before the first file rather than on each of nine
    // hundred — and before the first byte is written, so a missing encoder is
    // an error at the start rather than half a copy.
    let ffmpeg = match plan.converted() > 0 {
        false => None,
        true => Some(transcode::find_ffmpeg().ok_or_else(transcode::missing_ffmpeg)?),
    };
    room_for(&plan, &destination)?;
    run(&plan, &destination, args, &recipe, ffmpeg.as_deref())
}

/// The format to encode into, when one was asked for.
fn convert(args: &Args) -> Result<Option<Target>, Box<dyn std::error::Error>> {
    let Some(word) = args.value("compress") else {
        return Ok(None);
    };
    Target::parse(word)
        .map(Some)
        .ok_or_else(|| format!("--compress takes {}: got \"{word}\"", Target::names()).into())
}

/// How hard the encoder should try, when it was said.
///
/// Refused rather than ignored in the two cases where it cannot be honoured,
/// and the second is the interesting one. `--compress wav --quality 128k` reads
/// as a request for small files; WAV has no quality setting at all, so the
/// option went into the void and the run produced files roughly eleven times
/// larger than the number that had just been typed. On the card this command
/// exists to fill, that is the difference between fitting and not.
///
/// Stopping costs nothing here — the check happens before a single file is
/// read — which is what settles it against a warning: a warning scrolls past a
/// plan and a progress line, and the run that follows is the wrong one.
fn quality(
    args: &Args,
    target: Option<Target>,
) -> Result<Option<Quality>, Box<dyn std::error::Error>> {
    let Some(word) = args.value("quality") else {
        return Ok(None);
    };
    let Some(target) = target else {
        return Err(
            "--quality says how to encode, and nothing is being encoded: add --compress".into(),
        );
    };
    if target.lossless() {
        return Err(format!(
            "--quality means nothing for {}: a lossless format keeps every sample, \
             so there is no quality to choose.\n\
             It applies to {}.\n\
             Drop --quality, or compress to one of those.",
            args.value("compress").unwrap_or("that format"),
            Target::lossy_names()
        )
        .into());
    }
    Quality::parse(word)
        .map(Some)
        .ok_or_else(|| format!("--quality takes {}: got \"{word}\"", Quality::FORMS).into())
}

/// Where the copy is going, checked before anything else is read.
///
/// **The folder must already exist.** Creating it would be friendlier exactly
/// once and catastrophic the rest of the time: `aede copy /Volumes/Player` with
/// the player unplugged would create `/Volumes/Player` on the internal disk and
/// quietly fill it with sixty gigabytes, leaving the user with a full disk, no
/// copy, and nothing on screen that said so.
///
/// **And it comes back canonical**, because the guard downstream compares it
/// against the watched roots — which `scan` stores canonical — as strings. A
/// destination reached through a symbolic link names the same folder by a
/// string that never compares equal, and "is this inside my library" answered
/// `no` for a folder that plainly was. On macOS that is the ordinary case
/// rather than a corner one: `/var` is a link to `/private/var`, so every path
/// under it exists in two spellings and only one of them is the catalog's.
/// `scan` and `check` both canonicalize what they are given; this is the same
/// step, in the one command that writes.
fn destination(args: &Args) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let Some(raw) = args.positionals.first() else {
        return Err("where to? aede copy /Volumes/Player --query \"loved\"".into());
    };
    if args.positionals.len() > 1 {
        return Err(format!(
            "one destination at a time: \"{}\" was not understood.\n\
             The selection goes in --query, not in the arguments.",
            args.positionals[1..].join(" ")
        )
        .into());
    }
    let path = PathBuf::from(raw);
    if !path.exists() {
        return Err(format!(
            "\"{raw}\" does not exist.\n\
             The folder is not created for you: a destination that is missing is \
             usually a drive that is not plugged in, and filling the internal disk \
             instead is the one outcome nobody wants."
        )
        .into());
    }
    if !path.is_dir() {
        return Err(format!("\"{raw}\" is not a folder").into());
    }
    Ok(canonical(&path))
}

/// The tracks to copy: an expression, a saved collection, or the whole library.
fn selection(
    args: &Args,
    catalog: &Catalog,
) -> Result<Vec<aede_core::model::Id>, Box<dyn std::error::Error>> {
    let data = user_data(args, catalog)?;
    let context = aede_core::query::Context {
        catalog,
        data: &data,
        owner: aede_core::user::LOCAL_USER,
    };
    let expression = match (args.value("query"), args.value("collection")) {
        (Some(_), Some(_)) => {
            return Err("--query and --collection both name a selection: give one".into());
        }
        (Some(expression), None) => expression.to_string(),
        (None, Some(name)) => data
            .collection(aede_core::user::LOCAL_USER, name)
            .map(|c| c.expression.clone())
            .ok_or_else(|| format!("no collection is called \"{name}\""))?,
        // No selection is the whole library, which is what somebody filling an
        // empty drive means. It is not a mistake, so it is not an error.
        (None, None) => String::new(),
    };
    let parsed = aede_core::query::parse(&expression)?;
    if let Some((what, value)) = aede_core::query::unknown_values(&parsed, &context).first() {
        return Err(
            format!("no {what} matches \"{value}\".\nRun \"aede {what}s\" for the list.").into(),
        );
    }
    Ok(aede_core::query::run(&parsed, &context))
}

/// What the copy is about to do, said before it does it.
fn announce(plan: &Plan, destination: &Path, restrict: bool, recipe: &Recipe) {
    println!("{}", ui::section("Copy"));
    let counts = plan.counts();
    let mut table = Table::plain(2).align(1, Align::Right);
    for (kind, label) in [
        (ItemKind::Audio, "Tracks"),
        (ItemKind::Cover, "Covers"),
        (ItemKind::Other, "Other files"),
    ] {
        if let Some(count) = counts.get(&kind) {
            table.push(vec![label.into(), count.to_string()]);
        }
    }
    if plan.converted() > 0 {
        table.push(vec!["To encode".into(), plan.converted().to_string()]);
        let untouched = counts.get(&ItemKind::Audio).copied().unwrap_or(0) - plan.converted();
        if untouched > 0 {
            table.push(vec!["Copied as they are".into(), untouched.to_string()]);
        }
    }
    table.push(vec![
        match plan.size_is_estimated() {
            true => "Size (estimated)".into(),
            false => "Size".to_string(),
        },
        text::format_size(plan.total_bytes()),
    ]);
    print!("{}", table.render());
    println!("  {}", ui::dim(&format!("to {}", destination.display())));
    if plan.size_is_estimated() {
        println!(
            "  {}",
            ui::dim("what an encoder produces is not known until it has: the size is a guess")
        );
    }
    // Said plainly, because it is the one thing about a conversion that
    // surprises people: a library that is half MP3 already comes out half
    // untouched, and a silent skip would look like files had been lost.
    let audio = counts.get(&ItemKind::Audio).copied().unwrap_or(0);
    if plan.converted() > 0 && plan.converted() < audio {
        println!(
            "  {}",
            ui::dim("what is already compressed is copied rather than encoded a second time")
        );
    }
    // And the case where *nothing* is encoded, which is the same silence seen
    // from the other side: `--compress mp3` over a selection that is already
    // MP3 did its job perfectly and said nothing at all about it, leaving the
    // option looking ignored. It was honoured; it simply had nothing to do.
    if recipe.convert.is_some() && plan.converted() == 0 && audio > 0 {
        println!(
            "  {}",
            ui::yellow(
                "nothing here needs encoding: every track is already compressed, \
                       or already in that format"
            )
        );
    }
    if restrict {
        println!(
            "  {}",
            ui::dim("this destination refuses some characters: names are adapted")
        );
    }

    // Renames are listed, not counted. A copy whose names differ from the
    // library is a copy nobody can compare against the original, and "37 files
    // renamed" is not something anyone can act on.
    if !plan.renamed.is_empty() {
        println!("{}", ui::section("Renamed"));
        let mut t = Table::new(&["In the library", "On the destination"]);
        for renamed in plan.renamed.iter().take(20) {
            t.push(vec![
                text::file_name(&renamed.from.to_string_lossy()).to_string(),
                text::file_name(&renamed.to.to_string_lossy()).to_string(),
            ]);
        }
        print!("{}", t.render());
        if plan.renamed.len() > 20 {
            println!(
                "  {}",
                ui::dim(&format!("… and {} more", plan.renamed.len() - 20))
            );
        }
    }

    if !plan.rootless.is_empty() {
        println!("{}", ui::section("No tree to keep"));
        println!(
            "  {}",
            ui::yellow(&format!(
                "{} under no watched folder, and so with no place in the copy",
                ui::plural(plan.rootless.len(), "file")
            ))
        );
        for path in plan.rootless.iter().take(5) {
            println!("      {}", ui::dim(&path.to_string_lossy()));
        }
    }
}

/// Refuses a copy that cannot fit, before writing the first byte.
///
/// Filling a card to the last byte and failing on the final album is a waste of
/// twenty minutes that one `statvfs`-shaped question could have avoided. The
/// answer is approximate — other things may be writing to the same volume — so
/// it refuses only when the shortfall is plain.
fn room_for(plan: &Plan, destination: &Path) -> Res {
    let Some(free) = free_space(destination) else {
        return Ok(());
    };
    let needed = plan.total_bytes();
    if needed <= free {
        return Ok(());
    }
    Err(format!(
        "not enough room: {} to copy, {} free on {}.\n\
         Narrow the selection with --query, or free some space.",
        text::format_size(needed),
        text::format_size(free),
        destination.display()
    )
    .into())
}

/// Bytes free on the volume holding a folder, when the platform will say.
///
/// `None` rather than a guess when it will not: refusing a copy on a number
/// nobody produced would be worse than not checking.
fn free_space(destination: &Path) -> Option<u64> {
    // No `libc` in this project, and one number does not justify one. `df` is
    // specified by POSIX, present everywhere this runs, and its `-k` output is
    // the one shape that does not vary between implementations.
    let output = std::process::Command::new("df")
        .arg("-k")
        .arg(destination)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().nth(1)?;
    let available: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(available * 1024)
}

/// Writes the plan, saying where it is as it goes.
fn run(plan: &Plan, destination: &Path, args: &Args, recipe: &Recipe, ffmpeg: Option<&str>) -> Res {
    let verify = args.has("verify");
    let replace = args.has("replace");
    let interactive = ui::is_interactive();
    let total = plan.items.len();

    let mut copied = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();

    for (done, item) in plan.items.iter().enumerate() {
        match write_one(item, destination, verify, replace, recipe, ffmpeg) {
            Ok(copy::Wrote::Copied) => copied += 1,
            Ok(copy::Wrote::Skipped) => skipped += 1,
            // One unreadable file does not end the run: the other nine hundred
            // are still worth copying, and the report names every failure.
            Err(reason) => failures.push((item.source.clone(), reason)),
        }
        if interactive && ((done + 1).is_multiple_of(REDRAW_EVERY) || done + 1 == total) {
            print!("\r  {}/{total}   ", done + 1);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
    }
    if interactive {
        println!("\r  {total}/{total}   ");
    }

    println!("{}", ui::section("Copied"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Written".into(), copied.to_string()]);
    if skipped > 0 {
        table.push(vec!["Already there".into(), skipped.to_string()]);
    }
    if !failures.is_empty() {
        table.push(vec!["Failed".into(), failures.len().to_string()]);
    }
    print!("{}", table.render());
    if verify {
        println!(
            "  {}",
            ui::dim("each file was read back and compared with what was read")
        );
    }
    if skipped > 0 {
        println!(
            "  {}",
            ui::dim(
                "what was already there at the right size was left alone — \
                    --replace writes it again"
            )
        );
    }

    if !failures.is_empty() {
        println!("{}", ui::section("Failed"));
        let mut t = Table::new(&["File", "Reason"]).path_limit(0, 50);
        for (path, reason) in failures.iter().take(20) {
            t.push(vec![path.to_string_lossy().to_string(), reason.clone()]);
        }
        print!("{}", t.render());
        // A run that lost files must not report success: a script that copies
        // to a card and then wipes the source has to be able to tell.
        return Err(format!("{} could not be copied", ui::plural(failures.len(), "file")).into());
    }
    Ok(())
}

fn write_one(
    item: &Item,
    destination: &Path,
    verify: bool,
    replace: bool,
    recipe: &Recipe,
    ffmpeg: Option<&str>,
) -> Result<copy::Wrote, String> {
    let target = destination.join(&item.relative);
    let Some(format) = item.convert else {
        return copy::copy_one(&item.source, &target, item.size, verify, replace);
    };
    let Some(ffmpeg) = ffmpeg else {
        return Err("no encoder".into());
    };

    // A file already there is left alone, as in a plain copy — but the test
    // cannot be the size, which for an encoder's output is a guess. Existence
    // and a non-empty length is what can honestly be checked, so an
    // interrupted conversion is finished rather than restarted, and --replace
    // is how somebody who changed the quality asks for the work again.
    if !replace
        && let Ok(existing) = std::fs::metadata(&target)
        && existing.is_file()
        && existing.len() > 0
    {
        return Ok(copy::Wrote::Skipped);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }

    // Encoded under a temporary name and moved into place, exactly as a copy
    // is: a run interrupted mid-encode must never leave a half file wearing a
    // whole one's name, or the resume above would count it as done.
    let partial = copy::partial_path(&target);
    let outcome = transcode::convert(ffmpeg, &item.source, &partial, format, recipe.quality)
        .and_then(|()| match verify {
            // Comparing checksums here would be meaningless: the bytes differ
            // by construction. What can be checked is that the result reads
            // back as audio of the right length — which catches the failure
            // that happens, an encode cut short.
            true => transcode::verify(&partial, source_duration(&item.source)),
            false => Ok(()),
        });
    if let Err(reason) = outcome {
        let _ = std::fs::remove_file(&partial);
        return Err(reason);
    }
    std::fs::rename(&partial, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(copy::Wrote::Copied)
}

/// How long the source plays, read from the file rather than from the catalog.
///
/// The catalog would be quicker, and wrong the day the file changed without a
/// scan: a verification that trusts a stale figure verifies nothing.
fn source_duration(source: &Path) -> Option<u64> {
    aede_core::tags::read(source)
        .ok()
        .and_then(|tags| tags.properties.duration_ms)
}
