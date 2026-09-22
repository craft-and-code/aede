//! `fetch --logos`: an artist's mark, not their face.
//!
//! For the same web interface `--portraits` exists to feed: a logo reads
//! differently from a photograph, and a shelf display benefits from having
//! both, the artist's own mark beside the picture of them. This pass fills
//! that gap the same way `--portraits` fills its own — by asking a source
//! that already has the image, and never by generating or guessing one.
//!
//! # One source, and no alternative to try first
//!
//! Unlike a portrait, which Wikidata may already hold under a stated
//! licence, no source Aède otherwise reads carries an artist's logo at all —
//! there is nothing to try before [`fanarttv`], so this pass asks it
//! directly, wherever a key is set.
//!
//! [`fanarttv::logo_url`] prefers `hdmusiclogo`; only where an artist has
//! none does it fall back to `musiclogo` — the two are the same submissions
//! at two resolutions, not two pools to choose the more liked from.
//!
//! The record is written under [`fanarttv::LOGO_SOURCE`], never
//! [`fanarttv::SOURCE`]: the same request also answers `--portraits`, and a
//! shared name would mean whichever pass ran second erased what the other
//! had found — see [`fanarttv::LOGO_SOURCE`] for the reasoning in full.
//!
//! # Where the logo is written
//!
//! The same folder a portrait would go into — see the module doc on
//! [`super::portraits`] for how it is chosen — distinguished by filename
//! rather than by a folder of its own: `logo.jpg`/`logo.png` beside
//! `artist.jpg`/`artist.png`, never colliding with it.
//!
//! The record in `sources.json` carries only the address it was downloaded
//! from — see [`aede_core::sources::Picture`] — for the same reason
//! `--portraits` keeps just the address: the file on disk is what a viewer
//! looks at, and re-fetching costs one request, not a reason to duplicate the
//! bytes into the attributed layer too.
//!
//! # Banners
//!
//! `--banners` rides the very same answer: [`fanarttv::banner_url`] is read
//! out of the response already fetched for the logo, never a second request
//! for one artist. Unlike the logo it is tracked by disk presence alone, the
//! choice `fetch --covers --images` makes for the pictures beside a cover —
//! see [`aede_core::coverart::exists_beside`] — rather than through
//! `sources.json`: a banner has no name or licence worth attributing, only
//! bytes, and the folder already says whether one is there. An artist that
//! already has a logo is still asked when `--banners` wants one it does not
//! yet have.
//!
//! # Complete Fanart.tv artwork
//!
//! `--fanart` widens this pass without adding a second metadata request. It
//! keeps the best portrait and banner, a 4K background whenever that pool is
//! non-empty (1080p otherwise), and the cover plus one cdART image per disc
//! for every local album identified by release-group MBID. Album images live
//! in the album's `artwork/` folder; artist images share the artist target
//! folder above. Separate completion records for every image family make an
//! installation which ran the older logo-only pass eligible once, while
//! allowing a later run to ask for a family that was previously excluded.

// Compiled in every build, for the reason `fetch` is.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aede_core::coverart::{self, Kind};
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{self, Facts, LabelFacts, Picture, ReleaseFacts, SourceRecord};
use aede_core::store;
use aede_core::user::EntityRef;
use aede_core::{clock, fanarttv};

use crate::ui;

use super::Res;
use super::fetch::{
    Ask, FanartOptions, Refusal, ask_bytes, ask_with_backoff, queue, worth_deferring,
};

/// An artist to ask about, and where the logo would be written.
struct Target {
    entity: EntityRef,
    name: String,
    /// The MusicBrainz artist identifier: what the lookup is made by, and
    /// what names the artist's folder under `assets/` when no shared folder
    /// exists.
    mbid: String,
    /// The folder the logo is written into — see the module doc for how it
    /// is chosen.
    destination: PathBuf,
    /// Local albums belonging to this artist, paired with their release-group
    /// identifiers so the album section of the same response can be used.
    albums: Vec<AlbumTarget>,
}

struct AlbumTarget {
    entity: EntityRef,
    release_group: String,
    destination: PathBuf,
}

/// A record label already identified by MusicBrainz, and the assets folder
/// which can hold its logo without guessing which album folder represents it.
struct LabelTarget {
    entity: EntityRef,
    name: String,
    mbid: String,
    destination: PathBuf,
}

/// What became of asking about one artist.
enum Outcome {
    /// A logo was found and is now on disk.
    Written {
        url: String,
        /// `false` when a file was already there — written by a previous,
        /// interrupted run, or dropped in by hand — in which case nothing
        /// was downloaded twice, only recorded.
        new: bool,
    },
    /// Asked, and nothing was found.
    Nothing,
}

#[derive(Default)]
struct ExtraReport {
    written: usize,
    failed: Vec<String>,
}

fn excluded_names(selected: FanartOptions) -> Vec<&'static str> {
    [
        (!selected.logo, "logo"),
        (!selected.label_logo, "label logo"),
        (!selected.portrait, "portrait"),
        (!selected.background, "background"),
        (!selected.banner, "banner"),
        (!selected.album_cover, "album cover"),
        (!selected.cdart, "cdART"),
    ]
    .into_iter()
    .filter_map(|(excluded, name)| excluded.then_some(name))
    .collect()
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
    println!(
        "{}",
        ui::section(if asked.fanart.all {
            "Fanart.tv artwork"
        } else {
            "Logos"
        })
    );
    let Some(key) = fanarttv_key else {
        // Unlike a portrait, a logo has no Wikidata fallback — a missing key
        // does not narrow what this pass can do, it is the whole of it.
        println!(
            "  {}",
            ui::dim(
                "fanart.tv is the only source for a logo, so a missing key leaves every artist unasked"
            )
        );
        println!("  {}", ui::dim(&fanarttv::no_key()));
        return Ok(());
    };

    // Labels have no folder of their own. Once `fetch --labels` has established
    // their MusicBrainz id, Fanart.tv can answer them just as certainly as an
    // artist; their images live in the stable per-label assets directory.
    if asked.fanart.label_logo {
        run_label_logos(
            catalog, transport, backoff, held, path, data_dir, key, asked,
        )?;
    }

    let targets = targets(
        catalog,
        held,
        asked.names,
        asked.scope,
        data_dir,
        asked.again,
        asked.fanart,
    );
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

    let least = targets.len() as u64 * fanarttv::REQUEST_INTERVAL.as_millis() as u64;
    match asked.fanart.all {
        true => println!(
            "  {}, one metadata request each plus the images found, at least {}",
            ui::plural(targets.len(), "artist"),
            ui::long_duration(least)
        ),
        false => println!(
            "  {}, one to two requests each, {} to {}",
            ui::plural(targets.len(), "artist"),
            ui::long_duration(least),
            ui::long_duration(least * 2)
        ),
    }
    if asked.fanart.logo {
        println!(
            "  {}",
            ui::dim(
                "written as logo.jpg or logo.png beside the music when every \
                 album shares a folder, in assets/ otherwise; an artist that \
                 already has one is never touched",
            )
        );
    }
    if asked.fanart.banner && !asked.fanart.all {
        println!(
            "  {}",
            ui::dim(
                "--banners: a wide banner.jpg or banner.png goes into the \
                 same folder, read from the very answer already fetched \
                 for the logo; a folder that has one is not asked about \
                 again",
            )
        );
    }
    if asked.fanart.all {
        let excluded = excluded_names(asked.fanart);
        println!(
            "  {}",
            ui::dim(&match excluded.is_empty() {
                true => "--fanart: every available image family is enabled".to_string(),
                false => format!("--fanart: excluded {}", excluded.join(", ")),
            })
        );
    }

    if super::fetch::asked_nothing(
        asked,
        &targets.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
    ) {
        return Ok(());
    }

    let (mut written, mut already, mut none, mut failed) = (0usize, 0usize, 0usize, 0usize);
    let mut extras_written = 0usize;
    let mut pending = queue(&targets);
    let mut done = 0usize;
    let total = targets.len();
    while let Some((target, retried)) = pending.pop_front() {
        print!("\r  asking: {}/{}", done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        match attempt(transport, backoff, target, key, asked.fanart) {
            Ok((outcome, extras)) => {
                if let Some(outcome) = &outcome {
                    store(held, target, outcome);
                }
                if extras.failed.is_empty() {
                    store_artwork(held, target, asked.fanart);
                }
                sources::save(held, path)?;
                match outcome {
                    Some(Outcome::Written { new: true, .. }) => written += 1,
                    Some(Outcome::Written { new: false, .. }) => already += 1,
                    Some(Outcome::Nothing) => none += 1,
                    None => {}
                }
                extras_written += extras.written;
                for why in extras.failed {
                    failed += 1;
                    eprintln!("\r  {} {} (artwork): {why}", ui::red("×"), target.name);
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

    let has_extras = asked.fanart.banner
        || asked.fanart.portrait
        || asked.fanart.background
        || asked.fanart.album_cover
        || asked.fanart.cdart;
    match (asked.fanart.logo, has_extras) {
        (true, true) => println!(
            "{} {written} logos, {extras_written} other images, {none} with \
             no logo found, {failed} failed",
            ui::green("→")
        ),
        (true, false) => println!(
            "{} {written} written, {none} with no logo found, {failed} failed",
            ui::green("→")
        ),
        (false, true) => println!(
            "{} {extras_written} images written, {failed} failed",
            ui::green("→")
        ),
        (false, false) => {}
    }
    if already > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} already had a logo on disk, placed by hand or by an \
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

/// Asks about one artist and writes the logo if one turns up.
///
/// A single function rather than a state machine, for the same reason
/// `portraits::attempt` is one: a retry simply redoes the whole thing on the
/// next pass through the queue.
///
/// Every selected artist image is read from the same metadata answer. Image
/// download failures travel back separately from [`Outcome`] so a failure on
/// one optional family never costs the logo this attempt can still write.
fn attempt(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    target: &Target,
    key: &str,
    selected: FanartOptions,
) -> Result<(Option<Outcome>, ExtraReport), Refusal> {
    let doc = match ask_with_backoff(transport, &fanarttv::lookup_url(&target.mbid, key), backoff) {
        // The service answers `404` for an artist it holds nothing of, which
        // is a real answer, not a failure — the same distinction
        // `fetch --portraits` reads out of the same status for Fanart.tv.
        Ok(doc) => Some(doc),
        Err(Refusal::Missing) => None,
        Err(why) => return Err(why),
    };
    if let Some(response) = &doc {
        fanarttv::artist_response(response, &target.mbid).map_err(Refusal::Failed)?;
    }

    let mut extras = ExtraReport::default();
    if let Some(response) = &doc {
        if selected.banner {
            fetch_extra(
                transport,
                backoff,
                target,
                Kind::Banner,
                fanarttv::banner_url(response),
                &mut extras,
            );
        }
        if selected.background {
            fetch_extra(
                transport,
                backoff,
                target,
                Kind::Background,
                fanarttv::background_url(response),
                &mut extras,
            );
        }
        if selected.portrait {
            fetch_extra(
                transport,
                backoff,
                target,
                Kind::Artist,
                fanarttv::portrait_url(response),
                &mut extras,
            );
        }
        if selected.album_cover || selected.cdart {
            fetch_album_artwork(transport, backoff, target, response, selected, &mut extras);
        }
    }

    if !selected.logo {
        return Ok((None, extras));
    }
    if let Some(url) = doc.as_ref().and_then(fanarttv::logo_url) {
        let bytes = ask_bytes(transport, &url, backoff)?;
        let new = write(target, Kind::Logo, &bytes)?;
        return Ok((Some(Outcome::Written { url, new }), extras));
    }
    Ok((Some(Outcome::Nothing), extras))
}

fn fetch_extra(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    target: &Target,
    kind: Kind,
    url: Option<String>,
    report: &mut ExtraReport,
) {
    let Some(url) = url else { return };
    if coverart::exists_beside(&target.destination, kind) {
        return;
    }
    let result = ask_bytes(transport, &url, backoff)
        .map_err(|why| why.to_string())
        .and_then(|bytes| write(target, kind, &bytes).map_err(|why| why.to_string()));
    match result {
        Ok(true) => report.written += 1,
        Ok(false) => {}
        Err(why) => report.failed.push(why),
    }
}

fn fetch_album_artwork(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    target: &Target,
    response: &aede_core::json::Json,
    selected: FanartOptions,
    report: &mut ExtraReport,
) {
    for album in &target.albums {
        let Some(artwork) = fanarttv::album_artwork(response, &album.release_group) else {
            continue;
        };
        let folder = coverart::extras_in(&album.destination);
        let mut images = Vec::new();
        if selected.album_cover
            && let Some(cover) = artwork.cover
        {
            images.push((Kind::Front, cover, (0, 1)));
        }
        if selected.cdart {
            let disc_count = artwork.discs.len();
            images.extend(
                artwork
                    .discs
                    .into_iter()
                    .enumerate()
                    .map(|(index, url)| (Kind::Media, url, (index, disc_count))),
            );
        }
        for (kind, url, position) in images {
            let result = ask_bytes(transport, &url, backoff)
                .map_err(|why| why.to_string())
                .and_then(|bytes| {
                    coverart::write_image(&folder, kind, position, &bytes)
                        .map(|written| matches!(written, coverart::Written::New(_)))
                });
            match result {
                Ok(true) => report.written += 1,
                Ok(false) => {}
                Err(why) => report.failed.push(why),
            }
        }
    }
}

/// Writes an image into the target's folder, unless one of that kind is
/// already there.
///
/// The general-purpose writer this program keeps for every image it puts
/// into somebody's library — see [`coverart::write_image`] — carrying both of
/// its guards: the bytes are sniffed, and nothing already there is ever
/// overwritten. `true` means this call is the one that wrote it.
fn write(target: &Target, kind: Kind, bytes: &[u8]) -> Result<bool, Refusal> {
    match coverart::write_image(&target.destination, kind, (0, 1), bytes) {
        Ok(coverart::Written::New(_)) => Ok(true),
        Ok(coverart::Written::Already(_)) => Ok(false),
        Err(why) => Err(Refusal::Failed(why)),
    }
}

/// Files what was found, including when that was nothing.
///
/// A record with no logo is not wasted, for the same reason `--portraits`
/// keeps one with nothing found: "asked, and there is nothing" is what keeps
/// the next run from asking again.
fn store(held: &mut sources::Sources, target: &Target, outcome: &Outcome) {
    let logo = match outcome {
        Outcome::Written { url, .. } => Some(Picture { url: url.clone() }),
        Outcome::Nothing => None,
    };
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some(target.mbid.clone()),
        fetched_at: clock::now_seconds(),
        // Reached by identifier — the artist's own MusicBrainz id — never by
        // matching a name, so nothing stored here is ever a guess.
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(sources::ArtistFacts {
            logo,
            ..Default::default()
        }),
    });
}

fn store_artwork(held: &mut sources::Sources, target: &Target, selected: FanartOptions) {
    let now = clock::now_seconds();
    for source in [
        selected
            .portrait
            .then_some(fanarttv::PORTRAIT_ARTWORK_SOURCE),
        selected.background.then_some(fanarttv::BACKGROUND_SOURCE),
        selected.banner.then_some(fanarttv::BANNER_SOURCE),
    ]
    .into_iter()
    .flatten()
    {
        held.set(SourceRecord {
            key: target.entity.key.clone(),
            source: source.to_string(),
            source_id: Some(target.mbid.clone()),
            fetched_at: now,
            confidence: sources::Confidence::Identified,
            facts: Facts::Artist(Default::default()),
        });
    }
    for album in &target.albums {
        for source in [
            selected.album_cover.then_some(fanarttv::ALBUM_COVER_SOURCE),
            selected.cdart.then_some(fanarttv::CDART_SOURCE),
        ]
        .into_iter()
        .flatten()
        {
            held.set(SourceRecord {
                key: album.entity.key.clone(),
                source: source.to_string(),
                source_id: Some(album.release_group.clone()),
                fetched_at: now,
                confidence: sources::Confidence::Identified,
                facts: Facts::Release(ReleaseFacts::default()),
            });
        }
    }
}

/// How many artists this pass would ask about, if it ran now.
///
/// The same function as the walk, counted rather than re-derived — see
/// [`super::summaries::waiting`] for why that discipline matters here too.
/// `None` for the key answers zero without walking anything: with no key
/// there is nothing this pass could ever ask.
pub fn waiting(
    catalog: &Catalog,
    held: &sources::Sources,
    data_dir: &Path,
    fanarttv_key: Option<&str>,
) -> usize {
    if fanarttv_key.is_none() {
        return 0;
    }
    // `--banners` narrows nothing this counts — an artist a `--banners` run
    // would still visit for the logo alone is already counted, the same way
    // `covers::waiting` never asks about `--images`.
    targets(
        catalog,
        held,
        &[],
        &super::fetch::EVERYTHING,
        data_dir,
        false,
        FanartOptions {
            logo: true,
            label_logo: true,
            ..FanartOptions::default()
        },
    )
    .len()
}

/// Who to ask about: artists MusicBrainz answered, that are missing either
/// the logo or — when `--banners` was given — the banner.
fn targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    scope: &super::fetch::Scope,
    data_dir: &Path,
    again: bool,
    selected: FanartOptions,
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
        let Facts::Artist(_) = &record.facts else {
            continue;
        };
        // An ordinary fetch always stores the identifier it matched, so this
        // is not expected to be empty — but a layer on disk is user-editable
        // text, and a row missing it is a row this pass cannot ask about and
        // has no business guessing.
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

        // Computed before the skip decision, not after: whether a banner is
        // still wanted is a question about this very folder, the same
        // ordering `covers::survey` uses for `--images`.
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

        let albums = releases_by_artist
            .get(&artist_row.id)
            .into_iter()
            .flat_map(|releases| releases.iter().copied())
            .filter(|release| scope.has_release(release.id))
            .filter_map(|release| {
                let album_entity = EntityRef::of(catalog, EntityKind::Release, release.id)?;
                let release_group = release.release_group_mbid.clone().or_else(|| {
                    held.get(&album_entity, sources::MUSICBRAINZ)
                        .and_then(|record| record.source_id.clone())
                })?;
                Some(AlbumTarget {
                    entity: album_entity,
                    release_group,
                    destination: PathBuf::from(&release.folder),
                })
            })
            .collect::<Vec<_>>();

        let legacy_complete = held.get(&entity, fanarttv::ARTWORK_SOURCE).is_some();
        let needs_logo = selected.logo && (again || !has_logo(held, &entity));
        let needs_artist_art = [
            (selected.portrait, fanarttv::PORTRAIT_ARTWORK_SOURCE),
            (selected.background, fanarttv::BACKGROUND_SOURCE),
            (selected.banner, fanarttv::BANNER_SOURCE),
        ]
        .into_iter()
        .any(|(wanted, source)| {
            let already_on_disk = source == fanarttv::BANNER_SOURCE
                && coverart::exists_beside(&destination, Kind::Banner);
            wanted
                && (again
                    || (!legacy_complete
                        && !already_on_disk
                        && held.get(&entity, source).is_none()))
        });
        let needs_album_art = albums.iter().any(|album| {
            let legacy = held.get(&album.entity, fanarttv::ARTWORK_SOURCE).is_some();
            [
                (selected.album_cover, fanarttv::ALBUM_COVER_SOURCE),
                (selected.cdart, fanarttv::CDART_SOURCE),
            ]
            .into_iter()
            .any(|(wanted, source)| {
                wanted && (again || (!legacy && held.get(&album.entity, source).is_none()))
            })
        });
        if !needs_logo && !needs_artist_art && !needs_album_art {
            continue;
        }

        targets.push(Target {
            entity,
            name: artist_row.name.clone(),
            mbid: mbid.clone(),
            destination,
            albums,
        });
    }
    targets
}

/// `true` when a logo — not just an answer — is already on record.
fn has_logo(held: &sources::Sources, entity: &EntityRef) -> bool {
    // A record with `logo: None` means Fanart.tv was already asked and had
    // none. It is still an answer, not an invitation to ask again forever.
    held.get(entity, fanarttv::LOGO_SOURCE).is_some()
}

/// Downloads logos for labels that `fetch --labels` has already identified.
#[allow(clippy::too_many_arguments)]
fn run_label_logos(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &Path,
    data_dir: &Path,
    key: &str,
    asked: &super::fetch::Asked,
) -> Res {
    let targets = label_targets(
        catalog,
        held,
        asked.names,
        asked.scope,
        data_dir,
        asked.again,
    );
    if targets.is_empty() {
        return Ok(());
    }
    println!("{}", ui::section("Label logos"));
    println!(
        "  {}, one to two requests each",
        ui::plural(targets.len(), "label")
    );
    if super::fetch::asked_nothing(
        asked,
        &targets
            .iter()
            .map(|target| target.name.clone())
            .collect::<Vec<_>>(),
    ) {
        return Ok(());
    }

    let (mut written, mut none, mut failed) = (0usize, 0usize, 0usize);
    let mut pending = queue(&targets);
    let mut done = 0usize;
    let total = targets.len();
    while let Some((target, retried)) = pending.pop_front() {
        print!("\r  asking: {}/{}", done + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        match attempt_label(transport, backoff, target, key) {
            Ok(outcome) => {
                store_label(held, target, &outcome);
                sources::save(held, path)?;
                match outcome {
                    Outcome::Written { new: true, .. } => written += 1,
                    Outcome::Written { new: false, .. } | Outcome::Nothing => none += 1,
                }
                done += 1;
            }
            Err(why) if worth_deferring(&why) && !retried => pending.push_back((target, true)),
            Err(why) => {
                failed += 1;
                done += 1;
                eprintln!("\r  {} {}: {why}", ui::red("×"), target.name);
            }
        }
    }
    println!();
    println!(
        "{} {written} written, {none} with no logo found, {failed} failed",
        ui::green("→")
    );
    Ok(())
}

fn attempt_label(
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    target: &LabelTarget,
    key: &str,
) -> Result<Outcome, Refusal> {
    let doc = match ask_with_backoff(
        transport,
        &fanarttv::label_lookup_url(&target.mbid, key),
        backoff,
    ) {
        Ok(doc) => doc,
        Err(Refusal::Missing) => return Ok(Outcome::Nothing),
        Err(why) => return Err(why),
    };
    fanarttv::label_response(&doc, &target.mbid).map_err(Refusal::Failed)?;
    let Some(url) = fanarttv::label_logo_url(&doc) else {
        return Ok(Outcome::Nothing);
    };
    let bytes = ask_bytes(transport, &url, backoff)?;
    let new = match coverart::write_image(&target.destination, Kind::Logo, (0, 1), &bytes) {
        Ok(coverart::Written::New(_)) => true,
        Ok(coverart::Written::Already(_)) => false,
        Err(why) => return Err(Refusal::Failed(why)),
    };
    Ok(Outcome::Written { url, new })
}

fn store_label(held: &mut sources::Sources, target: &LabelTarget, outcome: &Outcome) {
    let logo = match outcome {
        Outcome::Written { url, .. } => Some(Picture { url: url.clone() }),
        Outcome::Nothing => None,
    };
    held.set(SourceRecord {
        key: target.entity.key.clone(),
        source: fanarttv::LABEL_LOGO_SOURCE.to_string(),
        source_id: Some(target.mbid.clone()),
        fetched_at: clock::now_seconds(),
        confidence: sources::Confidence::Identified,
        facts: Facts::Label(LabelFacts { logo }),
    });
}

fn label_targets(
    catalog: &Catalog,
    held: &sources::Sources,
    wanted: &[String],
    scope: &super::fetch::Scope,
    data_dir: &Path,
    again: bool,
) -> Vec<LabelTarget> {
    catalog
        .labels
        .iter()
        .filter_map(|label| {
            if !super::fetch::reaches(wanted, &[label.name.as_str()])
                || !scope.has_label(&label.key)
            {
                return None;
            }
            let entity = EntityRef::of(catalog, EntityKind::Label, label.id)?;
            if !again && held.get(&entity, fanarttv::LABEL_LOGO_SOURCE).is_some() {
                return None;
            }
            let mbid =
                label
                    .mbid
                    .clone()
                    .or_else(|| match held.label_identity(catalog, label.id)? {
                        sources::LabelIdentityResolution::Confirmed { mbid, .. }
                        | sources::LabelIdentityResolution::Agrees { mbid, .. } => Some(mbid),
                        sources::LabelIdentityResolution::Local { .. }
                        | sources::LabelIdentityResolution::Suggested { .. }
                        | sources::LabelIdentityResolution::Conflict { .. } => None,
                    })?;
            Some(LabelTarget {
                entity,
                name: label.name.clone(),
                destination: store::assets_dir(data_dir).join("labels").join(&mbid),
                mbid,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "logos_tests.rs"]
mod tests;
