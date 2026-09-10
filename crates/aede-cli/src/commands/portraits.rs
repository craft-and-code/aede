//! `fetch --portraits`: a picture of the artist or the band.
//!
//! For the web interface this library exists to feed one day: a shelf of
//! albums with no face beside any of them is not wrong, but it is not what
//! anybody actually wants to look at. This pass fills that gap the same way
//! `--covers` fills the one beside it — by asking a source that already has
//! the picture, and never by generating or guessing one.
//!
//! # Two sources, tried in that order, and never both
//!
//! [`wikipedia::portrait_file`] first: the `P18` image claim on the Wikidata
//! entity MusicBrainz already links every artist to, the same entity
//! `fetch --summaries` reads for prose. A Commons image carries a stated
//! licence, which is the property that makes it worth trying first.
//!
//! [`fanarttv`] second, and only where Wikidata has nothing: user-submitted,
//! with no licence Aède can state, so it is the fallback rather than the
//! first question. It needs a free key — see [`fanarttv::no_key`] — and
//! where none is set this pass simply does not ask it, and says so once.
//!
//! Whichever answers is what gets stored. `Sources::set` replaces a record
//! whole, and the two services are kept as two source names —
//! [`wikipedia::PORTRAIT_SOURCE`] and [`fanarttv::SOURCE`] — for exactly the
//! reason stated there: so that one being asked again never silently erases
//! what the other already found.
//!
//! # What concert photos and line-ups are not part of this
//!
//! Deliberately out of scope. Those are not a fact about the artist that a
//! source states once and Aède reads back — they are a curated choice, and
//! the interface this feeds will let somebody add their own. Fetching them
//! automatically would be answering a question nobody asked this pass.
//!
//! # Where the picture is written
//!
//! Beside the music, when the artist's answer is unambiguous: every album
//! [`super::shared_folder`] can find for them shares one parent, which is
//! the same folder `aede playlist --artists` already writes a discography
//! playlist into. It rides along with the user's own backup exactly as
//! `cover.jpg` does today — nothing about `aede backup` has to know it
//! exists.
//!
//! Everywhere else — a discography spread across more than one watched root,
//! an artist known only through compilations, credited on tracks that share
//! no album folder at all — guessing a destination is not something this
//! program does anywhere else, and it does not start here. The picture goes
//! into a per-artist folder under `assets/` in Aède's own data directory
//! instead, named by the MusicBrainz identifier so two artists that happen
//! to share a name never collide.
//!
//! The record in `sources.json` carries only the address it was downloaded
//! from — see [`aede_core::sources::Picture`] — because the file on disk is
//! what a viewer actually looks at, and re-fetching costs one request, not a
//! reason to duplicate the bytes into the attributed layer too.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aede_core::coverart::{self, Kind};
use aede_core::model::{Catalog, Id};
use aede_core::sources::{self, Facts, Picture, SourceRecord};
use aede_core::store;
use aede_core::user::EntityRef;
use aede_core::{clock, fanarttv, wikipedia};

use crate::ui;

use super::Res;
use super::fetch::{Ask, Refusal, ask_bytes, ask_with_backoff, queue, worth_deferring};

/// The width asked of Commons for a downloaded portrait.
///
/// Large enough to look right as the face of an artist page, small enough
/// that a library of a few hundred names is a few hundred megabytes rather
/// than a few gigabytes — the same reasoning [`super::covers::DEFAULT_SIZE`]
/// is picked with, at a size suited to a portrait rather than a full sleeve.
const PORTRAIT_WIDTH: u32 = 800;

/// An artist to ask about, and where the picture would be written.
struct Target {
    entity: EntityRef,
    name: String,
    /// The Wikidata entity id, when it is still worth asking about: linked by
    /// MusicBrainz, and — unless `--full` — not already answered.
    ///
    /// Decided once, here, rather than re-read inside [`attempt`]: whether a
    /// source is worth trying is a fact about what `held` already says, and
    /// asking it twice — once to decide whether this artist is a target at
    /// all, once to decide whether to actually ask — is how the two would
    /// eventually disagree.
    wikidata_id: Option<String>,
    /// The MusicBrainz artist identifier, only when Fanart.tv is still worth
    /// asking: a key is set, and — unless `--full` — it has not already
    /// answered. `None` skips it exactly as an absent [`Target::wikidata_id`]
    /// skips Wikidata.
    fanarttv_mbid: Option<String>,
    /// The MusicBrainz artist identifier, kept on its own beside
    /// [`Target::fanarttv_mbid`] because it is also what names the artist's
    /// folder under `assets/`, whether or not Fanart.tv itself is asked.
    mbid: String,
    /// The folder the picture is written into — see the module doc for how
    /// it is chosen.
    destination: PathBuf,
}

/// What became of asking about one artist.
enum Outcome {
    /// A picture was found and is now on disk.
    Written {
        source: &'static str,
        url: String,
        /// `false` when a file was already there — written by a previous,
        /// interrupted run, or dropped in by hand — in which case nothing
        /// was downloaded twice, only recorded.
        new: bool,
    },
    /// Asked, and nothing was found — everywhere it was still worth asking.
    Nothing {
        wikidata_asked: bool,
        fanarttv_asked: bool,
    },
}

/// The pass.
pub fn run(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &Path,
    data_dir: &Path,
    fanarttv_key: Option<&str>,
    asked: &super::fetch::Asked,
) -> Res {
    let targets = targets(
        catalog,
        held,
        asked.names,
        asked.scope,
        data_dir,
        fanarttv_key,
        asked.again,
    );
    println!("{}", ui::section("Portraits"));
    if targets.is_empty() {
        let narrowed = super::fetch::narrowing(asked.names, asked.scope);
        match narrowed.is_empty() {
            true => println!("  {}", ui::dim("nothing to ask about")),
            false => println!(
                "  {}",
                ui::dim(&format!("nothing to ask about for {narrowed}"))
            ),
        }
        return Ok(());
    }

    let least = targets.len() as u64 * wikipedia::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {}, one to four requests each, {} to {}",
        ui::plural(targets.len(), "artist"),
        ui::long_duration(least),
        ui::long_duration(least * 4)
    );
    println!(
        "  {}",
        ui::dim(
            "wikidata is asked first; fanart.tv only where it has nothing, \
             and only for an artist already asked",
        )
    );
    if fanarttv_key.is_none() {
        println!("  {}", ui::dim(&fanarttv::no_key()));
    }
    println!(
        "  {}",
        ui::dim(
            "written as artist.jpg or artist.png beside the music when every \
             album shares a folder, in assets/ otherwise; an artist that \
             already has one is never touched",
        )
    );

    if super::fetch::asked_nothing(
        asked,
        &targets.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
    ) {
        return Ok(());
    }

    let (mut written, mut already, mut none, mut failed) = (0usize, 0usize, 0usize, 0usize);
    let mut pending = queue(&targets);
    let mut done = 0usize;
    let total = targets.len();
    while let Some((target, retried)) = pending.pop_front() {
        print!("\r  asking: {}/{}", done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        match attempt(transport, backoff, target, fanarttv_key) {
            Ok(outcome) => {
                store(held, target, &outcome);
                sources::save(held, path)?;
                match outcome {
                    Outcome::Written { new: true, .. } => written += 1,
                    Outcome::Written { new: false, .. } => already += 1,
                    Outcome::Nothing { .. } => none += 1,
                }
                done += 1;
            }
            Err(why) if worth_deferring(&why) && !retried => {
                pending.push_back((target, true));
                continue;
            }
            Err(why) => {
                failed += 1;
                done += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.name);
            }
        }
    }
    println!();

    println!(
        "{} {written} written, {none} with no portrait found, {failed} failed",
        ui::green("→")
    );
    if already > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} already had a picture on disk, placed by hand or by an \
                 interrupted run — nothing was downloaded twice",
                ui::plural(already, "artist")
            ))
        );
    }
    if written > 0 {
        println!(
            "  {}",
            ui::dim(
                "aede scan picks up the ones written beside the music — nothing \
                     registers a file, the scan finds it"
            )
        );
    }
    Ok(())
}

/// Asks about one artist, in order, and writes the picture if one turns up.
///
/// A single function rather than a state machine so that a retry — the
/// `Refusal` bubbling out of any request below, when it is worth deferring —
/// simply redoes the whole thing on the next pass through the queue, exactly
/// as `fetch --covers` retries its own first request. One extra ask on the
/// rare retried artist costs far less than the bookkeeping needed to resume
/// midway through.
fn attempt(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    target: &Target,
    fanarttv_key: Option<&str>,
) -> Result<Outcome, Refusal> {
    let wikidata_asked = target.wikidata_id.is_some();
    if let Some(id) = &target.wikidata_id {
        let doc = match ask_with_backoff(transport, &wikipedia::entity_data_url(id), backoff) {
            Ok(doc) => Some(doc),
            // A entity id MusicBrainz linked to should exist; if Wikidata
            // says otherwise, that is the same "asked, and there is
            // nothing" answer as a claim with no image in it.
            Err(Refusal::Missing) => None,
            Err(why) => return Err(why),
        };
        if let Some(file) = doc
            .as_ref()
            .and_then(|doc| wikipedia::portrait_file(doc, id))
        {
            let url = wikipedia::commons_file_url(&file, PORTRAIT_WIDTH);
            let bytes = ask_bytes(transport, &url, backoff)?;
            let new = write(target, &bytes)?;
            return Ok(Outcome::Written {
                source: wikipedia::PORTRAIT_SOURCE,
                url,
                new,
            });
        }
    }

    let fanarttv_asked = target.fanarttv_mbid.is_some();
    if let Some(mbid) = &target.fanarttv_mbid {
        // `fanarttv_mbid` is only ever set when [`targets`] already found a
        // key, so this is never reached with none — but the URL still needs
        // the key itself, not just the fact that one exists.
        let key = fanarttv_key.expect("a key, since fanarttv_mbid was set from one");
        let doc = match ask_with_backoff(transport, &fanarttv::lookup_url(mbid, key), backoff) {
            // The service answers `404` for an artist it holds nothing of,
            // which is a real answer, not a failure — the same distinction
            // `fetch --covers` reads out of the same status for the archive.
            Ok(doc) => Some(doc),
            Err(Refusal::Missing) => None,
            Err(why) => return Err(why),
        };
        if let Some(url) = doc.as_ref().and_then(fanarttv::portrait_url) {
            let bytes = ask_bytes(transport, &url, backoff)?;
            let new = write(target, &bytes)?;
            return Ok(Outcome::Written {
                source: fanarttv::SOURCE,
                url,
                new,
            });
        }
    }

    Ok(Outcome::Nothing {
        wikidata_asked,
        fanarttv_asked,
    })
}

/// Writes the picture into the target's folder, unless one is already there.
///
/// The general-purpose writer this program keeps for every image it puts
/// into somebody's library — see [`coverart::write_image`] — carrying both of
/// its guards: the bytes are sniffed, and nothing already there is ever
/// overwritten. `true` means this call is the one that wrote it.
fn write(target: &Target, bytes: &[u8]) -> Result<bool, Refusal> {
    match coverart::write_image(&target.destination, Kind::Artist, (0, 1), bytes) {
        Ok(coverart::Written::New(_)) => Ok(true),
        Ok(coverart::Written::Already(_)) => Ok(false),
        Err(why) => Err(Refusal::Failed(why)),
    }
}

/// Files what was found, including when that was nothing.
///
/// A record with no picture is not wasted, for the same reason `--summaries`
/// keeps one with an empty prose: "asked, and there is nothing" is what
/// keeps the next run from asking again. Only the sources actually asked get
/// a record — Fanart.tv is never marked "asked" for an artist Wikidata
/// already answered, because it never was.
fn store(held: &mut sources::Sources, target: &Target, outcome: &Outcome) {
    let mut file = |source: &str, portrait: Option<Picture>| {
        held.set(SourceRecord {
            key: target.entity.key.clone(),
            source: source.to_string(),
            source_id: Some(target.mbid.clone()),
            fetched_at: clock::now_seconds(),
            // Both sources are reached by identifier — the Wikidata entity
            // MusicBrainz links to, the artist's own MusicBrainz id — never
            // by matching a name, so nothing stored here is ever a guess.
            confidence: sources::Confidence::Identified,
            facts: Facts::Artist(sources::ArtistFacts {
                portrait,
                ..Default::default()
            }),
        });
    };
    match outcome {
        Outcome::Written { source, url, .. } => file(source, Some(Picture { url: url.clone() })),
        Outcome::Nothing {
            wikidata_asked,
            fanarttv_asked,
        } => {
            if *wikidata_asked {
                file(wikipedia::PORTRAIT_SOURCE, None);
            }
            if *fanarttv_asked {
                file(fanarttv::SOURCE, None);
            }
        }
    }
}

/// How many artists this pass would ask about, if it ran now.
///
/// The same function as the walk, counted rather than re-derived — see
/// [`super::summaries::waiting`] for why that discipline matters here too.
/// Takes the Fanart.tv key for the same reason `run` does: an artist only
/// Fanart.tv could still answer for is not "waiting" while no key is set.
pub fn waiting(
    catalog: &Catalog,
    held: &sources::Sources,
    data_dir: &Path,
    fanarttv_key: Option<&str>,
) -> usize {
    targets(
        catalog,
        held,
        &[],
        &super::fetch::EVERYTHING,
        data_dir,
        fanarttv_key,
        false,
    )
    .len()
}

/// Who to ask about: artists MusicBrainz answered, that have not already got
/// a picture and have not already been asked everywhere they could be.
fn targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    scope: &super::fetch::Scope,
    data_dir: &Path,
    fanarttv_key: Option<&str>,
    again: bool,
) -> Vec<Target> {
    let mut releases_by_artist: BTreeMap<Id, Vec<&aede_core::model::Release>> = BTreeMap::new();
    for release in &catalog.releases {
        if let Some(artist) = release.album_artist_id {
            releases_by_artist.entry(artist).or_default().push(release);
        }
    }
    let roots: Vec<&str> = catalog.roots.iter().map(String::as_str).collect();

    let mut targets = Vec::new();
    for record in &held.records {
        if record.source != sources::MUSICBRAINZ {
            continue;
        }
        if !super::fetch::reaches(wanted, &[record.key.as_str()]) {
            continue;
        }
        if !scope.has_artist(&record.key) {
            continue;
        }
        let Facts::Artist(artist) = &record.facts else {
            continue;
        };
        // An ordinary fetch always stores the identifier it matched, so this
        // is not expected to be empty — but a layer on disk is user-editable
        // text, and a row missing it is a row this pass cannot ask Fanart.tv
        // about and has no business guessing.
        let Some(mbid) = &record.source_id else {
            continue;
        };
        // No folder to write beside and no name to show for an artist the
        // catalog no longer holds — a row left over from a shelf that has
        // since been reorganised.
        let Some(artist_row) = catalog.artists.iter().find(|a| a.key == record.key) else {
            continue;
        };
        if !super::fetch::has_album(catalog, artist_row.id) {
            continue;
        }
        let entity = record.entity();
        let linked = artist.wikidata.as_deref().and_then(wikipedia::entity_id);

        if !again && has_portrait(held, &entity) {
            continue;
        }
        // `again` reruns a source that has already answered; without it, a
        // source is worth trying only when it has not been asked yet — and
        // not at all when there is nothing to ask it with, a missing
        // Fanart.tv key above all, which is never a reason to keep retrying.
        let wikidata_id =
            linked.filter(|_| again || held.get(&entity, wikipedia::PORTRAIT_SOURCE).is_none());
        let fanarttv_mbid = fanarttv_key
            .filter(|_| again || held.get(&entity, fanarttv::SOURCE).is_none())
            .map(|_| mbid.clone());
        if wikidata_id.is_none() && fanarttv_mbid.is_none() {
            continue;
        }

        let destination = releases_by_artist
            .get(&artist_row.id)
            .and_then(|releases| super::shared_folder(releases))
            .and_then(|folder| {
                // A watched root is not an artist folder, the same guard
                // `playlist --artists` makes for the same reason.
                let name = folder.to_string_lossy().to_string();
                (!roots.contains(&name.as_str())).then_some(folder)
            })
            .unwrap_or_else(|| store::assets_dir(data_dir).join("artists").join(mbid));

        targets.push(Target {
            entity,
            name: artist_row.name.clone(),
            wikidata_id,
            fanarttv_mbid,
            mbid: mbid.clone(),
            destination,
        });
    }
    targets
}

/// `true` when a picture — not just an answer — is already on record.
fn has_portrait(held: &sources::Sources, entity: &EntityRef) -> bool {
    [wikipedia::PORTRAIT_SOURCE, fanarttv::SOURCE]
        .iter()
        .any(|source| match held.get(entity, source) {
            Some(SourceRecord {
                facts: Facts::Artist(artist),
                ..
            }) => artist.portrait.is_some(),
            _ => false,
        })
}

#[cfg(test)]
#[path = "portraits_tests.rs"]
mod tests;
