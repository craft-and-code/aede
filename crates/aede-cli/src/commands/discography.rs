//! `fetch --discography`, and the report it feeds: what is missing from the shelf.
//!
//! # Why this is a fact about the artist
//!
//! A release key is `artist|title|folder`. **An album you do not own has no
//! folder**, so a record missing from the library cannot be an entity of this
//! layer at all — there is nothing to key it on. It is stored instead as a fact
//! about the artist: *MusicBrainz credits fourteen albums to this name*, with
//! their identifiers, titles and dates.
//!
//! Which of them you are missing is then **derived on read**, by comparing that
//! list against the catalog. This is the same rule the layer already follows
//! for agreement with a tag, and for the same reason: a stored "missing" goes
//! stale the moment the record is bought, and the catalog would be left holding
//! a claim it had stopped being able to justify. Derived, it corrects itself.
//!
//! # Why it is asked for rather than assumed
//!
//! It is one more request per artist — more for the prolific, who need a second
//! page — on top of a run that already costs one per artist and one per album.
//! Nobody needs a wish list in order to file their music.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::model::Catalog;
use aede_core::sources::{self, ArtistFacts, Facts, KnownRelease, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, musicbrainz, text};

use crate::ui;

use super::Res;
use super::fetch::{Ask, Refusal, ask_with_backoff};

/// An artist to browse, and the identifier to browse by.
struct Target {
    entity: EntityRef,
    name: String,
    mbid: String,
}

/// How many pages one artist may cost, at most.
///
/// A hundred release groups per page, so this covers an artist with a thousand
/// of them — beyond any real discography, and the point is not the number but
/// that the loop has an end. A miscounted total from the service must not turn
/// into a request per second for ever.
const MAX_PAGES: usize = 10;

/// The pass, over artists MusicBrainz has already identified.
pub fn run(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    again: bool,
) -> Res {
    let targets = targets(catalog, held, again);
    println!("{}", ui::section("Discography"));
    if targets.is_empty() {
        println!(
            "  {}",
            ui::dim(
                "nothing to browse: run fetch first, and note that only artists \
                 with an album of their own here are browsed"
            )
        );
        return Ok(());
    }

    // At least one request each, and a second page only for the prolific — so
    // the estimate is a floor rather than a promise, and says so.
    let total_ms = targets.len() as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {}, at least one request each, about {} — more for the prolific",
        ui::plural(targets.len(), "artist"),
        ui::long_duration(total_ms)
    );

    let (mut stored, mut empty, mut failed) = (0usize, 0usize, 0usize);
    for (done, target) in targets.iter().enumerate() {
        print!("\r  browsing: {}/{}", done + 1, targets.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());

        match browse(transport, backoff, &target.mbid) {
            Ok(known) => {
                match known.is_empty() {
                    true => empty += 1,
                    false => stored += 1,
                }
                store(held, target, known);
                sources::save(held, path)?;
            }
            Err(why) => {
                failed += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.name);
            }
        }
    }
    println!();

    println!(
        "{} {stored} stored, {empty} with nothing credited, {failed} failed",
        ui::green("→")
    );
    println!(
        "  {}",
        ui::dim("aede missing lists the records this catalog does not hold")
    );
    Ok(())
}

/// Every page of one artist's discography, in order.
fn browse(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    mbid: &str,
) -> Result<Vec<KnownRelease>, Refusal> {
    let mut all: Vec<KnownRelease> = Vec::new();
    for page in 0..MAX_PAGES {
        let url = musicbrainz::discography_url(mbid, page * musicbrainz::BROWSE_LIMIT);
        let answer = ask_with_backoff(transport, &url, backoff)?;
        let (rows, total) = musicbrainz::discography(&answer);
        // A page that came back empty ends the walk whatever the total says:
        // asking again for the same nothing is how a miscount becomes a loop.
        let arrived = rows.len();
        all.extend(rows);
        if arrived == 0 || all.len() >= total {
            break;
        }
    }
    Ok(all)
}

/// Files the discography **into the artist's existing MusicBrainz record**.
///
/// Read, add the list, write back — rather than a record of its own. The layer
/// is keyed on (entity, source), so a second record from the same source about
/// the same artist is not a second opinion but a lost one: `set` would replace
/// the first, and the area, the dates and the links would vanish.
fn store(held: &mut sources::Sources, target: &Target, known: Vec<KnownRelease>) {
    let mut facts = match held.get(&target.entity, sources::MUSICBRAINZ) {
        Some(SourceRecord {
            facts: Facts::Artist(existing),
            ..
        }) => existing.clone(),
        _ => ArtistFacts::default(),
    };
    facts.discography = known;
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some(target.mbid.clone()),
        fetched_at: clock::now_seconds(),
        // Browsed by identifier: nothing here was matched by name.
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(facts),
    });
}

/// Artists worth browsing: identified, and with a shelf of their own.
///
/// Two conditions, and the second is the one that matters. The catalog holds an
/// artist for every credit it reads, so a guest on one track of a compilation is
/// an artist of this library in exactly the way a musician whose records fill a
/// folder is not. Browsing them costs a request a second for a discography
/// [`absent`] will never report — the two must agree on who has a shelf, or the
/// pass spends minutes fetching answers nothing can use.
fn targets(catalog: &Catalog, held: &sources::Sources, again: bool) -> Vec<Target> {
    let mut targets = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        let Facts::Artist(artist) = &record.facts else {
            continue;
        };
        let Some(mbid) = record.source_id.clone() else {
            continue;
        };
        if !again && !artist.discography.is_empty() {
            continue;
        }
        let entity = record.entity();
        if !has_shelf(catalog, &entity) {
            continue;
        }
        targets.push(Target {
            entity,
            name: record.key.clone(),
            mbid,
        });
    }
    targets
}

/// How many artists a `--discography` pass would browse, if it ran now.
pub fn waiting(catalog: &Catalog, held: &sources::Sources) -> usize {
    targets(catalog, held, false).len()
}

/// `true` when this artist is the album artist of something in the library.
///
/// The one question both halves of this module have to answer the same way:
/// who has a shelf here. Being in the catalog is not it — a name credited on
/// one track of a compilation is in the catalog and has no place of its own.
fn has_shelf(catalog: &Catalog, entity: &EntityRef) -> bool {
    let Some(id) = entity.resolve(catalog) else {
        return false;
    };
    catalog
        .releases
        .iter()
        .any(|release| release.album_artist_id == Some(id))
}

// --------------------------------------------------------------------------
// The report
// --------------------------------------------------------------------------

/// One record credited to an artist that this catalog does not hold.
pub struct Absent<'a> {
    /// The artist, as the catalog spells them.
    pub artist: String,
    /// The record MusicBrainz credits to them.
    pub known: &'a KnownRelease,
}

/// What is credited to your artists and not on your shelf.
///
/// **Derived, never stored.** The comparison happens here, at the moment of
/// reading, so the day an album is added to the library it stops being listed —
/// with nothing to update and nothing to go stale.
///
/// Two ways of recognising a record you already have, in this order:
///
/// 1. the **release-group identifier**, when your tags carry one. Exact, and
///    the only comparison that cannot be wrong.
/// 2. the **normalised title**, otherwise. `text::normalize` is the same
///    function that decides two spellings are one name everywhere else in this
///    program, so a shelf that says "Kind Of Blue" is not told it is missing
///    "Kind of Blue".
///
/// Only studio albums are reported — see
/// [`KnownRelease::is_studio_album`] for why a complete discography would be a
/// true and useless answer.
pub fn absent<'a>(
    catalog: &Catalog,
    held: &'a sources::Sources,
    aside: &[aede_core::user::SetAside],
) -> Vec<Absent<'a>> {
    let mut out: Vec<Absent<'a>> = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        let Facts::Artist(facts) = &record.facts else {
            continue;
        };
        if facts.discography.is_empty() {
            continue;
        }
        let entity = record.entity();
        // An artist the catalog cannot place has no shelf to be missing from.
        let Some(artist_id) = entity.resolve(catalog) else {
            continue;
        };
        let Some(artist) = catalog.artist(artist_id) else {
            continue;
        };

        // What is on the shelf, in both spellings the comparison uses.
        let mut ids: Vec<&str> = Vec::new();
        let mut titles: Vec<String> = Vec::new();
        for release in &catalog.releases {
            if release.album_artist_id != Some(artist_id) {
                continue;
            }
            if let Some(group) = release.release_group_mbid.as_deref() {
                ids.push(group);
            }
            titles.push(text::normalize(&release.title));
        }

        // **No album of their own, no shelf to have gaps in.** The catalog
        // holds an artist for every credit it reads — a guest on one track, a
        // composer, one name on a compilation — and being in the catalog is
        // not the same as having a place in the library. The first version
        // missed that distinction and answered a question nobody asked: one
        // Rolling Stones track on a compilation produced their entire studio
        // discography as "missing", which is true and worthless, and it did
        // so for every passing credit at once until the report was mostly
        // that. What this reports is an *incomplete* discography, which means
        // one that was started.
        //
        // The same rule decides who is browsed at all — see [`has_shelf`].
        if titles.is_empty() {
            continue;
        }

        for known in &facts.discography {
            if !known.is_studio_album() {
                continue;
            }
            // Set aside by hand. Filtered here rather than by dropping it from
            // the layer, because the two are different claims and this program
            // keeps them apart everywhere else: MusicBrainz still says this is
            // an album, and the user still says it is not one they want listed.
            // Deleting the record would lose the first to record the second.
            if aside.iter().any(|a| a.release_group == known.mbid) {
                continue;
            }
            let have = ids.iter().any(|id| *id == known.mbid)
                || titles.contains(&text::normalize(&known.title));
            if !have {
                out.push(Absent {
                    artist: artist.name.clone(),
                    known,
                });
            }
        }
    }
    // Oldest first within an artist, so a discography reads as one.
    out.sort_by(|a, b| {
        a.artist
            .cmp(&b.artist)
            .then_with(|| a.known.first_released.cmp(&b.known.first_released))
    });
    out
}

/// The `missing` command: the records your artists made and you do not have.
pub fn missing(args: &crate::args::Args) -> Res {
    let catalog = super::load(args)?;
    let held = super::sources_held(args)?;
    let user_path = aede_core::user::user_path(&super::data_dir(args));
    let mut user = aede_core::user::load(&user_path)?.unwrap_or_default();

    if args.has("forget") {
        return set_aside(args, &catalog, &held, &mut user, &user_path);
    }
    if args.has("list") {
        return listed(&user);
    }

    let wanted: Vec<String> = args
        .positionals
        .iter()
        .map(|name| text::normalize(name))
        .filter(|name| !name.is_empty())
        .collect();

    let browsed = held
        .records
        .iter()
        .any(|r| matches!(&r.facts, Facts::Artist(a) if !a.discography.is_empty()));
    if !browsed {
        println!("{}", ui::section("Missing"));
        // Nothing browsed is not an empty shelf, and a reader who cannot tell
        // which will conclude the wrong one — the same distinction `sources`
        // draws when nothing has been fetched at all.
        println!(
            "  {}",
            ui::dim(
                "no discography has been fetched, so nothing can be missing yet: \
                 aede fetch --discography asks for one"
            )
        );
        return Ok(());
    }

    let all = absent(&catalog, &held, &user.set_aside);
    let rows: Vec<&Absent> = all.iter().filter(|a| matches(a, &wanted)).collect();

    println!("{}", ui::section("Missing"));
    if rows.is_empty() {
        println!(
            "  {}",
            ui::dim(match wanted.is_empty() {
                true => "every studio album MusicBrainz credits to your artists is here",
                false => "nothing matching that is missing",
            })
        );
        aside_note(&user);
        return Ok(());
    }

    let mut table = crate::ui::Table::new(&["Artist", "Album", "Year"]).limit(1, 46);
    for row in &rows {
        table.push(vec![
            row.artist.clone(),
            row.known.title.clone(),
            row.known.year().unwrap_or("").to_string(),
        ]);
    }
    print!("{}", table.render());
    println!("  {}", ui::plural(rows.len(), "studio album"));
    // The filter is a decision, and a reader who is not told about it will read
    // the list as everything MusicBrainz knows.
    println!(
        "  {}",
        ui::dim("singles, live records and compilations are left out")
    );
    aside_note(&user);
    Ok(())
}

/// `true` when a row answers to one of the words the reader typed.
fn matches(row: &Absent, wanted: &[String]) -> bool {
    let artist = text::normalize(&row.artist);
    let title = text::normalize(&row.known.title);
    wanted.is_empty()
        || wanted
            .iter()
            .any(|w| artist.contains(w.as_str()) || title.contains(w.as_str()))
}

/// Says how many records are set aside, whenever any are.
///
/// **A filter the reader cannot see is a trap.** Everything else this command
/// leaves out is stated under the table — singles, live records, compilations —
/// and a decision the reader took themselves deserves the same treatment: the
/// day they wonder why an album is not listed, the answer is on screen.
fn aside_note(user: &aede_core::user::UserData) {
    if user.set_aside.is_empty() {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} set aside — aede missing --list shows them",
            ui::plural(user.set_aside.len(), "record")
        ))
    );
}

/// `aede missing --forget <text>`: take a record off the report, or put it back.
fn set_aside(
    args: &crate::args::Args,
    catalog: &Catalog,
    held: &sources::Sources,
    user: &mut aede_core::user::UserData,
    path: &std::path::Path,
) -> Res {
    let wanted: Vec<String> = args
        .positionals
        .iter()
        .map(|name| text::normalize(name))
        .filter(|name| !name.is_empty())
        .collect();
    if wanted.is_empty() {
        return Err("name the album to set aside: aede missing --forget \"<title>\"".into());
    }

    // Putting one back is answered from the list of decisions, not from the
    // report: a record that has been set aside is by definition absent from the
    // report, so looking there would find nothing and say so.
    if args.has("remove") {
        let before = user.set_aside.len();
        let put_back: Vec<String> = user
            .set_aside
            .iter()
            .filter(|a| {
                let title = text::normalize(&a.title);
                wanted.iter().any(|w| title.contains(w.as_str()))
            })
            .map(|a| a.title.clone())
            .collect();
        user.set_aside.retain(|a| {
            let title = text::normalize(&a.title);
            !wanted.iter().any(|w| title.contains(w.as_str()))
        });
        println!("{}", ui::section("Missing"));
        if user.set_aside.len() == before {
            println!("  {}", ui::dim("nothing set aside answers to that"));
            return Ok(());
        }
        aede_core::user::save(user, path)?;
        for title in &put_back {
            println!("  {} {title}", ui::green("→"));
        }
        println!("  {}", ui::dim("back on the list"));
        return Ok(());
    }

    let all = absent(catalog, held, &user.set_aside);
    let found: Vec<&Absent> = all.iter().filter(|a| matches(a, &wanted)).collect();
    match found.len() {
        0 => Err("nothing missing answers to that".into()),
        // Several equally good answers are refused rather than arbitrated —
        // the rule `best_match` and `find_releases` both follow. Setting aside
        // the wrong record is a decision nobody would see being taken.
        n if n > 1 => {
            let names: Vec<String> = found
                .iter()
                .map(|a| format!("{} — {}", a.artist, a.known.title))
                .collect();
            Err(format!(
                "several records answer to that, so none was set aside:\n  {}",
                names.join("\n  ")
            )
            .into())
        }
        _ => {
            let one = found[0];
            user.set_aside.push(aede_core::user::SetAside {
                owner: aede_core::user::LOCAL_USER.to_string(),
                release_group: one.known.mbid.clone(),
                title: one.known.title.clone(),
                created_at: aede_core::clock::now_seconds(),
            });
            aede_core::user::save(user, path)?;
            println!("{}", ui::section("Missing"));
            println!("  {} {} — {}", ui::green("→"), one.artist, one.known.title);
            println!(
                "  {}",
                ui::dim(
                    "set aside; --forget --remove puts it back. What MusicBrainz \
                     says is untouched — only what you are shown changed"
                )
            );
            Ok(())
        }
    }
}

/// `aede missing --list`: the decisions taken, so they can be undone.
fn listed(user: &aede_core::user::UserData) -> Res {
    println!("{}", ui::section("Set aside"));
    if user.set_aside.is_empty() {
        println!("  {}", ui::dim("nothing has been set aside"));
        return Ok(());
    }
    let mut table = crate::ui::Table::new(&["Album", "Identifier"]).limit(0, 46);
    for aside in &user.set_aside {
        table.push(vec![
            aside.title.clone(),
            format!(
                "https://musicbrainz.org/release-group/{}",
                aside.release_group
            ),
        ]);
    }
    print!("{}", table.render());
    println!(
        "  {}",
        ui::dim(
            "aede missing --forget --remove \"<title>\" puts one back; the address \
             is where its type is corrected for everybody"
        )
    );
    Ok(())
}

#[cfg(test)]
#[path = "discography_tests.rs"]
mod tests;
