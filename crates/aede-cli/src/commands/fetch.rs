//! The `fetch` command: ask MusicBrainz about what is in the catalog.
//!
//! Everything it decides — which answer is about us, how firmly, what to store
//! — lives in [`aede_core::musicbrainz`] and is tested without a network. What
//! is here is the walk, the progress, and the saving as it goes.
//!
//! **The rate is the shape of this command.** One request per second is not a
//! detail to tune later: six hundred artists is ten minutes, and a run that
//! long has to say so before it starts, show where it is, and lose nothing
//! when it is interrupted. So it saves after every answer rather than at the
//! end — the same rule `check` follows for a scan that may take an hour.

// Compiled in every build, because the tests below prove it in every build;
// only `fetch` itself needs the feature, and a build without it would
// otherwise report this machinery as dead.
#![cfg_attr(not(feature = "fetch"), allow(dead_code))]

use aede_core::json::Json;
use aede_core::model::EntityKind;
use aede_core::sources::{self, Facts, SourceRecord};
use aede_core::user::EntityRef;
use aede_core::{clock, musicbrainz, text};

use crate::args::Args;
use crate::ui;

use super::Res;

/// Whatever can answer a URL with JSON.
///
/// A trait for one implementation, which usually earns nothing — here it earns
/// the only thing that matters: the walk below, the refusals, the counting and
/// the saving all compile and run **without a network stack**, against a fake
/// that answers from a fixture. What is left unproven is then the twenty lines
/// that hand a URL to the client library, instead of this whole command.
pub trait Ask {
    /// Fetches a URL and parses the answer, or says why it could not.
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal>;

    /// Fetches a URL and hands back what came, unread.
    ///
    /// For the one thing this program downloads that is not an answer: an
    /// image. It sits on the same trait as [`Ask::get_json`] rather than on one
    /// of its own so that a caller cannot fetch a picture without waiting its
    /// turn — the throttle belongs to the client, and a second client would be
    /// a second rate limiter to forget about.
    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal>;
}

/// Why an answer did not arrive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The service asked us to slow down: stop, do not retry.
    RateLimited,
    /// Anything else, already worded for a reader.
    Failed(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::RateLimited => write!(
                f,
                "the service is refusing requests because too many were sent \
                 (one per second is the limit); nothing was lost, try later"
            ),
            Refusal::Failed(detail) => write!(f, "{detail}"),
        }
    }
}

#[cfg(not(feature = "fetch"))]
pub fn fetch(_args: &Args) -> Res {
    // The command exists in every build so that the help, the dispatch table
    // and the guards stay one list. A build without the feature says what it
    // is rather than pretending the command was never there.
    Err(
        "this build has no network support: it was compiled without the \
         \"fetch\" feature, so it cannot reach MusicBrainz"
            .into(),
    )
}

/// The client library, wrapped so that everything below it is testable.
#[cfg(feature = "fetch")]
struct Http(aede_core::http::Client);

#[cfg(feature = "fetch")]
impl Ask for Http {
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal> {
        match self.0.get_json(url) {
            Ok(value) => Ok(value),
            Err(aede_core::http::Error::RateLimited) => Err(Refusal::RateLimited),
            Err(other) => Err(Refusal::Failed(other.to_string())),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        match self.0.get_bytes(url) {
            Ok(bytes) => Ok(bytes),
            Err(aede_core::http::Error::RateLimited) => Err(Refusal::RateLimited),
            Err(other) => Err(Refusal::Failed(other.to_string())),
        }
    }
}

#[cfg(feature = "fetch")]
pub fn fetch(args: &Args) -> Res {
    use aede_core::http::Client;
    let mut transport = Http(Client::new(
        identity(env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_REPOSITORY"))?,
        musicbrainz::REQUEST_INTERVAL,
    ));
    run(args, &mut transport)
}

/// How long to wait before asking again, after a `503`.
///
/// Backing off is the polite reaction to a service saying "not now", and it is
/// also what tells the two meanings of `503` apart. MusicBrainz answers it both
/// when the rate has been exceeded — a ban that lasts — and when its search
/// server is momentarily overloaded, which passes. Nothing in the response
/// distinguishes them, but their *behaviour* does: a transient one lets the
/// next attempt through, a ban does not. So the client tries three times,
/// waiting longer each time, and only then gives up.
///
/// The first version stopped the whole run on the first `503`, which turned a
/// hiccup on request 5 of 402 into "come back later" — and looked exactly like
/// a rate limit that had not been exceeded.
const RETRY_AFTER: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(2),
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
];

/// The names typed after the command, normalised, or empty for the whole shelf.
///
/// Read once for the whole run and handed to every pass, because a name given
/// to `aede fetch --discography mika` used to be **swallowed**: the pass ran
/// over the entire library and nothing said the word had been ignored. That is
/// the fault this program refuses everywhere else, and it was in four places
/// at once — the ordinary fetch was the only half that read them.
pub(super) fn names_given(args: &Args) -> Vec<String> {
    args.positionals
        .iter()
        .map(|name| text::normalize(name))
        .filter(|name| !name.is_empty())
        .collect()
}

/// `true` when one of the names typed reaches this thing.
///
/// Empty means everything, which is what makes a bare `aede fetch --covers`
/// the whole library. A name matches on **any** of the strings offered — for
/// an album that is its title and its artist, so `--covers manson` finds the
/// records as well as the person, exactly as the ordinary fetch does.
///
/// Matching is `contains` on the normalised form, the same rule as the
/// ordinary fetch: a reader who types `pink` should not have to remember
/// whether the band is filed as "Pink Floyd" or "The Pink Floyd Sound".
pub(super) fn reaches(wanted: &[String], candidates: &[&str]) -> bool {
    wanted.is_empty()
        || candidates.iter().any(|candidate| {
            let key = text::normalize(candidate);
            wanted.iter().any(|w| key.contains(w.as_str()))
        })
}

/// What to say when names were given and nothing came of them.
///
/// Three states again, and only the first two used to be told apart. "No such
/// artist here" and "that artist is already done" are different problems with
/// different next steps, and printing the general "run fetch first" for both
/// sends somebody to re-run a pass that has nothing to do.
pub(super) fn nothing_named(wanted: &[String], but_for_full: usize) -> String {
    let names = wanted.join(", ");
    match but_for_full {
        0 => format!("nothing here matches {names}"),
        _ => format!(
            "{} matching {names}, already done: --full asks again",
            ui::plural(but_for_full, "artist")
        ),
    }
}

/// What the reader asked for, gathered once and handed to every pass.
///
/// The passes grew one parameter at a time — a name, a size, `--images`,
/// `--dry-run` — until the cover pass took nine, which is the point at which a
/// signature stops being read and starts being counted. Bundling them also
/// makes the three passes **the same shape**, so adding a fourth is a call
/// that looks like the others rather than a new argument list to invent.
///
/// Fields nobody but one pass reads — `size`, `images` — sit here all the
/// same: they are things the reader asked for, which is what this is.
pub(super) struct Asked<'a> {
    /// The names typed after the command; empty means the whole shelf.
    pub names: &'a [String],
    /// `--full`: ask again about what is already held.
    pub again: bool,
    /// `--dry-run`: say what would happen and do none of it.
    pub dry_run: bool,
    /// `--size`: how large an image to keep.
    pub size: aede_core::coverart::Size,
    /// `--images`: keep the pictures that are not the cover.
    pub images: bool,
}

/// A pass `fetch` can be asked for instead of its ordinary run.
///
/// Each answers a different question, and any combination of them is allowed.
/// What is **not** allowed is the typed order deciding anything: the passes go
/// out from the artist — who they are, what they recorded, what the records
/// look like — and running them the other way round would ask about albums
/// before the fetch that names them. So the order is fixed here and the
/// command says so when there is more than one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Wikipedia, through the wikidata link already in the layer.
    Summaries,
    /// Everything MusicBrainz credits to each artist.
    Discography,
    /// The front image of every album that has none.
    Covers,
    /// What AcoustID hears in the files that have been fingerprinted.
    Identify,
}

impl Pass {
    /// The passes asked for, in the order they will run.
    fn asked_for(args: &Args) -> Vec<Pass> {
        [
            ("summaries", Pass::Summaries),
            ("discography", Pass::Discography),
            ("covers", Pass::Covers),
            ("identify", Pass::Identify),
        ]
        .into_iter()
        .filter(|(flag, _)| args.has(flag))
        .map(|(_, pass)| pass)
        .collect()
    }

    /// The option that asks for it, for a message to name.
    fn option(self) -> &'static str {
        match self {
            Pass::Summaries => "--summaries",
            Pass::Discography => "--discography",
            Pass::Covers => "--covers",
            Pass::Identify => "--identify",
        }
    }

    /// Whether it needs the catalog, which decides whether one is loaded.
    ///
    /// `--summaries` does not: its input is the wikidata link already stored,
    /// and failing on a missing catalog would be a refusal with no reason
    /// behind it. Loading one anyway "for symmetry" would break that.
    fn needs_the_catalog(self) -> bool {
        self != Pass::Summaries
    }
}

/// Runs the passes asked for, in order, stopping at the first that cannot go on.
///
/// A pass returning `Err` means it could not do its work at all — the layer
/// could not be written, an option was unusable. Individual requests that fail
/// are counted inside each pass and do not stop the next one, which is the
/// distinction that lets a run over a large library survive a bad afternoon.
fn second_passes(
    passes: &[Pass],
    args: &Args,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    asked: &Asked,
) -> Res {
    // The order is not the typed one, so it is stated rather than left to be
    // inferred from the order the sections happen to come out in.
    if passes.len() > 1 {
        println!(
            "  {}",
            ui::dim(&format!(
                "{} in this order: {}",
                ui::plural(passes.len(), "pass"),
                passes
                    .iter()
                    .map(|p| p.option())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        );
    }

    // Loaded once for the whole run, and only if something needs it.
    let catalog = match passes.iter().any(|p| p.needs_the_catalog()) {
        true => Some(super::load(args)?),
        false => None,
    };
    for pass in passes {
        match pass {
            Pass::Summaries => {
                let langs = super::summaries::preferred_langs(
                    std::env::var("LC_ALL")
                        .or_else(|_| std::env::var("LANG"))
                        .ok()
                        .as_deref(),
                );
                super::summaries::run(transport, backoff, &langs, held, path, asked)?;
            }
            Pass::Discography => {
                let catalog = catalog.as_ref().expect("a catalog was loaded for it");
                super::discography::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Covers => {
                let catalog = catalog.as_ref().expect("a catalog was loaded for it");
                super::covers::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Identify => {
                let catalog = catalog.as_ref().expect("a catalog was loaded for it");
                super::identify::run(catalog, transport, backoff, held, path, asked)?;
            }
        }
    }
    Ok(())
}

/// The whole of the command except reaching the network.
pub fn run(args: &Args, transport: &mut dyn Ask) -> Res {
    run_with(args, transport, &RETRY_AFTER)
}

/// [`run`], with the waits made explicit so a test does not have to sit
/// through them.
pub fn run_with(args: &Args, transport: &mut dyn Ask, backoff: &[std::time::Duration]) -> Res {
    let path = sources::sources_path(&super::data_dir(args));
    let mut held = sources::load(&path)?.unwrap_or_default();

    // An option that only means something to another pass, given on its own,
    // is refused rather than ignored: a reader who typed `--images` and got an
    // ordinary fetch would conclude the feature does not work.
    if !args.has("covers") {
        for option in ["images", "size"] {
            if args.has(option) {
                return Err(format!(
                    "--{option} belongs to the cover art pass: aede fetch --covers --{option}"
                )
                .into());
            }
        }
    }

    // A second pass is a different question, often of a different service, so
    // each is its own run rather than a stage of the ordinary fetch:
    // `--summaries` alone does not re-ask MusicBrainz about a library it has
    // already answered on.
    //
    // Several of them together run one after another. They used to be three
    // `return`s in a row, so `--covers --discography` ran the covers and
    // **dropped the discography without a word** — the fault this program
    // refuses everywhere else: an option that cannot be honoured is refused,
    // never swallowed. Here it could be honoured, so it is.
    // What to ask about: the names given, or every artist in the library. Read
    // here rather than inside the ordinary run, because every pass honours
    // them now — `aede fetch --discography mika` used to swallow the word.
    let wanted = names_given(args);

    // Read once for the whole run, and before anything is asked: a width the
    // archive does not generate must be refused before a summaries pass has
    // spent ten minutes on the network for it.
    let asked = Asked {
        names: &wanted,
        again: args.has("full"),
        dry_run: args.has("dry-run"),
        size: match args.value("size") {
            Some(text) => aede_core::coverart::Size::parse(text).ok_or_else(|| {
                format!(
                    "--size takes 250, 500, 1200 or original; \"{text}\" is not \
                     one the archive generates"
                )
            })?,
            None => super::covers::DEFAULT_SIZE,
        },
        images: args.has("images"),
    };

    let passes = Pass::asked_for(args);
    if !passes.is_empty() {
        return second_passes(&passes, args, transport, backoff, &mut held, &path, &asked);
    }

    let catalog = super::load(args)?;

    // No `--limit` here on purpose: everywhere else in this program it means
    // "show a window of the result", and bounding how much work is done is a
    // different thing wearing the same word. Naming artists narrows the run.
    let mut targets: Vec<(EntityRef, String, Option<String>)> = Vec::new();
    for artist in &catalog.artists {
        // A blank name would go out as an empty query, which the search server
        // does not answer politely: it fails, and the failure looks like a
        // rate limit three steps from its cause.
        if artist.name.trim().is_empty() {
            continue;
        }
        let key = text::normalize(&artist.name);
        if !wanted.is_empty() && !wanted.iter().any(|w| key.contains(w.as_str())) {
            continue;
        }
        if let Some(entity) = EntityRef::of(&catalog, EntityKind::Artist, artist.id) {
            // Already answered, unless asked to do it again. A second run over
            // a library should cost what changed, not ten minutes again.
            if !args.has("full") && held.get(&entity, sources::MUSICBRAINZ).is_some() {
                continue;
            }
            targets.push((entity, artist.name.clone(), artist.mbid.clone()));
        }
    }
    // The albums, decided before anything is asked, so that one estimate and
    // one confirmation cover the whole run. Two prompts for one question is
    // how a confirmation becomes something a reader clicks through.
    let albums = super::releases::targets(&catalog, &held, &wanted, args.has("full"));

    if targets.is_empty() && albums.is_empty() {
        println!("{}", ui::section("Fetch"));
        println!(
            "  {}",
            ui::dim(match wanted.is_empty() {
                true => "every artist and album has already been asked about (--full asks again)",
                false => "nothing in this catalog matches, or it was already asked about",
            })
        );
        // Offered here too, and this is the exit that matters most: a reader
        // who fetched their library before this existed reaches *this* branch
        // every time, never the one below, and would never be told the second
        // pass is available. An announcement made only on the path that has
        // just done work is an announcement nobody who finished first ever
        // sees.
        offer_summaries(&held);
        offer_discography(&catalog, &held);
        offer_covers(&catalog, &held);
        offer_identify(&catalog, &held);
        return Ok(());
    }

    // How long this will take, before it starts. A predicted duration is
    // usually a bad idea here — how long a read takes depends on the disk —
    // but this one is not a guess: the rate is fixed by the service, and every
    // album costs exactly one request whatever route it takes in.
    let asks = targets.len() + albums.len();
    let total_ms = asks as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    println!("{}", ui::section("Fetch"));
    println!(
        "  {} and {} at one request per second, about {}",
        ui::plural(targets.len(), "artist"),
        ui::plural(albums.len(), "album"),
        ui::long_duration(total_ms)
    );
    if args.has("dry-run") {
        for (_, name, _) in &targets {
            println!("  {}", ui::dim(name));
        }
        for title in super::releases::names(&albums) {
            println!("  {}", ui::dim(title));
        }
        println!("  {}", ui::dim("nothing was asked: --dry-run"));
        return Ok(());
    }

    // A run of ten minutes is something to agree to, not something to
    // discover. Short ones are not worth a question — a confirmation asked
    // every time is a confirmation nobody reads.
    if asks > CONFIRM_ABOVE && !super::confirmed(args, "ask about all of them")? {
        println!("  {}", ui::dim("nothing was asked"));
        return Ok(());
    }

    let (mut stored, mut refused, mut failed) = (0usize, 0usize, 0usize);
    for (done, (entity, name, mbid)) in targets.iter().enumerate() {
        print!("\r  asking: {}/{asks}", done + 1);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // Ask by identifier when the tags carry one — Picard writes it, and a
        // library it has been through knows exactly which artist it means.
        // Searching by name there would replace an answer with a guess, and
        // it also asks a poorer question: a search result is abbreviated,
        // while a lookup returns the entity.
        let url = match mbid {
            Some(mbid) => format!(
                "{}/artist/{mbid}?fmt=json&inc={}",
                musicbrainz::WEB_SERVICE,
                musicbrainz::ARTIST_INCLUDES
            ),
            None => format!(
                "{}/artist/?query={}&fmt=json&limit=5",
                musicbrainz::WEB_SERVICE,
                encode(&musicbrainz::escape_query(name))
            ),
        };
        let answer = match ask_with_backoff(transport, &url, backoff) {
            Ok(answer) => answer,
            Err(Refusal::RateLimited) => {
                // Nothing is lost: what was stored stays stored, and the run
                // stops rather than hammering a service that has just said no.
                //
                // The name and the URL go into the message because the service
                // answers `503` to two different things — the rate being
                // exceeded, and its search backend refusing a query it could
                // not parse — and nothing in the response tells them apart.
                // Without them, a query this program built badly reads as
                // "you are going too fast", which is where an hour goes.
                println!();
                sources::save(&held, &path)?;
                return Err(format!(
                    "the service refused {} times in a row, waiting longer each \
                     time; that is a rate limit rather than a hiccup, so nothing \
                     more was asked.\n  it stopped on \"{name}\", asking:\n  {url}",
                    backoff.len() + 1
                )
                .into());
            }
            Err(other) => {
                failed += 1;
                eprintln!("\r  {} {name}: {other}", ui::red("×"));
                continue;
            }
        };

        // A lookup answered about the identifier it was given: that is a
        // certainty, and the only thing in this program that produces one.
        let found = match mbid {
            Some(_) => musicbrainz::artist(&answer)
                .map(|c| (c, aede_core::sources::Confidence::Identified))
                .ok_or(musicbrainz::NoMatch::Nothing),
            None => musicbrainz::best_match(&musicbrainz::artists(&answer), name),
        };
        match found {
            Ok((candidate, confidence)) => {
                held.set(SourceRecord {
                    key: entity.key.clone(),
                    source: sources::MUSICBRAINZ.to_string(),
                    source_id: Some(candidate.mbid),
                    fetched_at: clock::now_seconds(),
                    confidence,
                    facts: Facts::Artist(candidate.facts),
                });
                stored += 1;
                // Saved after each answer, not at the end: ten minutes of
                // waiting must not be undone by one interruption.
                sources::save(&held, &path)?;
            }
            Err(why) => {
                refused += 1;
                eprintln!("\r  {} {name}: {}", ui::yellow("?"), refusal(&why));
            }
        }
    }

    // The albums, in the same run and counted into the same report: they are
    // the same question asked of the same service, and splitting the totals
    // would leave the reader adding two lines up by eye.
    let (albums_stored, albums_refused, albums_failed) = super::releases::run(
        transport,
        backoff,
        &albums,
        &mut held,
        &path,
        targets.len(),
        asks,
    )?;
    stored += albums_stored;
    refused += albums_refused;
    failed += albums_failed;
    println!();

    println!(
        "{} {stored} stored, {refused} left alone, {failed} failed",
        ui::green("→")
    );
    if refused > 0 {
        // A refusal is the design working, not a fault, and a reader who is
        // not told that will read it as one.
        println!(
            "  {}",
            ui::dim(
                "left alone means no answer was clearly about that artist — nothing was guessed"
            )
        );
    }
    offer_summaries(&held);
    offer_discography(&catalog, &held);
    offer_covers(&catalog, &held);
    offer_identify(&catalog, &held);
    println!("  {}", ui::dim(&path.display().to_string()));
    Ok(())
}

/// Names the second pass, when there is something for it to do.
///
/// Printed on **both** ways out of the command, which is the whole point. A
/// flag that only `--help` mentions is a flag nobody finds, and a reader whose
/// library was already fetched leaves through the early return every time — so
/// announcing it only after a run that did work would hide it from exactly the
/// people who are ready for it.
///
/// The count comes from the pass's own `targets`, counted rather than derived a
/// second time: an offer that disagreed with the run it offers would be worse
/// than no offer.
fn offer_summaries(held: &sources::Sources) {
    let door = super::summaries::waiting(held);
    if door == 0 {
        return;
    }
    // `ui::plural` is no help here: the verb has to agree with the count too,
    // and it is irregular. Written out rather than assembled.
    let (them, have) = match door {
        1 => ("1 of them".to_string(), "has"),
        _ => (format!("{door} of them"), "have"),
    };
    println!(
        "  {}",
        ui::dim(&format!(
            "{them} {have} a wikidata link — aede fetch --summaries reads the article"
        ))
    );
}

/// Names the cover pass, when there are albums without artwork.
/// Names the identify pass, when files have been fingerprinted for it.
///
/// Offered only once a fingerprint exists, because the pass cannot do
/// anything before that and naming it earlier would be an instruction with a
/// missing step in it. `aede fingerprint` names this one on the line where it
/// finishes, which is the other half of the handover.
fn offer_identify(catalog: &aede_core::model::Catalog, held: &sources::Sources) {
    let door = super::identify::waiting(catalog, held);
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} fingerprinted and never asked about — \
             aede fetch --identify asks AcoustID what they are",
            ui::plural(door, "file")
        ))
    );
}

fn offer_covers(catalog: &aede_core::model::Catalog, held: &sources::Sources) {
    let door = super::covers::waiting(catalog, held);
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{door} of your albums have no cover, in the files or beside them — \
             aede fetch --covers looks for one"
        ))
    );
}

/// Names the discography pass, when there is something for it to browse.
///
/// Separate from the summaries offer rather than folded into it: they are two
/// different passes at two different costs, and one line proposing both would
/// make the reader work out which count belonged to which.
fn offer_discography(catalog: &aede_core::model::Catalog, held: &sources::Sources) {
    let door = super::discography::waiting(catalog, held);
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{door} of them can be browsed for what else they recorded — \
             aede fetch --discography, then aede missing"
        ))
    );
}

/// Above this many requests, the run is long enough to be worth agreeing to.
///
/// Below it there is nothing to decide — twenty seconds is not a commitment —
/// and a confirmation asked every time is a confirmation nobody reads.
const CONFIRM_ABOVE: usize = 20;

/// The `User-Agent`, refused rather than sent empty.
///
/// This is the bug that cost the first real run: `repository` was set on the
/// workspace and not inherited by the crates, so `CARGO_PKG_REPOSITORY` was
/// the empty string and the header went out as `aede/0.1.0 (  )`. MusicBrainz
/// throttles callers with no contact as one shared anonymous pool, so the very
/// first request came back `503` — a symptom pointing at the rate limit, three
/// steps away from the cause. A build that cannot say who it is now stops
/// here, where the message can name the manifest.
fn identity(version: &str, contact: &str) -> Result<String, Box<dyn std::error::Error>> {
    if contact.trim().is_empty() {
        return Err(
            "this build carries no contact address, and MusicBrainz requires \
                    one in the User-Agent: add `repository.workspace = true` to the \
                    crate's Cargo.toml and rebuild"
                .into(),
        );
    }
    Ok(format!("aede/{version} ( {contact} )"))
}

/// Asks, and asks again after waiting when the service says "not now".
///
/// Only `503` is retried. A failure to reach the service at all, or an answer
/// that is not JSON, will not be cured by waiting — retrying those would only
/// take three times as long to report the same thing.
pub(super) fn ask_with_backoff(
    transport: &mut dyn Ask,
    url: &str,
    backoff: &[std::time::Duration],
) -> Result<Json, Refusal> {
    let mut attempt = 0;
    loop {
        match transport.get_json(url) {
            Err(Refusal::RateLimited) if attempt < backoff.len() => {
                std::thread::sleep(backoff[attempt]);
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// Says why an answer was not taken, in words rather than a variant name.
pub(super) fn refusal(why: &aede_core::musicbrainz::NoMatch) -> String {
    use aede_core::musicbrainz::NoMatch;
    match why {
        NoMatch::Nothing => "MusicBrainz knows nobody by that name".to_string(),
        NoMatch::Ambiguous(names) => {
            format!("several answers are equally good: {}", names.join(", "))
        }
        NoMatch::TooWeak { best, score } => {
            format!("the closest was \"{best}\" at {score}%, not close enough")
        }
    }
}

/// Percent-encodes a query value.
///
/// Written by hand for the same reason the JSON reader is: one dependency, and
/// this is thirty lines. The unreserved set is RFC 3986's.
pub(super) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// The tests are long enough to be their own file. `#[path]` keeps them a
// child module of this one, so they still reach what is private here — the
// split is about the size of a file, not about what a test may see.
#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
