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
    asked: &super::fetch::Asked,
) -> Res {
    let (wanted, again) = (asked.names, asked.again);
    let targets = targets(catalog, held, wanted, again);
    println!("{}", ui::section("Discography"));
    if targets.is_empty() {
        if !wanted.is_empty() {
            println!(
                "  {}",
                ui::dim(&super::fetch::nothing_named(
                    wanted,
                    self::targets(catalog, held, wanted, true).len()
                ))
            );
            return Ok(());
        }
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
fn targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    again: bool,
) -> Vec<Target> {
    let mut targets = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        if !super::fetch::reaches(wanted, &[record.key.as_str()]) {
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
    targets(catalog, held, &[], false).len()
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
    /// Why this row is not normally on the report, and `None` when it is.
    ///
    /// Two quite different reasons, in one field because they answer one
    /// question — *why am I not being shown this* — and because a record can
    /// be both: a live album the reader also set aside. The words for the
    /// first are MusicBrainz's own; see [`KnownRelease::stated_type`].
    pub held_back: Option<String>,
    /// `true` when the reader set this one aside and asked to see it anyway.
    pub set_aside: bool,
    /// Which artist, for a caller asking about one rather than about all.
    pub artist_id: aede_core::model::Id,
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
/// Two things are held back, and `everything` lifts both.
///
/// Only studio albums are reported — see [`KnownRelease::is_studio_album`] for
/// why a complete discography would be a true and useless answer — and only
/// records the reader has not set aside. Those are the report's two filters,
/// and `--all` is one word for "hold nothing back", so it lifts both and every
/// row that came back this way carries the reason it would not normally be
/// here. **A filter is only honest when the reader can turn it off**: saying
/// what is left out and offering no way to see it is a locked door with a
/// label on it.
pub fn absent<'a>(
    catalog: &Catalog,
    held: &'a sources::Sources,
    aside: &[aede_core::user::SetAside],
    everything: bool,
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
            // Set aside by hand. Filtered here rather than by dropping it from
            // the layer, because the two are different claims and this program
            // keeps them apart everywhere else: MusicBrainz still says this is
            // an album, and the user still says it is not one they want listed.
            // Deleting the record would lose the first to record the second.
            let put_aside = aside.iter().any(|a| a.release_group == known.mbid);

            // Both reasons, in the order a reader meets them: what it is, then
            // what they decided about it. A record can be both, and naming only
            // one would answer half the question.
            let mut why: Vec<String> = Vec::new();
            if !known.is_studio_album() {
                why.push(known.stated_type());
            }
            if put_aside {
                why.push("set aside".to_string());
            }
            let held_back = (!why.is_empty()).then(|| why.join(", "));
            if held_back.is_some() && !everything {
                continue;
            }

            let have = ids.iter().any(|id| *id == known.mbid)
                || titles.contains(&text::normalize(&known.title));
            if !have {
                out.push(Absent {
                    artist: artist.name.clone(),
                    held_back,
                    set_aside: put_aside,
                    artist_id,
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

/// How many records one artist has that this shelf does not.
///
/// For a page to say so at the moment somebody is looking at that artist —
/// which is the moment they wonder. `aede missing` was in the help, in the
/// README and in the manual, and still could not be found, because it was
/// named nowhere near the question it answers. The same fault `aede extract`
/// had, and the same fix.
///
/// Derived like everything else here: nothing is stored, so the number falls
/// the day an album is added.
pub fn absent_for(
    catalog: &Catalog,
    held: &sources::Sources,
    aside: &[aede_core::user::SetAside],
    artist_id: aede_core::model::Id,
) -> usize {
    absent(catalog, held, aside, false)
        .iter()
        .filter(|record| record.artist_id == artist_id)
        .count()
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
    let wanted = super::fetch::names_given(args);

    if args.has("list") {
        return listed(&catalog, &held, &user, &wanted);
    }

    // "Hold nothing back", the same word as on every other listing, and here
    // it lifts everything this report holds back at once: the row limit, the
    // studio-album filter, and what the reader set aside. Three things, one
    // intent — *show me all of what the fetch brought back* — and a separate
    // option for each would be three words to learn for one question.
    let everything = args.has("all");
    let window = args.window(50)?;

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

    let all = absent(&catalog, &held, &user.set_aside, everything);
    let rows: Vec<&Absent> = all.iter().filter(|a| matches(a, &wanted)).collect();
    // What the ordinary run is leaving out, counted over the rows this run is
    // about rather than over the whole layer: a reader who typed a name is owed
    // the number for that name. Derived from the same walk that produces the
    // table, so the note and the list cannot disagree.
    let held_back = match everything {
        true => Vec::new(),
        false => absent(&catalog, &held, &user.set_aside, true)
            .into_iter()
            .filter(|a| a.held_back.is_some() && matches(a, &wanted))
            .collect(),
    };

    println!("{}", ui::section("Missing"));
    if rows.is_empty() {
        println!(
            "  {}",
            ui::dim(match (wanted.is_empty(), everything) {
                (true, true) => "every record MusicBrainz credits to your artists is here",
                (true, false) => "every studio album MusicBrainz credits to your artists is here",
                (false, _) => "nothing matching that is missing",
            })
        );
        left_out(&held_back, everything);
        return Ok(());
    }

    // The last column exists only when there is something to put in it: a blank
    // column on every ordinary run is a question the reader has to ask and
    // answer for themselves.
    let mut headings = vec!["Artist", "Album", "Year"];
    if everything {
        headings.push("Left out");
    }
    let mut table = crate::ui::Table::new(&headings).limit(1, 46);
    let total = rows.len();
    for row in rows.iter().skip(window.offset).take(window.limit) {
        let mut cells = vec![
            row.artist.clone(),
            row.known.title.clone(),
            row.known.year().unwrap_or("").to_string(),
        ];
        if everything {
            cells.push(row.held_back.clone().unwrap_or_default());
        }
        table.push(cells);
    }
    print!("{}", table.render());
    println!(
        "  {}",
        ui::plural(
            total,
            match everything {
                true => "record",
                false => "studio album",
            }
        )
    );
    super::announce_window(window, total, "record");
    left_out(&held_back, everything);
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

/// Says what this run held back, how much of it, and how to see it.
///
/// **A filter the reader cannot see is a trap, and one they cannot turn off is
/// only half an answer.** The count is of the rows *this* run held back, so it
/// falls with the name typed and cannot drift from the table above it; the way
/// to see them is named in the same breath, because a report that says "some
/// records are not shown" and stops there has told the reader they are missing
/// something and left them no move.
///
/// The two reasons are named separately even though `--all` lifts both: one is
/// this program's editorial decision about what a wish list is for, the other
/// is a decision the reader took themselves, and only the second has a listing
/// of its own to undo it from.
fn left_out(rows: &[Absent], everything: bool) {
    if everything {
        println!(
            "  {}",
            ui::dim(
                "nothing held back: everything the discography pass brought back, \
                 and the last column is what MusicBrainz calls each one"
            )
        );
        return;
    }
    if rows.is_empty() {
        // Still stated with nothing to state it about: it is the shape of the
        // report rather than an incident, and a reader who is not told will
        // read the list as everything MusicBrainz knows.
        println!(
            "  {}",
            ui::dim("singles, live records and compilations are left out")
        );
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} left out — --all lists them here, with the reason for each: \
             singles, live records, compilations, demos, and anything you set aside",
            ui::plural(rows.len(), "record")
        ))
    );
    let aside = rows.iter().filter(|row| row.set_aside).count();
    if aside > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} among them you set aside yourself — --list shows those on their own",
                ui::plural(aside, "record")
            ))
        );
    }
}

/// Which artist a set-aside record belongs to, as the stored discographies say.
///
/// Not kept on the record itself: a set-aside is keyed on the release group,
/// which is globally unique, so no artist is needed to tell two apart. But a
/// listing that shows only titles is a listing nobody can act on, and the
/// answer is already in the layer — derived at the moment of reading, like
/// everything else here, so it cannot go stale.
///
/// Empty when the discography that named it has since been forgotten: the row
/// is still shown, because a decision the reader took must not disappear
/// because a fetch was undone.
fn whose(catalog: &Catalog, held: &sources::Sources, release_group: &str) -> String {
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        let Facts::Artist(facts) = &record.facts else {
            continue;
        };
        if !facts.discography.iter().any(|k| k.mbid == release_group) {
            continue;
        }
        // The catalog's spelling when it holds the artist, the layer's key
        // otherwise — a name is better than nothing, and this row exists to
        // be recognised.
        return record
            .entity()
            .resolve(catalog)
            .and_then(|id| catalog.artist(id))
            .map(|artist| artist.name.clone())
            .unwrap_or_else(|| record.key.clone());
    }
    String::new()
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

    let all = absent(catalog, held, &user.set_aside, false);
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

/// The set-aside decisions a run is about, each with the artist it belongs to.
///
/// The names typed used to be **swallowed**: `aede missing "MIKA" --list`
/// listed every decision on file and said nothing about the word. The fourth
/// time a command in this program has quietly dropped its argument, and the
/// reason it keeps happening is that each listing wrote its own matching. This
/// one calls [`super::fetch::reaches`], which is where that decision lives.
///
/// Split out of [`listed`] so the narrowing can be tested without reading what
/// was printed: a filter is a claim about which rows survive, and that is the
/// part worth pinning down.
fn aside_rows<'a>(
    catalog: &Catalog,
    held: &sources::Sources,
    user: &'a aede_core::user::UserData,
    wanted: &[String],
) -> Vec<(&'a aede_core::user::SetAside, String)> {
    user.set_aside
        .iter()
        .map(|aside| (aside, whose(catalog, held, &aside.release_group)))
        .filter(|(aside, artist)| super::fetch::reaches(wanted, &[&aside.title, artist]))
        .collect()
}

/// `aede missing --list`: the decisions taken, so they can be undone.
fn listed(
    catalog: &Catalog,
    held: &sources::Sources,
    user: &aede_core::user::UserData,
    wanted: &[String],
) -> Res {
    println!("{}", ui::section("Set aside"));
    if user.set_aside.is_empty() {
        println!("  {}", ui::dim("nothing has been set aside"));
        return Ok(());
    }

    let rows = aside_rows(catalog, held, user, wanted);
    if rows.is_empty() {
        println!(
            "  {}",
            ui::dim(&format!(
                "nothing set aside matches {} — {} in all",
                wanted.join(", "),
                ui::plural(user.set_aside.len(), "record")
            ))
        );
        return Ok(());
    }

    // The artist was missing from this table, and "Sweet Dreams" on its own
    // tells a reader nothing about whose decision they are looking at. It is
    // not stored on the record — a set-aside is keyed on the release group,
    // which is globally unique — so it is resolved from the discographies
    // that named it, the way everything else here is derived rather than kept.
    let mut table = crate::ui::Table::new(&["Artist", "Album", "Identifier"]).limit(1, 40);
    for (aside, artist) in &rows {
        table.push(vec![
            artist.clone(),
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
