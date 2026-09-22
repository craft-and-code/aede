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

use std::collections::BTreeSet;

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id};
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
    /// The service answered, and the answer was that it has nothing.
    ///
    /// **Not a failure**, and kept apart from one for that reason. A library of
    /// four hundred tracks holds plenty LRCLIB has never seen, and a run that
    /// reported four hundred failures would be describing a working service as
    /// broken. It was read out of the message text before this existed — a
    /// `detail.contains("404")` — which is a comparison that breaks the day the
    /// wording changes and says nothing when it does.
    Missing,
    /// No route, no name, a timeout: the service was never reached at all.
    ///
    /// Kept apart from [`Refusal::Failed`] because it is the one refusal a
    /// later attempt might not repeat — see [`worth_deferring`]. A run gives
    /// it exactly one more try, at the end of the list, before treating it
    /// the same as any other failure.
    Unreachable(String),
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
            Refusal::Missing => write!(f, "the service has nothing for this"),
            Refusal::Unreachable(detail) => write!(f, "could not reach the service: {detail}"),
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
            Err(aede_core::http::Error::Status(404)) => Err(Refusal::Missing),
            Err(aede_core::http::Error::Network(detail)) => Err(Refusal::Unreachable(detail)),
            Err(other) => Err(Refusal::Failed(other.to_string())),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        match self.0.get_bytes(url) {
            Ok(bytes) => Ok(bytes),
            Err(aede_core::http::Error::RateLimited) => Err(Refusal::RateLimited),
            Err(aede_core::http::Error::Status(404)) => Err(Refusal::Missing),
            Err(aede_core::http::Error::Network(detail)) => Err(Refusal::Unreachable(detail)),
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

/// What was typed after the command, sorted into folders and names.
///
/// **A path is not a name**, and every positional used to be read as one. So
/// `aede fetch --lyrics ~/Music/Alastis` normalised a whole path into words,
/// matched none of them against a title or an artist, and asked LRCLIB about
/// the **entire library** — the swallowed argument this program refuses
/// everywhere else, wearing a slash. Worse than a mistyped name, because it
/// looks like it worked.
///
/// A positional naming something that exists on the disk is a folder;
/// everything else is a name. That is the test [`super::scope_of`] already
/// makes for the commands that take `[folder…]`, so `aede fetch --lyrics
/// ~/Music/Alastis` and `aede check ~/Music/Alastis` mean the same thing by
/// the same word. The reading is never silent: [`run_with`] prints the folders
/// back before anything is asked, so a name that happens to also be a folder
/// on the disk is visibly read as the folder.
pub(super) fn folders_and_names(args: &Args) -> (Vec<String>, Vec<String>) {
    let mut folders = Vec::new();
    let mut names = Vec::new();
    for raw in &args.positionals {
        if std::path::Path::new(raw).exists() {
            folders.push(raw.clone());
            continue;
        }
        let name = text::normalize(raw);
        if !name.is_empty() {
            names.push(name);
        }
    }
    (folders, names)
}

/// The names typed after the command, normalised, or empty for the whole shelf.
///
/// Read once for the whole run and handed to every pass, because a name given
/// to `aede fetch --discography mika` used to be **swallowed**: the pass ran
/// over the entire library and nothing said the word had been ignored. That is
/// the fault this program refuses everywhere else, and it was in four places
/// at once — the ordinary fetch was the only half that read them.
///
/// Folders are not names: see [`folders_and_names`], which is the one rule the
/// whole program reads a positional by.
pub(super) fn names_given(args: &Args) -> Vec<String> {
    folders_and_names(args).1
}

/// The folders typed after the command, and what the catalog holds under them.
///
/// A name and a folder narrow a run in two different ways — one asks *who*,
/// the other asks *where* — and every pass has to honour both. Which artists,
/// albums and tracks a folder holds is a walk over the catalog, so it is
/// worked out **once**, here, exactly as the names are: six passes working it
/// out separately is six chances for two of them to disagree about what
/// `~/Music/Alastis` means.
///
/// [`Default`] is the whole library, which is what the `waiting` counters ask
/// for: they report what a pass would find over everything, not over what
/// happened to be typed this time.
#[derive(Default)]
pub(super) struct Scope {
    /// The folders, canonical, as the catalog spells them; empty for the whole
    /// library.
    folders: Vec<String>,
    /// What they hold. `None` when no folder was given, which is not the same
    /// as a folder that holds nothing — the first reaches everything, the
    /// second reaches nothing at all.
    holds: Option<Held>,
}

/// No folders: the whole library, which is what most callers mean.
///
/// A `static` rather than a [`Default::default()`](Default) at each call site
/// because several callers want a `&'static Scope` — the `waiting` counters,
/// which report over everything, and the tests that build an [`Asked`] by
/// hand. It costs nothing: an empty `Vec` allocates nothing and a `static` is
/// never dropped.
pub(super) static EVERYTHING: Scope = Scope {
    folders: Vec::new(),
    holds: None,
};

/// The catalog under the folders.
#[derive(Default)]
struct Held {
    tracks: BTreeSet<Id>,
    releases: BTreeSet<Id>,
    /// Artists by their catalog key rather than their identifier: two of the
    /// passes read the attributed layer instead of the catalog, and a stored
    /// record carries a key and no identifier.
    artists: BTreeSet<String>,
    /// Labels credited on a release in scope, by their catalog key — the
    /// same reason `artists` is keyed rather than indexed.
    labels: BTreeSet<String>,
}

impl Scope {
    /// The scope the folders name, refusing one the catalog has never seen.
    ///
    /// That refusal is [`super::scope_from`]'s, the same one `check` and
    /// `playlist` make and for the same reason: a folder added since the last
    /// scan would otherwise produce a run with nothing to do and a cheerful
    /// line saying so, which reads as *there is nothing to fetch here* and is
    /// in fact *I have never heard of that folder*.
    pub(super) fn of(
        catalog: &Catalog,
        typed: &[String],
    ) -> Result<Scope, Box<dyn std::error::Error>> {
        if typed.is_empty() {
            return Ok(Scope::default());
        }
        let folders = super::scope_from(typed, catalog)?;
        let mut holds = Held::default();
        for track in &catalog.tracks {
            let Some(file) = catalog.file(track.file_id) else {
                continue;
            };
            if !super::in_scope(&file.path, &folders) {
                continue;
            }
            holds.tracks.insert(track.id);
            if let Some(release) = track.release_id {
                holds.releases.insert(release);
            }
        }
        // The album artist is credited on the record rather than on each of
        // its tracks, so a folder holding a record whose tracks name only the
        // guests would otherwise not reach the person whose record it is.
        for id in &holds.releases {
            let Some(artist) = catalog
                .release(*id)
                .and_then(|release| release.album_artist_id)
                .and_then(|id| catalog.artist(id))
            else {
                continue;
            };
            holds.artists.insert(artist.key.clone());
        }
        // Every label credited on a release in scope, the same derivation as
        // the artist above and for the same reason: a label is never itself
        // under a folder, only the records that name it are.
        for id in &holds.releases {
            let Some(release) = catalog.release(*id) else {
                continue;
            };
            for &label_id in &release.label_ids {
                if let Some(label) = catalog.label(label_id) {
                    holds.labels.insert(label.key.clone());
                }
            }
        }
        Ok(Scope {
            folders,
            holds: Some(holds),
        })
    }

    /// `true` when no folder was given, so the whole library is in reach.
    pub(super) fn is_empty(&self) -> bool {
        self.holds.is_none()
    }

    /// The folders, for a message that names them.
    pub(super) fn folders(&self) -> &[String] {
        &self.folders
    }

    pub(super) fn has_track(&self, id: Id) -> bool {
        match &self.holds {
            None => true,
            Some(held) => held.tracks.contains(&id),
        }
    }

    pub(super) fn has_release(&self, id: Id) -> bool {
        match &self.holds {
            None => true,
            Some(held) => held.releases.contains(&id),
        }
    }

    /// `true` when the artist filed under this key has something in reach.
    pub(super) fn has_artist(&self, key: &str) -> bool {
        match &self.holds {
            None => true,
            Some(held) => held.artists.contains(key),
        }
    }

    /// `true` when the label filed under this key is credited on something in
    /// reach.
    pub(super) fn has_label(&self, key: &str) -> bool {
        match &self.holds {
            None => true,
            Some(held) => held.labels.contains(key),
        }
    }
}

/// Whether this is an artist the library owns an album by, rather than an
/// invited musician or another contributor credited on one track.
pub(super) fn has_album(catalog: &Catalog, artist: Id) -> bool {
    catalog
        .releases
        .iter()
        .any(|release| release.album_artist_id == Some(artist))
}

/// How a run was narrowed, worded for a message; empty when it was not.
///
/// A name and a folder are two different questions, and a run narrowed by both
/// says both rather than running them together into a list the reader has to
/// sort out again.
pub(super) fn narrowing(names: &[String], scope: &Scope) -> String {
    match (names.is_empty(), scope.is_empty()) {
        (true, true) => String::new(),
        (false, true) => names.join(", "),
        (true, false) => scope.folders().join(", "),
        (false, false) => format!("{} under {}", names.join(", "), scope.folders().join(", ")),
    }
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
pub(super) fn nothing_named(wanted: &[String], scope: &Scope, but_for_full: usize) -> String {
    let named = narrowing(wanted, scope);
    match but_for_full {
        0 => format!("nothing here matches {named}"),
        _ => format!(
            "{} matching {named}, already done: --full asks again",
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
/// Fields nobody but one pass reads — `size`, `images`, `key` — sit here all
/// the same: they are things the reader arranged, which is what this is,
/// whether they typed them or exported them.
pub(super) struct Asked<'a> {
    /// The names typed after the command; empty means the whole shelf.
    pub names: &'a [String],
    /// The folders typed after the command, and what they hold; empty means
    /// the whole library.
    ///
    /// Beside the names rather than folded into them: a name asks *who* and a
    /// folder asks *where*, they narrow a run independently, and a pass given
    /// both honours both.
    pub scope: &'a Scope,
    /// `--full`: ask again about what is already held.
    pub again: bool,
    /// `--dry-run`: say what would happen and do none of it.
    pub dry_run: bool,
    /// `--size`: how large an image to keep.
    pub size: aede_core::coverart::Size,
    /// `--images`: keep the pictures that are not the cover.
    pub images: bool,
    /// Which parts of a Fanart.tv answer this run should keep.
    pub fanart: FanartOptions,
    /// Which language the prose is wanted in, most wanted first.
    ///
    /// `--lang` when it was given, the shell's own locale otherwise, and
    /// English last in either case — for a great many artists it is the only
    /// article there is, and being *last* means it never displaces a language
    /// the reader actually asked for. Gathered here rather than read inside the
    /// pass, like the key below and for the same reason.
    pub langs: Vec<String>,
    /// The AcoustID key, from `AEDE_ACOUSTID_KEY`, when this machine has one.
    ///
    /// Read here rather than inside the pass that needs it, for the reason
    /// [`aede_core::acoustid::key`] gives: the process environment is shared by
    /// every thread, so a pass that reaches for it cannot be handed a different
    /// answer by a test. Gathered at the edge, like everything else in here.
    pub key: Option<String>,
    /// The Fanart.tv key, from `AEDE_FANARTTV_KEY`, when this machine has one.
    ///
    /// Gathered here for the same reason [`Asked::key`] is — see
    /// [`aede_core::fanarttv::key`]. `--portraits` and `--logos` are the only
    /// passes that read it — a portrait tries Wikidata first and needs no key
    /// at all, a logo has no such alternative and needs one for every artist.
    pub portrait_key: Option<String>,
}

/// The independently selectable image families in a Fanart.tv music answer.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FanartOptions {
    /// The broad `--fanart` pass was explicitly requested.
    pub all: bool,
    pub logo: bool,
    pub label_logo: bool,
    pub portrait: bool,
    pub background: bool,
    pub banner: bool,
    pub album_cover: bool,
    pub cdart: bool,
}

impl FanartOptions {
    fn from_args(args: &Args) -> Self {
        let all = args.has("fanart");
        Self {
            all,
            logo: args.has("logos") || (all && !args.has("no-logo")),
            label_logo: args.has("logos") || (all && !args.has("no-label-logo")),
            portrait: all && !args.has("no-portrait"),
            background: all && !args.has("no-background"),
            banner: args.has("banners") || (all && !args.has("no-banner")),
            album_cover: all && !args.has("no-album-cover"),
            cdart: all && !args.has("no-cdart"),
        }
    }
}

/// Prints the list a pass would have asked about, and says nothing was.
///
/// **`--dry-run` is a promise, and two of the four passes were not keeping it.**
/// `--covers` and `--identify` stopped; `--summaries` and `--discography` read
/// the flag, said nothing, and went to the network — which for summaries is two
/// requests per artist against Wikimedia by somebody who had just said *ask
/// nothing*. The rule was stated in prose on the first pass that honoured it,
/// which is exactly how the third and fourth came not to: **a rule stated in a
/// comment is a rule the next caller does not have.**
///
/// So the sentinel is a function. A pass prints its own plan — the titles, the
/// artists, whatever it is about — and then calls this, which prints the one
/// line and answers whether to stop. Four call sites, one wording, and an
/// end-to-end test walks all of them.
pub(super) fn asked_nothing(asked: &Asked, what: &[String]) -> bool {
    if !asked.dry_run {
        return false;
    }
    for one in what {
        println!("  {}", ui::dim(one));
    }
    println!("  {}", ui::dim("nothing was asked: --dry-run"));
    true
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
    /// The words, for tracks that have none — see [`super::lyrics`].
    Lyrics,
    /// What AcoustID hears in the files that have been fingerprinted.
    Identify,
    /// Work relationships of recordings already identified in local tags.
    Recordings,
    /// A picture of the artist, from Wikidata or Fanart.tv — see
    /// [`super::portraits`].
    Portraits,
    /// The artist's logo, from Fanart.tv — see [`super::logos`].
    Logos,
    /// A label's own MusicBrainz identifier, closing the gap passive capture
    /// leaves — see [`super::labels`].
    Labels,
}

impl Pass {
    /// The passes asked for, in the order they will run.
    fn asked_for(args: &Args) -> Vec<Pass> {
        [
            ("summaries", Pass::Summaries),
            ("discography", Pass::Discography),
            ("covers", Pass::Covers),
            ("lyrics", Pass::Lyrics),
            ("identify", Pass::Identify),
            ("recordings", Pass::Recordings),
            ("labels", Pass::Labels),
            ("portraits", Pass::Portraits),
            ("logos", Pass::Logos),
            ("fanart", Pass::Logos),
        ]
        .into_iter()
        .filter(|(flag, _)| args.has(flag))
        .map(|(_, pass)| pass)
        .fold(Vec::new(), |mut passes, pass| {
            if !passes.contains(&pass) {
                passes.push(pass);
            }
            passes
        })
    }

    /// The option that asks for it, for a message to name.
    fn option(self) -> &'static str {
        match self {
            Pass::Summaries => "--summaries",
            Pass::Discography => "--discography",
            Pass::Covers => "--covers",
            Pass::Lyrics => "--lyrics",
            Pass::Identify => "--identify",
            Pass::Recordings => "--recordings",
            Pass::Portraits => "--portraits",
            Pass::Logos => "--logos",
            Pass::Labels => "--labels",
        }
    }

    /// Whether it needs the catalog, which decides whether one is loaded.
    ///
    /// `--summaries` does not: its input is the wikidata link already stored,
    /// and failing on a missing catalog would be a refusal with no reason
    /// behind it. Loading one anyway "for symmetry" would break that.
    ///
    /// A **folder** overrides this, in [`run_with`] rather than here: the
    /// catalog is the only thing that can say which artists a folder holds, so
    /// `aede fetch --summaries ~/Music/Alastis` needs one even though the pass
    /// alone does not. The pass still never reads it — it is handed the answer.
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
///
/// One argument per thing a pass might need to do its work — network, retry,
/// storage, and where and whose library this is — because folding them into
/// a context struct would still need one field per parameter and one
/// accessor per call site, for no reader's benefit.
#[allow(clippy::too_many_arguments)]
fn second_passes(
    passes: &[Pass],
    args: &Args,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    data_dir: &std::path::Path,
    asked: &Asked,
    catalog: Option<&Catalog>,
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

    for pass in passes {
        match pass {
            Pass::Summaries => {
                super::summaries::run(transport, backoff, &asked.langs, held, path, asked)?;
            }
            Pass::Discography => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::discography::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Covers => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::covers::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Lyrics => {
                let catalog = catalog.expect("a catalog was loaded for it");
                // The only pass that writes nothing into the attributed layer:
                // its answer is a file beside the music, which the next scan
                // discovers, exactly as it would one put there by hand.
                super::lyrics::run(args, catalog, transport, backoff, asked)?;
            }
            Pass::Identify => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::identify::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Portraits => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::portraits::run(
                    catalog,
                    transport,
                    backoff,
                    held,
                    path,
                    data_dir,
                    asked.portrait_key.as_deref(),
                    asked,
                )?;
            }
            Pass::Logos => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::logos::run(
                    catalog,
                    transport,
                    backoff,
                    held,
                    path,
                    data_dir,
                    asked.portrait_key.as_deref(),
                    asked,
                )?;
            }
            Pass::Labels => {
                let catalog = catalog.expect("a catalog was loaded for it");
                super::labels::run(catalog, transport, backoff, held, path, asked)?;
            }
            Pass::Recordings => {
                let catalog = catalog.expect("a catalog was loaded for it");
                recording_links(catalog, transport, backoff, held, path, asked, args)?;
            }
        }
    }
    Ok(())
}

/// Retrieves MusicBrainz work relationships for recordings local tags already
/// identify. No title search is offered: without a recording MBID there is no
/// safe question to ask.
fn recording_links(
    catalog: &Catalog,
    transport: &mut dyn Ask,
    backoff: &[std::time::Duration],
    held: &mut sources::Sources,
    path: &std::path::Path,
    asked: &Asked,
    args: &Args,
) -> Res {
    let mut targets = Vec::new();
    for recording in &catalog.recordings {
        let Some(mbid) = recording.mbid.as_deref() else {
            continue;
        };
        let Some(&track_id) = recording
            .track_ids
            .iter()
            .find(|&&track_id| asked.scope.has_track(track_id))
        else {
            continue;
        };
        let Some(entity) = EntityRef::of(catalog, EntityKind::Track, track_id) else {
            continue;
        };
        if !asked.again && held.get(&entity, sources::MUSICBRAINZ).is_some() {
            continue;
        }
        if !reaches(asked.names, &[recording.title.as_str()]) {
            continue;
        }
        targets.push((entity, recording.title.as_str(), mbid));
    }
    println!("{}", ui::section("Recording relationships"));
    if targets.is_empty() {
        println!("  {}", ui::dim("no identified recording is waiting"));
        return Ok(());
    }
    if asked.dry_run {
        for (_, title, _) in &targets {
            println!("  {}", ui::dim(title));
        }
        println!("  {}", ui::dim("nothing was asked: --dry-run"));
        return Ok(());
    }
    let total = targets.len();
    let estimate_ms = total as u64 * musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    println!(
        "  {} at one request per second, about {}",
        ui::plural(total, "recording"),
        ui::long_duration(estimate_ms)
    );
    if total > CONFIRM_ABOVE && !super::confirmed(args, "ask about all of them")? {
        println!("  {}", ui::dim("nothing was asked"));
        return Ok(());
    }
    let (mut stored, mut refused, mut failed) = (0, 0, 0);
    for (index, (entity, title, mbid)) in targets.into_iter().enumerate() {
        print!("\r  asking: {}/{}", index + 1, total);
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let url = format!(
            "{}/recording/{mbid}?fmt=json&inc={}",
            musicbrainz::WEB_SERVICE,
            musicbrainz::RECORDING_INCLUDES
        );
        match ask_with_backoff(transport, &url, backoff) {
            Ok(answer) => match musicbrainz::recording(&answer) {
                Some(candidate) => {
                    held.set(SourceRecord {
                        key: entity.key,
                        source: sources::MUSICBRAINZ.to_string(),
                        source_id: Some(candidate.mbid),
                        fetched_at: clock::now_seconds(),
                        confidence: sources::Confidence::Identified,
                        facts: Facts::Track(candidate.facts),
                    });
                    sources::save(held, path)?;
                    stored += 1;
                }
                None => {
                    refused += 1;
                    eprintln!(
                        "  {} {title}: no readable MusicBrainz recording",
                        ui::yellow("?")
                    );
                }
            },
            Err(Refusal::RateLimited) => {
                println!();
                sources::save(held, path)?;
                return Err(
                    "MusicBrainz is rate limiting requests; saved answers are safe, try later"
                        .into(),
                );
            }
            Err(error) => {
                failed += 1;
                eprintln!("\n  {} {title}: {error}", ui::red("×"));
            }
        }
    }
    println!();
    println!(
        "{} {stored} stored, {refused} left alone, {failed} failed",
        ui::green("→")
    );
    Ok(())
}

/// The whole of the command except reaching the network.
pub fn run(args: &Args, transport: &mut dyn Ask) -> Res {
    run_with(args, transport, &RETRY_AFTER)
}

/// [`run`], with the waits made explicit so a test does not have to sit
/// through them.
pub fn run_with(args: &Args, transport: &mut dyn Ask, backoff: &[std::time::Duration]) -> Res {
    let data_dir = super::data_dir(args);
    let path = sources::sources_path(&data_dir);
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
    if !args.has("logos") && !args.has("fanart") && args.has("banners") {
        return Err("--banners belongs to a Fanart.tv pass: aede fetch --logos --banners".into());
    }
    let exclusions = [
        "no-logo",
        "no-label-logo",
        "no-portrait",
        "no-background",
        "no-banner",
        "no-album-cover",
        "no-cdart",
    ];
    if !args.has("fanart")
        && let Some(option) = exclusions.iter().find(|option| args.has(option))
    {
        return Err(format!(
            "--{option} belongs to the complete pass: aede fetch --fanart --{option}"
        )
        .into());
    }
    for (positive, negative) in [("logos", "no-logo"), ("banners", "no-banner")] {
        if args.has("fanart") && args.has(positive) && args.has(negative) {
            return Err(format!("--{positive} and --{negative} ask for opposite things").into());
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
    // What to ask about: the names given, the folders given, or every artist
    // in the library. Read here rather than inside the ordinary run, because
    // every pass honours them now — `aede fetch --discography mika` used to
    // swallow the word, and `aede fetch --lyrics ~/Music/Alastis` the path.
    let (folders, wanted) = folders_and_names(args);
    let passes = Pass::asked_for(args);

    // Loaded once for the whole run, and only when something needs it. An
    // ordinary fetch always does; a pass may not; and a **folder** always
    // does, whatever the pass, because the catalog is the only thing that can
    // say what a folder holds.
    let catalog = match passes.is_empty()
        || !folders.is_empty()
        || passes.iter().any(|pass| pass.needs_the_catalog())
    {
        true => Some(super::load(args)?),
        false => None,
    };
    let scope = match &catalog {
        Some(catalog) => Scope::of(catalog, &folders)?,
        // A folder would have loaded one, so there is no folder to place.
        None => Scope::default(),
    };
    // Said before anything is asked, and on every run that gives one: reading
    // a positional as a folder is a *reading*, and a reading the person cannot
    // see is one they cannot correct.
    if !scope.is_empty() {
        println!(
            "  {}",
            ui::dim(&format!(
                "only what is under {}",
                scope.folders().join(", ")
            ))
        );
    }

    // Read once for the whole run, and before anything is asked: a width the
    // archive does not generate must be refused before a summaries pass has
    // spent ten minutes on the network for it.
    let shell_locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .ok();
    let asked = Asked {
        names: &wanted,
        scope: &scope,
        again: args.has("full"),
        dry_run: args.has("dry-run"),
        key: aede_core::acoustid::key(),
        portrait_key: aede_core::fanarttv::key(),
        // `--lang` is a statement of intent and the locale is a guess about
        // one, so the option wins. Neither is read anywhere but here.
        langs: super::summaries::preferred_langs(match args.value("lang") {
            Some(asked) => Some(asked),
            None => shell_locale.as_deref(),
        }),
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
        fanart: FanartOptions::from_args(args),
    };

    if !passes.is_empty() {
        return second_passes(
            &passes,
            args,
            transport,
            backoff,
            &mut held,
            &path,
            &data_dir,
            &asked,
            catalog.as_ref(),
        );
    }

    let catalog = catalog.expect("an ordinary fetch always loads one");

    // No `--limit` here on purpose: everywhere else in this program it means
    // "show a window of the result", and bounding how much work is done is a
    // different thing wearing the same word. Naming artists narrows the run.
    let mut targets: Vec<(EntityRef, String, Option<String>)> = Vec::new();
    for artist in &catalog.artists {
        if !has_album(&catalog, artist.id) {
            continue;
        }
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
        // A folder narrows by where the music is rather than by what it is
        // called, and the two are asked together: `aede fetch ozzy ~/Music/80s`
        // is the artist, on that shelf.
        if !scope.has_artist(&artist.key) {
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
    let albums = super::releases::targets(&catalog, &held, &wanted, &scope, args.has("full"));

    if targets.is_empty() && albums.is_empty() {
        println!("{}", ui::section("Fetch"));
        println!(
            "  {}",
            ui::dim(match wanted.is_empty() && scope.is_empty() {
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
        offer_portraits(&catalog, &held, &data_dir);
        offer_logos(&catalog, &held, &data_dir);
        offer_labels(&catalog, &held);
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
    let mut pending = queue(&targets);
    let mut done = 0usize;
    while let Some((item @ (entity, name, mbid), retried)) = pending.pop_front() {
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
            Err(other) if worth_deferring(&other) && !retried => {
                // Not shown, not counted: the same name goes back to the end
                // of the queue instead, on the theory that whatever kept the
                // service from answering will often have passed by the time
                // everything else here has had its turn.
                pending.push_back((item, true));
                continue;
            }
            Err(other) => {
                failed += 1;
                done += 1;
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
        done += 1;
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
    offer_portraits(&catalog, &held, &data_dir);
    offer_logos(&catalog, &held, &data_dir);
    offer_labels(&catalog, &held);
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

/// Names the portraits pass, when there is an artist to ask about.
///
/// Reads the Fanart.tv key the same way [`run_with`] does, so a door counted
/// here without a key set is a door the pass itself would also find, not an
/// offer that promises more than `--portraits` will actually try.
fn offer_portraits(
    catalog: &aede_core::model::Catalog,
    held: &sources::Sources,
    data_dir: &std::path::Path,
) {
    let door = super::portraits::waiting(
        catalog,
        held,
        data_dir,
        aede_core::fanarttv::key().as_deref(),
    );
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} with no picture yet — aede fetch --portraits looks for one",
            ui::plural(door, "artist")
        ))
    );
}

/// Names the logos pass, when there is an artist to ask about.
///
/// Reads the Fanart.tv key the same way [`offer_portraits`] does, for the
/// same reason: a door counted here without a key set is a door the pass
/// itself would also find.
fn offer_logos(
    catalog: &aede_core::model::Catalog,
    held: &sources::Sources,
    data_dir: &std::path::Path,
) {
    let door = super::logos::waiting(
        catalog,
        held,
        data_dir,
        aede_core::fanarttv::key().as_deref(),
    );
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} with no logo yet — aede fetch --logos looks for one",
            ui::plural(door, "artist")
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

/// Names the labels pass, when there is a label with no MusicBrainz
/// identifier of its own yet.
fn offer_labels(catalog: &aede_core::model::Catalog, held: &sources::Sources) {
    let door = super::labels::waiting(catalog, held);
    if door == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} with no MusicBrainz identifier of their own yet — \
             aede fetch --labels looks for one",
            ui::plural(door, "label")
        ))
    );
}

/// Above this many requests, the run is long enough to be worth agreeing to.
///
/// Below it there is nothing to decide — twenty seconds is not a commitment —
/// and a confirmation asked every time is a confirmation nobody reads.
pub(super) const CONFIRM_ABOVE: usize = 20;

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

/// [`ask_with_backoff`], for bytes.
///
/// Promoted here once a second caller needed it — `fetch --covers` first,
/// `fetch --portraits` after — rather than kept as two copies that would
/// eventually differ.
pub(super) fn ask_bytes(
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

/// Turns a plain list into a queue where nothing has been retried yet.
///
/// A queue rather than a plain list because of what a caller does with a
/// [`Refusal`] that [`worth_deferring`] returns `true` for: push the same
/// target back onto the end, with the flag now `true`, instead of reporting
/// it. Everything already asked before it gets its turn first, which is
/// usually enough time for a service that merely stumbled to have recovered —
/// so asking it again is the exception rather than the rule. A target only
/// gets that once: `true` on the way back in means the next refusal is final.
pub(super) fn queue<T>(targets: &[T]) -> std::collections::VecDeque<(&T, bool)> {
    targets.iter().map(|target| (target, false)).collect()
}

/// Whether a refusal is worth asking again later rather than reporting now.
///
/// Only [`Refusal::Unreachable`] qualifies: no route, no name, a timeout are
/// all a property of one attempt, and often gone by the time everything else
/// in the queue has had its turn. A `404`, a rate limit that has used up its
/// own retries, or an answer that was not JSON will not read differently for
/// having waited — deferring those would only delay the same report.
pub(super) fn worth_deferring(refusal: &Refusal) -> bool {
    matches!(refusal, Refusal::Unreachable(_))
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
