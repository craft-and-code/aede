//! A query language over the catalog and over what the user wrote about it.
//!
//! Options compose by AND and by nothing else. That is the ceiling no number of
//! new flags ever raises: there is no `--genre metal OR --genre jazz`, no
//! "everything except this label", no "between 1990 and 1999". A grammar has
//! all three for free, and having one is what turns a saved query into a smart
//! collection — which, since a selection is already what `--csv`, `--m3u` and
//! M3's queue consume, is playable the day it is written.
//!
//! ```text
//! genre:metal year:1990..1999 rating:>=4 -label:earache
//! (artist:ozzy OR artist:dio) loved
//! album.rating:5 played:0
//! work:"War Pigs" instrument:guitar
//! guest:"Zakk Wylde" with:"Ozzy Osbourne"
//! ```
//!
//! **It is an interface, not a storage engine.** Defined on its own it works
//! today over the vectors in memory and tomorrow over SQL. Defined as "whatever
//! the database makes easy" it would arrive late and shaped by the wrong
//! concerns.
//!
//! Everything evaluates against a **track**, because a track is the finest
//! grain and every coarser answer is a fold of it: the albums matching a query
//! are the albums of the tracks matching it. One evaluator, not five.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Catalog, EntityKind, Id};
use crate::text;
use crate::user::{EntityRef, UserData};

/// A parsed query, ready to be run against any catalog.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// Matches everything, which is what an empty expression means.
    All,
    /// Every part must match.
    And(Vec<Query>),
    /// At least one part must match.
    Or(Vec<Query>),
    /// The part must not match.
    Not(Box<Query>),
    /// One condition on one field.
    Term(Term),
}

/// One condition: a field, and what it is being asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// Which field, already resolved to something known.
    pub field: Field,
    /// What is being asked of it.
    pub test: Test,
}

/// What a term asks of a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    /// The text contains this, compared normalized.
    Contains(String),
    /// The text is exactly this, compared normalized.
    Is(String),
    /// The number satisfies the comparison.
    Compare(Compare, f64),
    /// The number falls in the range, either end open.
    Between(Option<f64>, Option<f64>),
    /// The flag is set.
    Set,
    /// The flag is not set, which `lossless:false` and `loved:no` ask for.
    Unset,
}

/// How two numbers are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    /// `=`
    Equal,
    /// `>`
    Greater,
    /// `>=`
    AtLeast,
    /// `<`
    Less,
    /// `<=`
    AtMost,
}

/// Which value of a track a term reads.
///
/// The names are the ones typed, and the dotted ones say **where** an opinion
/// was written: `rating` is the track's own, `album.rating` the album's,
/// `artist.rating` the artist's. Without that distinction "rated five stars"
/// would be a different claim depending on where the user happened to put it,
/// and no message could say which was meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Track title.
    Title,
    /// Any artist credited on the track, in any role.
    Artist,
    /// Album title.
    Album,
    /// The canonical recorded performance behind the local track.
    Recording,
    /// An identified composition, its explicit parent, or this file's WORK tag.
    Work,
    /// A movement title or number written in the local file's tags.
    Movement,
    /// The release group shared by this edition and its remasters.
    ReleaseGroup,
    /// The album's own artist.
    AlbumArtist,
    /// A genre carried by the track or by its album.
    Genre,
    /// A label the album came out on.
    Label,
    /// The comment tag, as the tagger wrote it.
    Comment,
    /// The words: from the tag that carries them, or from the `.lrc` beside
    /// the file.
    Lyrics,
    /// The file's path.
    Path,
    /// The codec: flac, mp3, opus…
    Codec,
    /// Year of the album.
    Year,
    /// Playing time, in milliseconds.
    Duration,
    /// Size on disk, in bytes.
    Size,
    /// Bitrate in kbps.
    Bitrate,
    /// Sample rate in Hz.
    SampleRate,
    /// `true` for a lossless codec.
    Lossless,
    /// `true` when the album is one several artists share.
    Compilation,
    /// Stars given, on the entity named by the scope.
    Rating(Scope),
    /// A favourite, on the entity named by the scope.
    Loved(Scope),
    /// A favourite, wherever it was actually written: the track itself, or
    /// failing that its album, or failing that its artist.
    ///
    /// What a bare `loved` asks. Unlike [`Field::Rating`], a favourite is a
    /// blunter signal — closer to "this matters to me" than to a precise
    /// score — so loving a whole album should not have to be repeated one
    /// track at a time. `track.loved` still reaches the precise, track-only
    /// question, exactly as `album.loved` and `artist.loved` already do for
    /// theirs.
    LovedAnywhere,
    /// A free label, on the entity named by the scope.
    Tag(Scope),
    /// A note was written, and contains this text.
    Note(Scope),
    /// How many times it was played.
    Played,
    /// An artist audible on the track, in any performing role.
    ///
    /// The class `model::is_performing_role` draws: singing one guest verse
    /// counts, having written the words does not. `artist --with` asks exactly
    /// this question, and no pile of role fields ORed together would say it as
    /// plainly.
    Performing,
    /// An artist credited in one named role.
    ///
    /// `artist:` matches any credit, in any role, which is what makes
    /// `artist:ozzy artist:"zakk wylde"` already mean "both are on it". This
    /// asks the finer question the graph was built for: *who did what*.
    Credit(&'static str),
    /// An instrument or relationship attribute on a credit.
    Instrument,
    /// A performing artist appearing on another artist's non-compilation release.
    Guest,
    /// A performing artist appearing on a compilation.
    CompilationArtist,
    /// An artist with a non-performing contribution such as composer or producer.
    Contributor,
    /// A performing artist sharing the track with at least one other performer.
    Collaborator,
    /// The whole track, for a bare word with no field.
    Anything,
}

/// Which entity an opinion was written on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The track itself.
    Track,
    /// The album it belongs to.
    Album,
    /// Its main artist.
    Artist,
}

/// Why a query could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryError {
    /// What went wrong, worded for the person who typed it.
    pub message: String,
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for QueryError {}

fn error(message: impl Into<String>) -> QueryError {
    QueryError {
        message: message.into(),
    }
}

mod parsing;
pub use parsing::parse;

/// Every field, by the name it is typed under.
const FIELD_NAMES: &[(&str, Field)] = &[
    ("title", Field::Title),
    ("artist", Field::Artist),
    ("album", Field::Album),
    ("albumartist", Field::AlbumArtist),
    ("recording", Field::Recording),
    ("work", Field::Work),
    ("movement", Field::Movement),
    ("releasegroup", Field::ReleaseGroup),
    ("release-group", Field::ReleaseGroup),
    ("release_group", Field::ReleaseGroup),
    ("genre", Field::Genre),
    ("label", Field::Label),
    ("comment", Field::Comment),
    ("lyrics", Field::Lyrics),
    ("path", Field::Path),
    ("codec", Field::Codec),
    ("format", Field::Codec),
    ("year", Field::Year),
    ("duration", Field::Duration),
    ("length", Field::Duration),
    ("size", Field::Size),
    ("bitrate", Field::Bitrate),
    ("samplerate", Field::SampleRate),
    ("lossless", Field::Lossless),
    ("compilation", Field::Compilation),
    ("played", Field::Played),
    ("rating", Field::Rating(Scope::Track)),
    ("album.rating", Field::Rating(Scope::Album)),
    ("artist.rating", Field::Rating(Scope::Artist)),
    ("loved", Field::LovedAnywhere),
    ("track.loved", Field::Loved(Scope::Track)),
    ("album.loved", Field::Loved(Scope::Album)),
    ("artist.loved", Field::Loved(Scope::Artist)),
    ("tag", Field::Tag(Scope::Track)),
    ("album.tag", Field::Tag(Scope::Album)),
    ("artist.tag", Field::Tag(Scope::Artist)),
    // One field per role, so that the credit table can be asked its own
    // question: `composer:bach performer:gould` is what a graph is for, and
    // what no pile of options was ever going to express. A role arriving from
    // MusicBrainz at M1 needs one row here.
    ("composer", Field::Credit("composer")),
    ("lyricist", Field::Credit("lyricist")),
    ("producer", Field::Credit("producer")),
    ("engineer", Field::Credit("engineer")),
    ("performer", Field::Credit("performer")),
    ("conductor", Field::Credit("conductor")),
    ("orchestra", Field::Credit("orchestra")),
    ("choir", Field::Credit("choir")),
    ("ensemble", Field::Credit("ensemble")),
    ("soloist", Field::Credit("soloist")),
    ("remixer", Field::Credit("remixer")),
    ("featured", Field::Credit("featured")),
    ("mainartist", Field::Credit("main")),
    ("performing", Field::Performing),
    ("instrument", Field::Instrument),
    ("guest", Field::Guest),
    ("compilationartist", Field::CompilationArtist),
    ("compilation-artist", Field::CompilationArtist),
    ("contributor", Field::Contributor),
    ("collaborator", Field::Collaborator),
    ("with", Field::Collaborator),
    ("note", Field::Note(Scope::Track)),
    ("album.note", Field::Note(Scope::Album)),
    ("artist.note", Field::Note(Scope::Artist)),
];

fn field_named(name: &str) -> Option<Field> {
    let wanted = name.trim().to_lowercase();
    FIELD_NAMES
        .iter()
        .find(|(n, _)| *n == wanted)
        .map(|(_, f)| f.clone())
}

fn is_numeric(field: &Field) -> bool {
    matches!(
        field,
        Field::Year
            | Field::Duration
            | Field::Size
            | Field::Bitrate
            | Field::SampleRate
            | Field::Played
            | Field::Rating(_)
    )
}

fn is_flag(field: &Field) -> bool {
    matches!(
        field,
        Field::Lossless | Field::Compilation | Field::Loved(_) | Field::LovedAnywhere
    )
}

/// The same question, asked of the album or the artist instead of the track.
///
/// Every field the *user* writes carries a scope, and a bare `rating`, `tag`
/// or `note` means the track's own. That is deliberate — five stars on an
/// artist is not five stars on a track — but it makes one answer badly
/// misleading: somebody who rated an **album** types `rating`, is told
/// nothing matches, and concludes the feature is broken. It is not; they
/// asked a different question from the one they meant. (`loved` is the one
/// exception: see [`Field::LovedAnywhere`], which already looks past the
/// track, so there is nothing here for it to rescope.)
///
/// So a caller that gets an empty result can ask the same question again at
/// another scope, and — if *that* answers — say which scope holds what was
/// written. The suggestion is only ever a suggestion: the query itself keeps
/// meaning exactly what it says.
///
/// Fields that carry no scope are left alone, so a mixed expression such as
/// `genre:metal rating` is rescoped only where rescoping means something.
pub fn rescoped(query: &Query, scope: Scope) -> Query {
    match query {
        Query::All => Query::All,
        Query::And(parts) => Query::And(parts.iter().map(|p| rescoped(p, scope)).collect()),
        Query::Or(parts) => Query::Or(parts.iter().map(|p| rescoped(p, scope)).collect()),
        Query::Not(inner) => Query::Not(Box::new(rescoped(inner, scope))),
        Query::Term(term) => Query::Term(Term {
            field: match &term.field {
                Field::Rating(_) => Field::Rating(scope),
                Field::Loved(_) => Field::Loved(scope),
                Field::Tag(_) => Field::Tag(scope),
                Field::Note(_) => Field::Note(scope),
                other => other.clone(),
            },
            test: term.test.clone(),
        }),
    }
}

/// `true` when the expression asks about something the user wrote, at the
/// track's own scope — the case where [`rescoped`] has anything to offer.
pub fn asks_about_the_track_itself(query: &Query) -> bool {
    match query {
        Query::All => false,
        Query::And(parts) | Query::Or(parts) => parts.iter().any(asks_about_the_track_itself),
        Query::Not(inner) => asks_about_the_track_itself(inner),
        Query::Term(term) => matches!(
            term.field,
            Field::Rating(Scope::Track)
                | Field::Loved(Scope::Track)
                | Field::Tag(Scope::Track)
                | Field::Note(Scope::Track)
        ),
    }
}

/// Fields a bare mention can ask about: "is there one at all?"
///
/// Wider than [`is_flag`], and the two were one predicate until that turned out
/// to answer two different questions with one answer. `is_flag` says whether
/// `field:true` means a yes or a no; this says whether the field's *name*,
/// written alone, is a question. They coincide for `lossless` and `loved`, and
/// come apart exactly where it matters:
///
/// - `note:vinyle` searches inside the note, so `note` is not a yes/no field;
/// - `note` alone can only mean "the ones I have written a note on", because
///   nobody searches a music library for the word "note".
///
/// Before they were separated there was **no way at all** to ask which things
/// carried a note, a tag or a rating: a bare `note` fell through to a text
/// search for the word, `note:true` searched for the word "true", and the
/// fallback in [`flag_of`] that exists precisely to answer this — "any other
/// field used as a bare flag asks whether it holds anything" — was unreachable.
///
/// The cost is that a bare `note`, `tag` or `rating` can no longer be a text
/// search for those three words. Written with a field they still are:
/// `title:note` finds the word.
fn asks_whether_it_holds_anything(field: &Field) -> bool {
    is_flag(field) || matches!(field, Field::Rating(_) | Field::Note(_) | Field::Tag(_))
}

/// Fields whose values come from a closed list the library holds.
///
/// Asking for a genre that exists and holds nothing, and asking for a genre
/// nobody ever heard of, are two different questions and deserve two different
/// answers — the same distinction `artists --role` already draws. A grammar
/// that answered "nothing matches" to both would be a step backwards from the
/// options it is meant to replace.
fn closed_vocabulary(field: &Field) -> Option<&'static str> {
    Some(match field {
        Field::Genre => "genre",
        Field::Label => "label",
        Field::Credit(_)
        | Field::Performing
        | Field::Guest
        | Field::CompilationArtist
        | Field::Contributor
        | Field::Collaborator => "artist",
        _ => return None,
    })
}

/// Values a query names that the library has never heard of.
///
/// Returned rather than raised, so the caller decides: a command refuses, and
/// a saved collection listing shows the row anyway.
pub fn unknown_values(query: &Query, context: &Context) -> Vec<(String, String)> {
    let mut found = Vec::new();
    collect_unknown(query, context, &mut found);
    found
}

fn collect_unknown(query: &Query, context: &Context, out: &mut Vec<(String, String)>) {
    match query {
        Query::All => {}
        Query::And(parts) | Query::Or(parts) => {
            for part in parts {
                collect_unknown(part, context, out);
            }
        }
        Query::Not(inner) => collect_unknown(inner, context, out),
        Query::Term(term) => {
            let Some(what) = closed_vocabulary(&term.field) else {
                return;
            };
            let wanted = match &term.test {
                Test::Contains(value) | Test::Is(value) => text::normalize(value),
                _ => return,
            };
            if wanted.is_empty() {
                return;
            }
            let known = match &term.field {
                Field::Genre => context
                    .catalog
                    .genres
                    .iter()
                    .any(|g| g.key.contains(&wanted)),
                Field::Label => context.catalog.labels.iter().any(|l| {
                    l.key.contains(&wanted)
                        || l.mbid
                            .as_deref()
                            .is_some_and(|mbid| text::normalize(mbid).contains(&wanted))
                        || context
                            .sourced
                            .label_mbids
                            .get(&l.id)
                            .is_some_and(|mbid| text::normalize(mbid).contains(&wanted))
                }),
                // A role field names a person, and a person who is in the
                // library but never credited that way is an empty result, not
                // a misunderstanding.
                Field::Credit(_)
                | Field::Performing
                | Field::Guest
                | Field::CompilationArtist
                | Field::Contributor
                | Field::Collaborator => {
                    context.catalog.artists.iter().any(|a| {
                        a.key.contains(&wanted)
                            || a.mbid
                                .as_deref()
                                .is_some_and(|mbid| text::normalize(mbid) == wanted)
                    }) || context
                        .sourced
                        .credits
                        .values()
                        .chain(context.sourced.edition_credits.values())
                        .flatten()
                        .any(|credit| {
                            text::normalize(&credit.artist_name).contains(&wanted)
                                || text::normalize(&credit.artist_mbid) == wanted
                                || credit
                                    .credited_as
                                    .as_deref()
                                    .is_some_and(|name| text::normalize(name).contains(&wanted))
                        })
                }
                _ => true,
            };
            if !known {
                out.push((what.to_string(), wanted));
            }
        }
    }
}

/// The field names, for a help message or a completion.
pub fn field_names() -> Vec<&'static str> {
    FIELD_NAMES.iter().map(|(name, _)| *name).collect()
}

// --------------------------------------------------------------------------
// Running a query
// --------------------------------------------------------------------------

mod indexes;
mod sorting;
pub use sorting::{Sort, SortKey, sort, sort_key_names};

/// Everything needed to answer, gathered once rather than per track.
pub struct Context<'a> {
    /// The library.
    pub catalog: &'a Catalog,
    /// What the user wrote about it.
    pub data: &'a UserData,
    /// Whose opinions count.
    pub owner: &'a str,
    /// Certain external relationships, indexed once for this evaluation.
    sourced: SourcedRelations,
    local: indexes::LocalIndexes<'a>,
}

impl<'a> Context<'a> {
    /// Creates a query context over local tags and user data.
    pub fn new(catalog: &'a Catalog, data: &'a UserData, owner: &'a str) -> Context<'a> {
        Context {
            catalog,
            data,
            owner,
            sourced: SourcedRelations::default(),
            local: indexes::LocalIndexes::new(catalog, data, owner),
        }
    }

    /// Adds externally sourced relationships that are certain enough to
    /// traverse. Approximate matches remain evidence and never silently become
    /// query results.
    pub fn with_sources(mut self, sources: &crate::sources::Sources) -> Context<'a> {
        self.sourced = SourcedRelations::new(self.catalog, sources);
        self
    }
}

#[derive(Default)]
struct SourcedRelations {
    works: BTreeMap<Id, Vec<crate::sources::WorkLink>>,
    credits: BTreeMap<Id, Vec<crate::sources::CreditLink>>,
    edition_credits: BTreeMap<Id, Vec<crate::sources::CreditLink>>,
    label_mbids: BTreeMap<Id, String>,
}

impl SourcedRelations {
    fn new(catalog: &Catalog, sources: &crate::sources::Sources) -> SourcedRelations {
        let mut indexed = SourcedRelations::default();
        for link in sources
            .work_links(catalog)
            .into_iter()
            .filter(|link| link.trusted)
        {
            let works = indexed.works.entry(link.recording_id).or_default();
            if !works.contains(&link.work) {
                works.push(link.work);
            }
        }
        for link in sources
            .credit_links(catalog)
            .into_iter()
            .filter(|link| link.trusted)
        {
            let credits = indexed.credits.entry(link.recording_id).or_default();
            if !credits.contains(&link.credit) {
                credits.push(link.credit);
            }
        }
        for link in sources
            .edition_credit_links(catalog)
            .into_iter()
            .filter(|link| link.trusted)
        {
            let credits = indexed.edition_credits.entry(link.release_id).or_default();
            if !credits.contains(&link.credit) {
                credits.push(link.credit);
            }
        }
        for label in &catalog.labels {
            let Some(identity) = sources.label_identity(catalog, label.id) else {
                continue;
            };
            let mbid = match identity {
                crate::sources::LabelIdentityResolution::Confirmed { mbid, .. }
                | crate::sources::LabelIdentityResolution::Agrees { mbid, .. }
                | crate::sources::LabelIdentityResolution::Accepted { mbid, .. } => mbid,
                _ => continue,
            };
            indexed.label_mbids.insert(label.id, mbid);
        }
        indexed
    }
}

/// The tracks a query matches, in catalog order.
///
/// Order matters because everything downstream pages through it, and paging is
/// only meaningful while the order is the same on every run.
pub fn run(query: &Query, context: &Context) -> Vec<Id> {
    context
        .catalog
        .tracks
        .iter()
        .filter(|track| matches(query, context, track.id))
        .map(|track| track.id)
        .collect()
}

/// Whether one track satisfies a query.
pub fn matches(query: &Query, context: &Context, track: Id) -> bool {
    match query {
        Query::All => true,
        Query::And(parts) => parts.iter().all(|p| matches(p, context, track)),
        Query::Or(parts) => parts.iter().any(|p| matches(p, context, track)),
        Query::Not(inner) => !matches(inner, context, track),
        Query::Term(term) => term_matches(term, context, track),
    }
}

fn term_matches(term: &Term, context: &Context, track: Id) -> bool {
    match &term.test {
        Test::Set => flag_of(&term.field, context, track),
        Test::Unset => !flag_of(&term.field, context, track),
        Test::Compare(compare, wanted) => match number_of(&term.field, context, track) {
            // A track with no year cannot satisfy a question about years. It is
            // absent from the answer rather than counted as zero, which would
            // put every untagged file in "before 1970".
            None => false,
            Some(value) => match compare {
                Compare::Equal => (value - wanted).abs() < f64::EPSILON,
                Compare::Greater => value > *wanted,
                Compare::AtLeast => value >= *wanted,
                Compare::Less => value < *wanted,
                Compare::AtMost => value <= *wanted,
            },
        },
        Test::Between(low, high) => match number_of(&term.field, context, track) {
            None => false,
            Some(value) => {
                low.map(|l| value >= l).unwrap_or(true) && high.map(|h| value <= h).unwrap_or(true)
            }
        },
        Test::Contains(wanted) => {
            let wanted = text::normalize(wanted);
            texts_of(&term.field, context, track)
                .iter()
                .any(|value| text::normalize(value).contains(&wanted))
        }
        Test::Is(wanted) => {
            let wanted = text::normalize(wanted);
            texts_of(&term.field, context, track)
                .iter()
                .any(|value| text::normalize(value) == wanted)
        }
    }
}

/// The reference for the entity a scope names, starting from a track.
fn scoped(scope: Scope, context: &Context, track: Id) -> Option<EntityRef> {
    let catalog = context.catalog;
    match scope {
        Scope::Track => EntityRef::of(catalog, EntityKind::Track, track),
        Scope::Album => catalog
            .track(track)
            .and_then(|t| t.release_id)
            .and_then(|r| EntityRef::of(catalog, EntityKind::Release, r)),
        Scope::Artist => main_artist(context, track)
            .and_then(|artist| EntityRef::of(catalog, EntityKind::Artist, artist.id)),
    }
}

fn annotation<'a>(
    scope: Scope,
    context: &'a Context,
    track: Id,
) -> Option<&'a crate::user::Annotation> {
    let reference = scoped(scope, context, track)?;
    context
        .local
        .annotation(context.data, context.owner, &reference)
}

fn main_artist<'a>(context: &Context<'a>, track: Id) -> Option<&'a crate::model::Artist> {
    context
        .local
        .credits(context.catalog, EntityKind::Track, track)
        .iter()
        .filter(|credit| credit.role == "main")
        .find_map(|credit| context.catalog.artist(credit.artist_id))
}

fn local_credits<'a>(context: &Context<'a>, track: Id) -> Vec<&'a crate::model::Credit> {
    let mut credits = context
        .local
        .credits(context.catalog, EntityKind::Track, track)
        .to_vec();
    if let Some(release) = context.catalog.track(track).and_then(|row| row.release_id) {
        credits.extend_from_slice(&context.local.credits(
            context.catalog,
            EntityKind::Release,
            release,
        ));
    }
    credits
}

fn flag_of(field: &Field, context: &Context, track: Id) -> bool {
    match field {
        Field::Lossless => context
            .catalog
            .track(track)
            .and_then(|t| context.catalog.file(t.file_id))
            .map(|f| f.properties.lossless)
            .unwrap_or(false),
        Field::Compilation => context
            .catalog
            .track(track)
            .and_then(|t| t.release_id)
            .and_then(|r| context.catalog.release(r))
            .map(|r| r.is_compilation)
            .unwrap_or(false),
        Field::Loved(scope) => annotation(*scope, context, track)
            .map(|a| a.loved)
            .unwrap_or(false),
        Field::LovedAnywhere => [Scope::Track, Scope::Album, Scope::Artist]
            .into_iter()
            .any(|scope| annotation(scope, context, track).is_some_and(|a| a.loved)),
        // Any other field used as a bare flag asks whether it holds anything.
        _ => {
            !texts_of(field, context, track).is_empty()
                || number_of(field, context, track).is_some()
        }
    }
}

fn number_of(field: &Field, context: &Context, track: Id) -> Option<f64> {
    let catalog = context.catalog;
    let track_row = catalog.track(track)?;
    let file = catalog.file(track_row.file_id);
    match field {
        Field::Year => track_row
            .release_id
            .and_then(|r| catalog.release(r))
            .and_then(|r| r.year)
            .map(f64::from),
        Field::Duration => track_row.duration_ms.map(|d| d as f64),
        Field::Size => file.map(|f| f.size as f64),
        Field::Bitrate => file.and_then(|f| f.properties.bitrate_kbps).map(f64::from),
        Field::SampleRate => file.and_then(|f| f.properties.sample_rate).map(f64::from),
        Field::Played => {
            let reference = scoped(Scope::Track, context, track)?;
            Some(f64::from(context.local.play_count(
                context.data,
                context.owner,
                &reference,
            )))
        }
        Field::Rating(scope) => annotation(*scope, context, track)
            .and_then(|a| a.rating)
            .map(f64::from),
        _ => None,
    }
}

fn texts_of(field: &Field, context: &Context, track: Id) -> Vec<String> {
    let catalog = context.catalog;
    let Some(track_row) = catalog.track(track) else {
        return Vec::new();
    };
    let file = catalog.file(track_row.file_id);
    let release = track_row.release_id.and_then(|r| catalog.release(r));
    match field {
        Field::Title => vec![track_row.title.clone()],
        Field::Artist => {
            let mut values = Vec::new();
            for credit in context
                .local
                .credits(context.catalog, EntityKind::Track, track)
                .iter()
            {
                extend_artist_values(catalog, credit.artist_id, &mut values);
                if let Some(credited_as) = &credit.credited_as {
                    push_unique(&mut values, credited_as.clone());
                }
            }
            if let Some(release_id) = track_row.release_id {
                for credit in context
                    .local
                    .credits(context.catalog, EntityKind::Release, release_id)
                    .iter()
                {
                    extend_artist_values(catalog, credit.artist_id, &mut values);
                }
            }
            for credit in sourced_credits(context, track) {
                extend_sourced_credit_values(credit, &mut values);
            }
            values
        }
        Field::Album => release
            .map(|r| {
                let mut values = vec![r.title.clone()];
                if let Some(mbid) = &r.mbid {
                    values.push(mbid.clone());
                }
                values
            })
            .unwrap_or_default(),
        Field::Recording => catalog
            .recording(track_row.recording_id)
            .map(|recording| {
                let mut values = vec![recording.title.clone()];
                if let Some(mbid) = &recording.mbid {
                    values.push(mbid.clone());
                }
                values
            })
            .unwrap_or_default(),
        Field::Work => {
            let mut values = catalog
                .recording(track_row.recording_id)
                .map(|recording| {
                    recording
                        .work_ids
                        .iter()
                        .filter_map(|&id| catalog.work(id))
                        .flat_map(|work| vec![work.title.clone(), work.mbid.clone()])
                        .collect()
                })
                .unwrap_or_default();
            // A local WORK/grouping tag is searchable even without an MBID.
            // This is text on one file, not a new canonical work identity.
            if let Some(work_tag) = catalog
                .file(track_row.file_id)
                .and_then(|file| file.first_tag("grouping"))
            {
                push_unique(&mut values, work_tag.to_string());
            }
            if let Some(works) = context.sourced.works.get(&track_row.recording_id) {
                for work in works {
                    push_unique(&mut values, work.title.clone());
                    push_unique(&mut values, work.mbid.clone());
                    for parent in &work.parents {
                        push_unique(&mut values, parent.title.clone());
                        push_unique(&mut values, parent.mbid.clone());
                    }
                }
            }
            values
        }
        Field::Movement => catalog
            .file(track_row.file_id)
            .map(|file| {
                ["movement", "movementnumber"]
                    .into_iter()
                    .filter_map(|tag| file.first_tag(tag).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        Field::ReleaseGroup => release
            .and_then(|r| r.release_group_id)
            .and_then(|id| catalog.release_group(id))
            .map(|group| vec![group.title.clone(), group.mbid.clone()])
            .unwrap_or_default(),
        Field::AlbumArtist => release
            .and_then(|r| r.album_artist_id)
            .map(|a| artist_values(catalog, a))
            .unwrap_or_default(),
        Field::Genre => {
            let mut names: Vec<String> = context
                .local
                .genres(context.catalog, EntityKind::Track, track)
                .iter()
                .map(|g| g.name.clone())
                .collect();
            if let Some(release) = release {
                names.extend(
                    context
                        .local
                        .genres(context.catalog, EntityKind::Release, release.id)
                        .iter()
                        .map(|g| g.name.clone()),
                );
            }
            names
        }
        Field::Label => release
            .map(|r| {
                r.label_ids.iter().fold(Vec::new(), |mut values, &id| {
                    if let Some(label) = catalog.label(id) {
                        values.push(label.name.clone());
                        if let Some(mbid) = &label.mbid {
                            values.push(mbid.clone());
                        }
                        if let Some(mbid) = context.sourced.label_mbids.get(&id) {
                            push_unique(&mut values, mbid.clone());
                        }
                    }
                    values
                })
            })
            .unwrap_or_default(),
        Field::Comment => file
            .and_then(|f| f.tags.get("comment"))
            .cloned()
            .unwrap_or_default(),
        // The tag first, because it costs nothing — raw tags are in the
        // catalog. The sidecar is only opened when the tag holds nothing, and
        // only for the tracks that have one, which is what keeps a search
        // across a library from being ten thousand file reads.
        Field::Lyrics => catalog
            .lyrics_of_track(track)
            .map(|lyrics| vec![lyrics.text()])
            .unwrap_or_default(),
        Field::Path => file.map(|f| vec![f.path.clone()]).unwrap_or_default(),
        Field::Codec => file
            .map(|f| {
                vec![
                    f.properties.codec.clone(),
                    f.properties.container.clone(),
                    f.properties.quality_label(),
                ]
            })
            .unwrap_or_default(),
        Field::Performing => credits_as_text(context, track, |role, _| {
            crate::model::is_performing_role(role)
        }),
        Field::Credit(role) => credits_as_text(context, track, |credited, attributes| {
            credited == *role
                || (*role == "orchestra" && credited == "performing orchestra")
                || (*role == "soloist"
                    && matches!(credited, "performer" | "instrument" | "vocal")
                    && attributes.iter().any(|attribute| attribute.name == "solo"))
        }),
        Field::Instrument => {
            let mut values = Vec::new();
            for credit in context
                .local
                .credits(context.catalog, EntityKind::Track, track)
                .iter()
            {
                for attribute in &credit.attributes {
                    push_unique(&mut values, attribute.name.clone());
                    if let Some(value) = &attribute.value {
                        push_unique(&mut values, value.clone());
                    }
                    if let Some(credited_as) = &attribute.credited_as {
                        push_unique(&mut values, credited_as.clone());
                    }
                }
            }
            for credit in sourced_credits(context, track) {
                for attribute in &credit.attributes {
                    push_unique(&mut values, attribute.name.clone());
                    if let Some(value) = &attribute.value {
                        push_unique(&mut values, value.clone());
                    }
                    if let Some(credited_as) = &attribute.credited_as {
                        push_unique(&mut values, credited_as.clone());
                    }
                }
            }
            values
        }
        Field::Guest | Field::CompilationArtist | Field::Contributor | Field::Collaborator => {
            participation_values(field, context, track, release)
        }
        Field::Tag(scope) => annotation(*scope, context, track)
            .map(|a| a.tags.iter().cloned().collect())
            .unwrap_or_default(),
        Field::Note(scope) => annotation(*scope, context, track)
            .and_then(|a| a.note.clone())
            .map(|n| vec![n])
            .unwrap_or_default(),
        // A bare word searches where a person would expect it to.
        Field::Anything => {
            let mut all = vec![track_row.title.clone()];
            all.extend(texts_of(&Field::Artist, context, track));
            all.extend(texts_of(&Field::Album, context, track));
            all
        }
        _ => Vec::new(),
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn artist_values(catalog: &Catalog, artist_id: Id) -> Vec<String> {
    let Some(artist) = catalog.artist(artist_id) else {
        return Vec::new();
    };
    let mut values = vec![artist.name.clone()];
    if let Some(mbid) = &artist.mbid {
        values.push(mbid.clone());
    }
    values.extend(artist.aliases.iter().cloned());
    values
}

fn extend_artist_values(catalog: &Catalog, artist_id: Id, values: &mut Vec<String>) {
    for value in artist_values(catalog, artist_id) {
        push_unique(values, value);
    }
}

fn sourced_credits<'a>(
    context: &'a Context<'_>,
    track_id: Id,
) -> Vec<&'a crate::sources::CreditLink> {
    let Some(track) = context.catalog.track(track_id) else {
        return Vec::new();
    };
    context
        .sourced
        .credits
        .get(&track.recording_id)
        .into_iter()
        .flat_map(|credits| credits.iter())
        .chain(
            track
                .release_id
                .and_then(|id| context.sourced.edition_credits.get(&id))
                .into_iter()
                .flat_map(|credits| credits.iter()),
        )
        .collect()
}

fn extend_sourced_credit_values(credit: &crate::sources::CreditLink, values: &mut Vec<String>) {
    push_unique(values, credit.artist_name.clone());
    push_unique(values, credit.artist_mbid.clone());
    if let Some(credited_as) = &credit.credited_as {
        push_unique(values, credited_as.clone());
    }
}

fn credits_as_text(
    context: &Context,
    track: Id,
    wanted: impl Fn(&str, &[crate::model::CreditAttribute]) -> bool,
) -> Vec<String> {
    let catalog = context.catalog;
    let mut values = Vec::new();
    for credit in local_credits(context, track)
        .into_iter()
        .filter(|credit| wanted(&credit.role, &credit.attributes))
    {
        extend_artist_values(catalog, credit.artist_id, &mut values);
        if let Some(credited_as) = &credit.credited_as {
            push_unique(&mut values, credited_as.clone());
        }
    }
    for credit in sourced_credits(context, track)
        .into_iter()
        .filter(|credit| wanted(&credit.role, &credit.attributes))
    {
        extend_sourced_credit_values(credit, &mut values);
    }
    values
}

fn participation_values(
    field: &Field,
    context: &Context,
    track: Id,
    release: Option<&crate::model::Release>,
) -> Vec<String> {
    let catalog = context.catalog;
    if catalog.track(track).is_none() {
        return Vec::new();
    }
    let performing: Vec<&crate::model::Credit> = context
        .local
        .credits(context.catalog, EntityKind::Track, track)
        .iter()
        .copied()
        .filter(|credit| crate::model::is_performing_role(&credit.role))
        .collect();
    let sourced_performing: Vec<&crate::sources::CreditLink> = sourced_credits(context, track)
        .into_iter()
        .filter(|credit| crate::model::is_performing_role(&credit.role))
        .collect();
    let mut distinct_performers: BTreeSet<String> = performing
        .iter()
        .map(|credit| local_artist_key(catalog, credit.artist_id))
        .collect();
    distinct_performers.extend(
        sourced_performing
            .iter()
            .map(|credit| sourced_artist_key(catalog, credit)),
    );
    if matches!(field, Field::Contributor) {
        let mut values = Vec::new();
        for credit in local_credits(context, track)
            .into_iter()
            .filter(|credit| !crate::model::is_performing_role(&credit.role))
        {
            extend_artist_values(catalog, credit.artist_id, &mut values);
        }
        for credit in sourced_credits(context, track)
            .into_iter()
            .filter(|credit| !crate::model::is_performing_role(&credit.role))
        {
            extend_sourced_credit_values(credit, &mut values);
        }
        return values;
    }
    let Some(release) = release else {
        return if matches!(field, Field::Collaborator) && distinct_performers.len() > 1 {
            performer_values(catalog, &performing, &sourced_performing)
        } else {
            Vec::new()
        };
    };
    let mut values = Vec::new();
    match field {
        Field::Guest => {
            if !release.is_compilation {
                for credit in performing {
                    if release.album_artist_id != Some(credit.artist_id) {
                        extend_artist_values(catalog, credit.artist_id, &mut values);
                    }
                }
                for credit in sourced_performing {
                    if !is_album_artist(catalog, release, credit) {
                        extend_sourced_credit_values(credit, &mut values);
                    }
                }
            }
        }
        Field::CompilationArtist => {
            if release.is_compilation {
                for credit in performing {
                    if release.album_artist_id != Some(credit.artist_id) {
                        extend_artist_values(catalog, credit.artist_id, &mut values);
                    }
                }
                for credit in sourced_performing {
                    if !is_album_artist(catalog, release, credit) {
                        extend_sourced_credit_values(credit, &mut values);
                    }
                }
            }
        }
        Field::Contributor => unreachable!("contributors are handled before release lookup"),
        Field::Collaborator if distinct_performers.len() > 1 => {
            return performer_values(catalog, &performing, &sourced_performing);
        }
        _ => {}
    }
    values
}

fn local_artist_key(catalog: &Catalog, artist_id: Id) -> String {
    catalog
        .artist(artist_id)
        .map(|artist| {
            artist
                .mbid
                .clone()
                .unwrap_or_else(|| format!("name:{}", artist.key))
        })
        .unwrap_or_else(|| format!("local:{artist_id}"))
}

fn sourced_artist_key(catalog: &Catalog, credit: &crate::sources::CreditLink) -> String {
    let credited_key = credit.credited_as.as_deref().map(text::normalize);
    if let Some(artist) = catalog.artists.iter().find(|artist| {
        artist.mbid.as_deref() == Some(credit.artist_mbid.as_str())
            || artist.key == text::normalize(&credit.artist_name)
            || credited_key.as_deref() == Some(artist.key.as_str())
    }) {
        return local_artist_key(catalog, artist.id);
    }
    if credit.artist_mbid.trim().is_empty() {
        format!("name:{}", text::normalize(&credit.artist_name))
    } else {
        credit.artist_mbid.clone()
    }
}

fn is_album_artist(
    catalog: &Catalog,
    release: &crate::model::Release,
    credit: &crate::sources::CreditLink,
) -> bool {
    let Some(artist) = release.album_artist_id.and_then(|id| catalog.artist(id)) else {
        return false;
    };
    artist.mbid.as_deref() == Some(credit.artist_mbid.as_str())
        || artist.key == text::normalize(&credit.artist_name)
        || credit
            .credited_as
            .as_deref()
            .is_some_and(|name| artist.key == text::normalize(name))
}

fn performer_values(
    catalog: &Catalog,
    local: &[&crate::model::Credit],
    sourced: &[&crate::sources::CreditLink],
) -> Vec<String> {
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for credit in local {
        if seen.insert(credit.artist_id) {
            extend_artist_values(catalog, credit.artist_id, &mut values);
        }
    }
    for credit in sourced {
        extend_sourced_credit_values(credit, &mut values);
    }
    values
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
