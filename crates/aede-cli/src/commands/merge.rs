//! The `merge` command: saying that two spellings are one musician.
//!
//! The half of artist identity no authority can settle. `MUSICBRAINZ_ARTISTID`
//! answers for the files that went through Picard, and `model::identity` uses
//! it without a heuristic; for an old rip, a download or a friend's drive there
//! is nothing to consult, and comparing the strings is exactly what must not
//! happen — a fragment match merges Angus Young with Neil Young. **Nobody on
//! earth knows that your `O. Osbourne` is Ozzy except you**, so the program
//! asks.
//!
//! Three properties hold everywhere in here.
//!
//! **Nothing on disk is touched.** Not the audio files, not their tags, not
//! what any source said. A merge is a statement about how the shelf is *read*,
//! it lives in `user.json` with everything else that was said rather than
//! derived, and `--forget` takes it back.
//!
//! **It takes effect on the next scan**, because the spelling a track is filed
//! under is decided as the artist is interned and there is no later moment at
//! which one row can become another without rebuilding what points at it. The
//! command says so rather than leaving the reader to notice.
//!
//! **Two identifiers are two people.** Where both spellings carry a
//! MusicBrainz identifier and the identifiers differ, the merge is refused: the
//! disagreement is with MusicBrainz, it is a fact somebody else can check, and
//! the place to fix it is there. That is the one case in this file where the
//! program knows better than the person typing.

use aede_core::model::Catalog;
use aede_core::text;
use aede_core::user::{self, SameArtist, UserData};

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// What the owner has said, ready for a scan.
///
/// Read straight from `user.json` rather than from a loaded [`UserData`],
/// because the scan runs before anything else is loaded and needs no more than
/// this. A file that cannot be read gives an empty list: a merge nobody can
/// read is a merge that has not been stated, and refusing to scan over it would
/// make one unreadable line cost the whole library.
pub fn stated(dir: &std::path::Path) -> Vec<aede_core::model::identity::Chosen> {
    let path = user::user_path(dir);
    let Ok(Some(data)) = user::load(&path) else {
        return Vec::new();
    };
    chosen(&data)
}

/// The same list, from data already in hand.
fn chosen(data: &UserData) -> Vec<aede_core::model::identity::Chosen> {
    data.same_artist
        .iter()
        .map(|m| aede_core::model::identity::Chosen {
            spelling: m.spelling.clone(),
            filed_as: m.filed_as.clone(),
        })
        .collect()
}

pub fn merge(args: &Args) -> Res {
    let dir = data_dir(args);
    let path = user::user_path(&dir);
    let mut data = user::load(&path)?.unwrap_or_default();

    if args.has("list") {
        // The catalog is wanted but not required: a statement can be read back
        // before anything has ever been scanned, and refusing to show one for
        // want of a shelf would hide exactly the statements that have not been
        // applied yet.
        return listed(args, &data, load(args).ok());
    }
    if args.has("forget") {
        return forget(args, &mut data, &path);
    }

    let catalog = load(args)?;
    let [spelling, filed_as] = named(args)?;
    state(&catalog, &mut data, &path, &spelling, &filed_as)
}

/// The two names a merge is about, in the order they were typed.
///
/// **The order is the statement**: the first gives way to the second, which is
/// the way it reads aloud and the way `--list` shows it back. Two positionals
/// exactly, because "merge these" with one name is not a statement and with
/// three it is two statements the command would have to guess how to pair.
fn named(args: &Args) -> Result<[String; 2], Box<dyn std::error::Error>> {
    match args.positionals.as_slice() {
        [one, other] => Ok([one.clone(), other.clone()]),
        _ => Err("name both spellings, the one that gives way first: \
                  aede merge \"O. Osbourne\" \"Ozzy Osbourne\""
            .into()),
    }
}

/// `aede merge <spelling> <filed as>`: record that the two are one artist.
fn state(
    catalog: &Catalog,
    data: &mut UserData,
    path: &std::path::Path,
    spelling: &str,
    filed_as: &str,
) -> Res {
    let (from, to) = (text::normalize(spelling), text::normalize(filed_as));
    if from.is_empty() || to.is_empty() {
        return Err("a spelling with no letters in it names nobody".into());
    }
    if from == to {
        return Err(format!(
            "\"{spelling}\" and \"{filed_as}\" are already one name here — \
             the shelf matches on {from}"
        )
        .into());
    }

    // Both sides are looked up, and neither has to exist. A spelling the shelf
    // has never held is not a mistake: it is what a merge stated *before* the
    // folder holding it is scanned looks like, and refusing it would make the
    // order of two commands matter for no reason. What is worth saying is which
    // of the two the shelf does not know, so a typo is visible at once.
    let known = |key: &str| catalog.artists.iter().find(|a| a.key == key);
    let (left, right) = (known(&from), known(&to));

    // The one refusal. Two identifiers are two people, said so by the only
    // authority on the question, and a statement to the contrary belongs on
    // MusicBrainz rather than in a file on this disk.
    if let (Some(one), Some(other)) = (left, right)
        && let (Some(a), Some(b)) = (&one.mbid, &other.mbid)
        && a != b
    {
        return Err(format!(
            "MusicBrainz says these are two people, so they were not merged:\n\
             \t{} · musicbrainz {a}\n\
             \t{} · musicbrainz {b}\n\
             If that is wrong, it is wrong at the source — correcting it there \
             fixes it for everybody, and a later fetch brings the correction back.",
            one.name, other.name
        )
        .into());
    }

    let existing = data
        .same_artist
        .iter_mut()
        .find(|m| m.owner == user::LOCAL_USER && m.spelling == from);
    match existing {
        Some(already) if already.filed_as == to => {
            println!("{}", ui::section("Merge"));
            println!("  {}", ui::dim(&format!("{from} → {to} was already said")));
            return Ok(());
        }
        // One spelling gives way to one artist. Saying it again with another
        // destination is a correction, not a second statement, and keeping
        // both would leave the shelf to arbitrate between them.
        Some(already) => {
            let was = already.filed_as.clone();
            already.filed_as = to.clone();
            already.created_at = aede_core::clock::now_seconds();
            user::save(data, path)?;
            println!("{}", ui::section("Merge"));
            println!("  {} {from} → {to}", ui::green("→"));
            println!("  {}", ui::dim(&format!("it used to be filed under {was}")));
            println!("  {}", ui::dim(RESCAN));
            return Ok(());
        }
        None => {}
    }

    data.same_artist.push(SameArtist {
        owner: user::LOCAL_USER.to_string(),
        spelling: from.clone(),
        filed_as: to.clone(),
        created_at: aede_core::clock::now_seconds(),
    });
    user::save(data, path)?;

    println!("{}", ui::section("Merge"));
    println!("  {} {from} → {to}", ui::green("→"));
    for (key, found, typed) in [(&from, left, spelling), (&to, right, filed_as)] {
        if found.is_none() {
            println!(
                "  {}",
                ui::yellow(&format!(
                    "no artist called {key} on the shelf — \"{typed}\" may be a typo, \
                     or a folder that has not been scanned yet"
                ))
            );
        }
    }
    println!("  {}", ui::dim(NOTHING_TOUCHED));
    println!("  {}", ui::dim(RESCAN));
    Ok(())
}

/// What every path in this command has to say once, and says in one place.
const NOTHING_TOUCHED: &str =
    "nothing in your files changed: this is how the shelf is read, not what it holds";
const RESCAN: &str = "run aede scan to rebuild the shelf around it";

/// `aede merge --list`: the statements on file, so they can be undone.
///
/// Each row says whether it is **in effect**. A merge is applied when the graph
/// is built, so one stated after the last scan is a statement the shelf has not
/// heard yet — and a listing that showed it as done would be a listing nobody
/// could trust. The test for it is exact and costs nothing: if the catalog still
/// holds an artist under the spelling that was to give way, the scan has not
/// happened.
fn listed(args: &Args, data: &UserData, catalog: Option<Catalog>) -> Res {
    println!("{}", ui::section("Merged artists"));
    if data.same_artist.is_empty() {
        println!("  {}", ui::dim("nobody has been merged"));
        println!(
            "  {}",
            ui::dim("aede doctor names the pairs worth looking at")
        );
        return Ok(());
    }

    // The words typed are matched against **both** halves, because a person
    // hunting for a statement remembers the artist, not which side of the arrow
    // they wrote them on. The name is never swallowed: `aede merge --list ozzy`
    // said nothing about the word in the fourth command that did it.
    let wanted: Vec<String> = args
        .positionals
        .iter()
        .map(|n| text::normalize(n))
        .filter(|n| !n.is_empty())
        .collect();
    let rows: Vec<&SameArtist> = data
        .same_artist
        .iter()
        .filter(|m| super::fetch::reaches(&wanted, &[&m.spelling, &m.filed_as]))
        .collect();
    if rows.is_empty() {
        println!(
            "  {}",
            ui::dim(&format!(
                "no merge matches {} — {} in all",
                wanted.join(", "),
                ui::plural(data.same_artist.len(), "statement")
            ))
        );
        return Ok(());
    }

    let waiting = |row: &SameArtist| {
        catalog
            .as_ref()
            .is_some_and(|c| c.artists.iter().any(|a| a.key == row.spelling))
    };
    let mut table = Table::new(&["Spelling", "Filed as", "Said", "State"]).align(2, Align::Right);
    for row in &rows {
        table.push(vec![
            row.spelling.clone(),
            row.filed_as.clone(),
            ui::since(row.created_at),
            match waiting(row) {
                true => "waiting for a scan".to_string(),
                false => "in effect".to_string(),
            },
        ]);
    }
    println!("{}", table.render());
    if rows.iter().any(|row| waiting(row)) {
        println!("  {}", ui::yellow(RESCAN));
    }
    println!(
        "  {}",
        ui::dim("aede merge --forget \"<spelling>\" takes one back")
    );
    Ok(())
}

/// `aede merge --forget <spelling>`: take a statement back.
///
/// Keyed on the spelling that gives way, which is the half a person can say
/// unambiguously: several spellings may be filed under one artist, so naming
/// the destination would be naming a group rather than a statement.
fn forget(args: &Args, data: &mut UserData, path: &std::path::Path) -> Res {
    let wanted: Vec<String> = args
        .positionals
        .iter()
        .map(|n| text::normalize(n))
        .filter(|n| !n.is_empty())
        .collect();
    if wanted.is_empty() {
        return Err("name the spelling to unmerge: aede merge --forget \"O. Osbourne\"".into());
    }

    let matched = |m: &SameArtist| wanted.iter().any(|w| m.spelling.contains(w.as_str()));
    let taken: Vec<String> = data
        .same_artist
        .iter()
        .filter(|m| matched(m))
        .map(|m| format!("{} → {}", m.spelling, m.filed_as))
        .collect();

    println!("{}", ui::section("Merged artists"));
    if taken.is_empty() {
        println!("  {}", ui::dim("no merge answers to that"));
        return Ok(());
    }
    data.same_artist.retain(|m| !matched(m));
    user::save(data, path)?;
    for row in &taken {
        println!("  {} {row}", ui::green("→"));
    }
    println!("  {}", ui::dim("taken back"));
    println!("  {}", ui::dim(RESCAN));
    Ok(())
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
