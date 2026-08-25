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
    /// The album's own artist.
    AlbumArtist,
    /// A genre carried by the track or by its album.
    Genre,
    /// A label the album came out on.
    Label,
    /// The comment tag, as the tagger wrote it.
    Comment,
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

// --------------------------------------------------------------------------
// Reading a query
// --------------------------------------------------------------------------

/// Splits a query into its words, keeping quoted runs whole.
///
/// Parentheses are words of their own so that `(a OR b)` needs no spaces around
/// them, which nobody would remember to type.
fn tokenize(input: &str) -> Result<Vec<String>, QueryError> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            None if ch == '(' || ch == ')' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(ch.to_string());
            }
            None => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err(error("a quotation mark is left open"));
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

/// Reads a query.
pub fn parse(input: &str) -> Result<Query, QueryError> {
    let words = tokenize(input)?;
    if words.is_empty() {
        return Ok(Query::All);
    }
    let mut at = 0usize;
    let query = parse_or(&words, &mut at)?;
    if at < words.len() {
        return Err(error(format!("\"{}\" is one bracket too many", words[at])));
    }
    Ok(query)
}

fn parse_or(words: &[String], at: &mut usize) -> Result<Query, QueryError> {
    let mut parts = vec![parse_and(words, at)?];
    while *at < words.len() && is_or(&words[*at]) {
        *at += 1;
        parts.push(parse_and(words, at)?);
    }
    Ok(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Query::Or(parts)
    })
}

fn is_or(word: &str) -> bool {
    word.eq_ignore_ascii_case("or") || word == "|" || word == "||"
}

fn parse_and(words: &[String], at: &mut usize) -> Result<Query, QueryError> {
    let mut parts: Vec<Query> = Vec::new();
    while *at < words.len() && words[*at] != ")" && !is_or(&words[*at]) {
        // `AND` may be written, and is what juxtaposition already means.
        if words[*at].eq_ignore_ascii_case("and") {
            *at += 1;
            continue;
        }
        parts.push(parse_unary(words, at)?);
    }
    if parts.is_empty() {
        return Err(error("something is missing between the brackets"));
    }
    Ok(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Query::And(parts)
    })
}

fn parse_unary(words: &[String], at: &mut usize) -> Result<Query, QueryError> {
    let word = &words[*at];
    if word == "-" || word.eq_ignore_ascii_case("not") {
        *at += 1;
        if *at >= words.len() {
            return Err(error("nothing follows the minus sign"));
        }
        return Ok(Query::Not(Box::new(parse_unary(words, at)?)));
    }
    if let Some(rest) = word.strip_prefix('-')
        && !rest.is_empty()
    {
        // `-genre:metal`, the common spelling, with no space after the sign.
        let mut inner = vec![rest.to_string()];
        inner.extend_from_slice(&words[*at + 1..]);
        let mut inner_at = 0usize;
        let negated = parse_unary(&inner, &mut inner_at)?;
        *at += inner_at;
        return Ok(Query::Not(Box::new(negated)));
    }
    if word == "(" {
        *at += 1;
        let inside = parse_or(words, at)?;
        if *at >= words.len() || words[*at] != ")" {
            return Err(error("a bracket is left open"));
        }
        *at += 1;
        return Ok(inside);
    }
    if word == ")" {
        return Err(error("a closing bracket has nothing to close"));
    }
    let term = parse_term(word)?;
    *at += 1;
    Ok(Query::Term(term))
}

fn parse_term(word: &str) -> Result<Term, QueryError> {
    let Some((name, value)) = word.split_once(':') else {
        // A bare word is a flag when it names one, and a search otherwise.
        if let Some(field) = field_named(word)
            && is_flag(&field)
        {
            return Ok(Term {
                field,
                test: Test::Set,
            });
        }
        return Ok(Term {
            field: Field::Anything,
            test: Test::Contains(word.to_string()),
        });
    };
    let Some(field) = field_named(name) else {
        return Err(error(format!(
            "\"{name}\" is not a field.\nFields: {}",
            FIELD_NAMES
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let test = parse_test(&field, value)?;
    Ok(Term { field, test })
}

fn parse_test(field: &Field, value: &str) -> Result<Test, QueryError> {
    if value.is_empty() {
        return Err(error("a field needs something after the colon"));
    }
    // A flag asked with a word: `lossless:false` reads better than `-lossless`
    // and means the same, so both are accepted rather than one being a trap.
    if is_flag(field) {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "1" => return Ok(Test::Set),
            "false" | "no" | "0" => return Ok(Test::Unset),
            other => {
                return Err(error(format!(
                    "\"{other}\" is not a yes or a no: try true or false"
                )));
            }
        }
    }
    if is_numeric(field) {
        // A range, either end of which may be left open: `1990..`, `..1999`.
        if let Some((low, high)) = value.split_once("..") {
            let low = parse_number(field, low)?;
            let high = parse_number(field, high)?;
            return Ok(Test::Between(low, high));
        }
        for (prefix, compare) in [
            (">=", Compare::AtLeast),
            ("<=", Compare::AtMost),
            (">", Compare::Greater),
            ("<", Compare::Less),
            ("=", Compare::Equal),
        ] {
            if let Some(rest) = value.strip_prefix(prefix) {
                let Some(number) = parse_number(field, rest)? else {
                    return Err(error(format!("\"{rest}\" is not a number")));
                };
                return Ok(Test::Compare(compare, number));
            }
        }
        let Some(number) = parse_number(field, value)? else {
            return Err(error(format!("\"{value}\" is not a number")));
        };
        return Ok(Test::Compare(Compare::Equal, number));
    }
    if let Some(exact) = value.strip_prefix('=') {
        return Ok(Test::Is(exact.to_string()));
    }
    Ok(Test::Contains(value.to_string()))
}

/// Reads a number, accepting `3:45` wherever a duration is expected.
fn parse_number(field: &Field, raw: &str) -> Result<Option<f64>, QueryError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if *field == Field::Duration
        && let Some((minutes, seconds)) = raw.split_once(':')
    {
        let minutes: f64 = minutes
            .parse()
            .map_err(|_| error(format!("\"{raw}\" is not a length")))?;
        let seconds: f64 = seconds
            .parse()
            .map_err(|_| error(format!("\"{raw}\" is not a length")))?;
        return Ok(Some((minutes * 60.0 + seconds) * 1000.0));
    }
    let number: f64 = raw
        .parse()
        .map_err(|_| error(format!("\"{raw}\" is not a number")))?;
    // Durations are stored in milliseconds and typed in seconds.
    Ok(Some(if *field == Field::Duration {
        number * 1000.0
    } else {
        number
    }))
}

/// Every field, by the name it is typed under.
const FIELD_NAMES: &[(&str, Field)] = &[
    ("title", Field::Title),
    ("artist", Field::Artist),
    ("album", Field::Album),
    ("albumartist", Field::AlbumArtist),
    ("genre", Field::Genre),
    ("label", Field::Label),
    ("comment", Field::Comment),
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
    ("loved", Field::Loved(Scope::Track)),
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
    ("remixer", Field::Credit("remixer")),
    ("featured", Field::Credit("featured")),
    ("mainartist", Field::Credit("main")),
    ("performing", Field::Performing),
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
        Field::Lossless | Field::Compilation | Field::Loved(_)
    )
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
        Field::Credit(_) | Field::Performing => "artist",
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
                Field::Label => context
                    .catalog
                    .labels
                    .iter()
                    .any(|l| l.key.contains(&wanted)),
                // A role field names a person, and a person who is in the
                // library but never credited that way is an empty result, not
                // a misunderstanding.
                Field::Credit(_) | Field::Performing => context
                    .catalog
                    .artists
                    .iter()
                    .any(|a| a.key.contains(&wanted)),
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

/// How a result is ordered.
///
/// A query with no order is a query whose second page means nothing: paging is
/// only meaningful while the order is the same on every run. Catalog order is
/// the default because it is deterministic; everything else is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    /// What to order on.
    pub key: SortKey,
    /// `true` to put the largest, latest or highest first.
    pub descending: bool,
}

/// What a result can be ordered on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// The order the catalog was built in, which groups an album together.
    Catalog,
    /// Track title.
    Title,
    /// Main artist, by filing name.
    Artist,
    /// Album title, then disc and track number.
    Album,
    /// Year of the album.
    Year,
    /// Playing time.
    Duration,
    /// Size on disk.
    Size,
    /// Stars given to the track.
    Rating,
    /// How many times it was played.
    Played,
}

/// The sort keys, by the name they are typed under.
const SORT_KEYS: &[(&str, SortKey)] = &[
    ("catalog", SortKey::Catalog),
    ("title", SortKey::Title),
    ("artist", SortKey::Artist),
    ("album", SortKey::Album),
    ("year", SortKey::Year),
    ("duration", SortKey::Duration),
    ("length", SortKey::Duration),
    ("size", SortKey::Size),
    ("rating", SortKey::Rating),
    ("played", SortKey::Played),
];

impl Sort {
    /// Reads `year`, `year-` or `-year`; the sign is the direction.
    pub fn parse(input: &str) -> Result<Sort, QueryError> {
        let raw = input.trim();
        let (name, descending) = match raw.strip_suffix('-').or_else(|| raw.strip_prefix('-')) {
            Some(rest) => (rest, true),
            None => (raw.trim_end_matches('+').trim_start_matches('+'), false),
        };
        let wanted = name.trim().to_lowercase();
        let Some((_, key)) = SORT_KEYS.iter().find(|(n, _)| *n == wanted) else {
            return Err(error(format!(
                "\"{name}\" is not something to sort on.\nTry: {}",
                sort_key_names().join(", ")
            )));
        };
        Ok(Sort {
            key: *key,
            descending,
        })
    }
}

/// The sort keys, for a help message.
pub fn sort_key_names() -> Vec<&'static str> {
    SORT_KEYS.iter().map(|(name, _)| *name).collect()
}

/// Puts a result in order.
///
/// Ties fall back on catalog order, so the same query gives the same rows in
/// the same places twice running — without which `--offset` would show a track
/// twice and hide another.
pub fn sort(tracks: &mut [Id], sort: Sort, context: &Context) {
    if sort.key == SortKey::Catalog {
        if sort.descending {
            tracks.reverse();
        }
        return;
    }
    let position: std::collections::BTreeMap<Id, usize> = tracks
        .iter()
        .enumerate()
        .map(|(at, &id)| (id, at))
        .collect();
    tracks.sort_by(|&a, &b| {
        use std::cmp::Ordering;
        // "Unknown" is not "smallest", and it is not "largest" either: a track
        // with nothing to compare goes last **whichever way round the sort was
        // asked**, which is why this sits outside the reversal. Sorting by year
        // must not open with everything nobody ever tagged.
        let order = match (missing(sort.key, context, a), missing(sort.key, context, b)) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                let order = compare(sort.key, context, a, b);
                if sort.descending {
                    order.reverse()
                } else {
                    order
                }
            }
        };
        order.then_with(|| position.get(&a).cmp(&position.get(&b)))
    });
}

/// `true` when a track has no value for this key, and therefore belongs at the
/// end rather than at either extreme.
fn missing(key: SortKey, context: &Context, track: Id) -> bool {
    match sort_field(key) {
        Some(field) => number_of(&field, context, track).is_none(),
        None => false,
    }
}

/// The field a numeric sort key reads, if it is a numeric one.
fn sort_field(key: SortKey) -> Option<Field> {
    Some(match key {
        SortKey::Year => Field::Year,
        SortKey::Duration => Field::Duration,
        SortKey::Size => Field::Size,
        SortKey::Rating => Field::Rating(Scope::Track),
        SortKey::Played => Field::Played,
        _ => return None,
    })
}

fn compare(key: SortKey, context: &Context, a: Id, b: Id) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let catalog = context.catalog;
    match key {
        SortKey::Catalog => Ordering::Equal,
        SortKey::Title => catalog
            .track(a)
            .map(|t| text::normalize(&t.title))
            .cmp(&catalog.track(b).map(|t| text::normalize(&t.title))),
        SortKey::Artist => sort_name(context, a).cmp(&sort_name(context, b)),
        SortKey::Album => album_position(context, a).cmp(&album_position(context, b)),
        SortKey::Year | SortKey::Duration | SortKey::Size | SortKey::Rating | SortKey::Played => {
            let Some(field) = sort_field(key) else {
                return Ordering::Equal;
            };
            let left = number_of(&field, context, a);
            let right = number_of(&field, context, b);
            match (left, right) {
                (Some(l), Some(r)) => l.partial_cmp(&r).unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            }
        }
    }
}

fn sort_name(context: &Context, track: Id) -> Option<String> {
    context
        .catalog
        .credits_on(EntityKind::Track, track)
        .into_iter()
        .find(|(_, role)| *role == "main")
        .map(|(artist, _)| artist.sort_name.clone())
}

/// Album title, then where the track sits in it: sorting by album and getting
/// the tracks shuffled inside it would be half an answer.
fn album_position(context: &Context, track: Id) -> (String, u32, u32) {
    let catalog = context.catalog;
    let Some(row) = catalog.track(track) else {
        return (String::new(), 0, 0);
    };
    let title = row
        .release_id
        .and_then(|r| catalog.release(r))
        .map(|r| text::normalize(&r.title))
        .unwrap_or_default();
    (title, row.disc_no.unwrap_or(1), row.track_no.unwrap_or(0))
}

/// Everything needed to answer, gathered once rather than per track.
pub struct Context<'a> {
    /// The library.
    pub catalog: &'a Catalog,
    /// What the user wrote about it.
    pub data: &'a UserData,
    /// Whose opinions count.
    pub owner: &'a str,
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
        Scope::Artist => catalog
            .credits_on(EntityKind::Track, track)
            .into_iter()
            .find(|(_, role)| *role == "main")
            .and_then(|(artist, _)| EntityRef::of(catalog, EntityKind::Artist, artist.id)),
    }
}

fn annotation<'a>(
    scope: Scope,
    context: &'a Context,
    track: Id,
) -> Option<&'a crate::user::Annotation> {
    let reference = scoped(scope, context, track)?;
    context.data.find(context.owner, &reference)
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
            Some(f64::from(
                context.data.play_count(context.owner, &reference),
            ))
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
        Field::Artist => catalog
            .credits_on(EntityKind::Track, track)
            .into_iter()
            .map(|(artist, _)| artist.name.clone())
            .collect(),
        Field::Album => release.map(|r| vec![r.title.clone()]).unwrap_or_default(),
        Field::AlbumArtist => release
            .and_then(|r| r.album_artist_id)
            .and_then(|a| catalog.artist(a))
            .map(|a| vec![a.name.clone()])
            .unwrap_or_default(),
        Field::Genre => {
            let mut names: Vec<String> = catalog
                .genres_of(EntityKind::Track, track)
                .into_iter()
                .map(|g| g.name.clone())
                .collect();
            if let Some(release) = release {
                names.extend(
                    catalog
                        .genres_of(EntityKind::Release, release.id)
                        .into_iter()
                        .map(|g| g.name.clone()),
                );
            }
            names
        }
        Field::Label => release
            .map(|r| {
                r.label_ids
                    .iter()
                    .filter_map(|&id| catalog.label(id))
                    .map(|l| l.name.clone())
                    .collect()
            })
            .unwrap_or_default(),
        Field::Comment => file
            .and_then(|f| f.tags.get("comment"))
            .cloned()
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
        Field::Performing => catalog
            .credits_on(EntityKind::Track, track)
            .into_iter()
            .filter(|(_, role)| crate::model::is_performing_role(role))
            .map(|(artist, _)| artist.name.clone())
            .collect(),
        Field::Credit(role) => catalog
            .credits_on(EntityKind::Track, track)
            .into_iter()
            .filter(|(_, credited)| credited == role)
            .map(|(artist, _)| artist.name.clone())
            .collect(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;
    use crate::user::{EntityRef, LOCAL_USER, Play, UserData};

    fn catalog() -> Catalog {
        model::build(
            vec![
                model::tests::track(
                    "/m/Deicide/Legion/01 Satan Spawn.flac",
                    &[
                        ("title", "Satan Spawn"),
                        ("artist", "Deicide"),
                        ("albumartist", "Deicide"),
                        ("album", "Legion"),
                        ("date", "1992"),
                        ("genre", "Death Metal"),
                        ("label", "Roadrunner"),
                        ("comment", "vinyl rip"),
                    ],
                    200_000,
                ),
                model::tests::track(
                    "/m/Ozzy/Blizzard/01 Crazy Train.flac",
                    &[
                        ("title", "Crazy Train"),
                        ("artist", "Ozzy Osbourne"),
                        ("albumartist", "Ozzy Osbourne"),
                        ("album", "Blizzard of Ozz"),
                        ("date", "1980"),
                        ("genre", "Heavy Metal"),
                    ],
                    295_000,
                ),
                model::tests::track(
                    "/m/Miles/Kind of Blue/01 So What.flac",
                    &[
                        ("title", "So What"),
                        ("artist", "Miles Davis"),
                        ("albumartist", "Miles Davis"),
                        ("album", "Kind of Blue"),
                        ("date", "1959"),
                        ("genre", "Jazz"),
                    ],
                    545_000,
                ),
            ],
            vec!["/m".into()],
            0,
        )
    }

    fn titles(expression: &str, catalog: &Catalog, data: &UserData) -> Vec<String> {
        let query = parse(expression).unwrap_or_else(|e| panic!("{expression}: {e}"));
        let context = Context {
            catalog,
            data,
            owner: LOCAL_USER,
        };
        run(&query, &context)
            .into_iter()
            .filter_map(|id| catalog.track(id))
            .map(|t| t.title.clone())
            .collect()
    }

    #[test]
    fn a_field_narrows_and_juxtaposition_means_and() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("genre:metal", &c, &d).len(), 2);
        assert_eq!(titles("genre:metal artist:ozzy", &c, &d), ["Crazy Train"]);
        assert!(titles("genre:metal artist:miles", &c, &d).is_empty());
    }

    #[test]
    fn or_and_not_are_the_two_things_options_can_never_express() {
        // The whole reason for a grammar: `--genre a --genre b` can only ever
        // mean "and", and there is no spelling at all for "except".
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("artist:ozzy OR artist:miles", &c, &d).len(), 2);
        assert_eq!(titles("genre:metal -artist:ozzy", &c, &d), ["Satan Spawn"]);
        assert_eq!(titles("-genre:metal", &c, &d), ["So What"]);
        assert_eq!(
            titles("(artist:ozzy OR artist:deicide) year:..1985", &c, &d),
            ["Crazy Train"]
        );
    }

    #[test]
    fn a_range_is_inclusive_and_either_end_may_be_left_open() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("year:1980..1992", &c, &d).len(), 2);
        assert_eq!(titles("year:..1959", &c, &d), ["So What"]);
        assert_eq!(titles("year:1992..", &c, &d), ["Satan Spawn"]);
        assert_eq!(titles("year:1980", &c, &d), ["Crazy Train"]);
        assert_eq!(titles("year:>=1980", &c, &d).len(), 2);
    }

    #[test]
    fn a_track_with_nothing_to_compare_is_absent_rather_than_zero() {
        // Counting a missing year as zero would file every untagged file under
        // "before 1970" — an answer, and a wrong one.
        let c = model::build(
            vec![model::tests::track(
                "/m/x/a.flac",
                &[("title", "No Year"), ("artist", "A"), ("album", "B")],
                1000,
            )],
            vec!["/m".into()],
            0,
        );
        let d = UserData::default();
        assert!(titles("year:<2000", &c, &d).is_empty());
        assert!(titles("year:0..3000", &c, &d).is_empty());
        assert_eq!(titles("-year:<2000", &c, &d), ["No Year"]);
    }

    #[test]
    fn what_the_user_wrote_is_queryable_and_says_where_it_was_written() {
        // "Rated five stars" is a different claim depending on whether the
        // stars were put on the track, the album or the artist, so the field
        // says which rather than folding the three together.
        let c = catalog();
        let mut d = UserData::default();
        let artist = EntityRef::new(EntityKind::Artist, "ozzy osbourne");
        d.entry(LOCAL_USER, &artist, 1).rating = Some(5);
        let track = EntityRef::new(EntityKind::Track, "/m/Miles/Kind of Blue/01 So What.flac");
        {
            let a = d.entry(LOCAL_USER, &track, 1);
            a.loved = true;
            a.tags.insert("vinyl".into());
            a.note = Some("the 1997 remaster".into());
        }

        assert_eq!(titles("artist.rating:5", &c, &d), ["Crazy Train"]);
        assert!(titles("rating:5", &c, &d).is_empty(), "not on the track");
        assert_eq!(titles("loved", &c, &d), ["So What"]);
        assert_eq!(titles("tag:vinyl", &c, &d), ["So What"]);
        assert_eq!(titles("note:remaster", &c, &d), ["So What"]);
        assert_eq!(titles("-loved", &c, &d).len(), 2);
    }

    #[test]
    fn a_flag_may_be_asked_either_way_round() {
        // `lossless:false` reads better than `-lossless` and means the same;
        // accepting only one of the two makes the other a silent trap, since
        // a value on a flag field would otherwise match nothing at all.
        let c = catalog();
        let mut d = UserData::default();
        d.entry(
            LOCAL_USER,
            &EntityRef::new(EntityKind::Track, "/m/Miles/Kind of Blue/01 So What.flac"),
            1,
        )
        .loved = true;

        assert_eq!(titles("loved:true", &c, &d), ["So What"]);
        assert_eq!(titles("loved:false", &c, &d).len(), 2);
        assert_eq!(titles("lossless:yes", &c, &d).len(), 3);
        assert!(titles("lossless:no", &c, &d).is_empty());

        let error = parse("loved:banana").expect_err("neither yes nor no");
        assert!(error.message.contains("yes or a no"), "{error}");
    }

    #[test]
    fn the_credit_table_can_be_asked_who_did_what() {
        // `artist:` matches any credit in any role, which is why two of them
        // already mean "both are on it". A role field asks the finer question
        // the graph was built for, and the one no pile of options expresses.
        let c = model::build(
            vec![
                model::tests::track(
                    "/m/a/01.flac",
                    &[
                        ("title", "Crazy Train"),
                        ("artist", "Ozzy Osbourne"),
                        ("album", "Blizzard"),
                        ("composer", "Randy Rhoads"),
                        ("producer", "Max Norman"),
                    ],
                    1000,
                ),
                model::tests::track(
                    "/m/b/01.flac",
                    &[
                        ("title", "Other"),
                        ("artist", "Randy Rhoads"),
                        ("album", "Elsewhere"),
                        ("composer", "Ozzy Osbourne"),
                    ],
                    1000,
                ),
            ],
            vec!["/m".into()],
            0,
        );
        let d = UserData::default();

        // The same two names, in swapped roles, are two different questions.
        assert_eq!(titles("composer:rhoads", &c, &d), ["Crazy Train"]);
        assert_eq!(titles("composer:ozzy", &c, &d), ["Other"]);
        assert_eq!(titles("producer:norman", &c, &d), ["Crazy Train"]);

        // And a role composes with everything else.
        assert_eq!(
            titles("composer:rhoads mainartist:ozzy", &c, &d),
            ["Crazy Train"]
        );
        assert!(titles("composer:rhoads mainartist:rhoads", &c, &d).is_empty());

        // `artist:` still means "credited at all, however".
        assert_eq!(titles("artist:ozzy", &c, &d).len(), 2);
    }

    #[test]
    fn who_is_audible_is_its_own_question() {
        // Singing one guest verse counts; having written the words does not.
        // `artist --with` asks exactly this, and it is not the same as
        // "credited at all".
        let c = model::build(
            vec![model::tests::track(
                "/m/a/01.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("album", "Blizzard"),
                    ("performer", "Randy Rhoads"),
                    ("lyricist", "Bob Daisley"),
                ],
                1000,
            )],
            vec!["/m".into()],
            0,
        );
        let d = UserData::default();
        assert_eq!(titles("performing:rhoads", &c, &d), ["Crazy Train"]);
        assert_eq!(titles("performing:ozzy", &c, &d), ["Crazy Train"]);
        assert!(
            titles("performing:daisley", &c, &d).is_empty(),
            "writing the words is not being heard"
        );
        assert_eq!(
            titles("artist:daisley", &c, &d),
            ["Crazy Train"],
            "but he is credited all the same"
        );
    }

    #[test]
    fn play_counts_answer_what_has_never_been_heard() {
        let c = catalog();
        let mut d = UserData::default();
        let track = EntityRef::new(EntityKind::Track, "/m/Ozzy/Blizzard/01 Crazy Train.flac");
        d.record_play(Play {
            owner: LOCAL_USER.into(),
            track,
            at: 1,
            ms_played: 1,
            completed: true,
        });
        assert_eq!(titles("played:>=1", &c, &d), ["Crazy Train"]);
        assert_eq!(titles("played:0", &c, &d).len(), 2);
    }

    #[test]
    fn a_length_may_be_typed_the_way_it_is_read() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("duration:>5:00", &c, &d), ["So What"]);
        assert_eq!(titles("duration:..240", &c, &d), ["Satan Spawn"]);
    }

    #[test]
    fn a_quoted_value_keeps_its_spaces_and_a_bare_word_searches() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("album:\"kind of blue\"", &c, &d), ["So What"]);
        assert_eq!(titles("crazy", &c, &d), ["Crazy Train"]);
        assert_eq!(titles("\"Miles Davis\"", &c, &d), ["So What"]);
    }

    #[test]
    fn exact_and_contains_are_different_questions() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("album:legion", &c, &d), ["Satan Spawn"]);
        assert_eq!(titles("album:=legion", &c, &d), ["Satan Spawn"]);
        assert!(
            titles("album:=legio", &c, &d).is_empty(),
            "exact means exact"
        );
        assert_eq!(titles("album:legio", &c, &d), ["Satan Spawn"]);
    }

    #[test]
    fn a_result_can_be_put_in_order_and_the_unknown_goes_last() {
        // "Unknown" is not "smallest": sorting by year must not open with
        // everything nobody ever tagged, whichever way round it is asked.
        let mut c = catalog();
        let d = UserData::default();
        let context = Context {
            catalog: &c,
            data: &d,
            owner: LOCAL_USER,
        };
        let mut tracks = run(&Query::All, &context);
        sort(&mut tracks, Sort::parse("year").unwrap(), &context);
        let years: Vec<u32> = tracks
            .iter()
            .filter_map(|&id| c.track(id))
            .filter_map(|t| t.release_id)
            .filter_map(|r| c.release(r))
            .filter_map(|r| r.year)
            .collect();
        assert_eq!(years, [1959, 1980, 1992]);

        let mut descending = run(&Query::All, &context);
        sort(&mut descending, Sort::parse("year-").unwrap(), &context);
        assert_eq!(
            c.track(descending[0]).map(|t| t.title.as_str()),
            Some("Satan Spawn")
        );

        // A track with no year lands last both ways.
        c = model::build(
            vec![
                model::tests::track(
                    "/m/x/a.flac",
                    &[("title", "No Year"), ("artist", "A"), ("album", "B")],
                    1000,
                ),
                model::tests::track(
                    "/m/y/b.flac",
                    &[
                        ("title", "Dated"),
                        ("artist", "A"),
                        ("album", "C"),
                        ("date", "1999"),
                    ],
                    1000,
                ),
            ],
            vec!["/m".into()],
            0,
        );
        let context = Context {
            catalog: &c,
            data: &d,
            owner: LOCAL_USER,
        };
        for order in ["year", "year-"] {
            let mut tracks = run(&Query::All, &context);
            sort(&mut tracks, Sort::parse(order).unwrap(), &context);
            assert_eq!(
                c.track(*tracks.last().unwrap()).map(|t| t.title.as_str()),
                Some("No Year"),
                "sorted {order}"
            );
        }

        let error = Sort::parse("bananas").expect_err("not a key");
        assert!(
            error.message.contains("not something to sort on"),
            "{error}"
        );
        assert!(error.message.contains("year"), "and lists them: {error}");
    }

    #[test]
    fn an_empty_query_matches_everything_and_a_broken_one_says_why() {
        let c = catalog();
        let d = UserData::default();
        assert_eq!(titles("", &c, &d).len(), 3);
        assert_eq!(titles("   ", &c, &d).len(), 3);

        assert!(parse("genre:").is_err(), "a field with nothing after it");
        assert!(parse("(genre:metal").is_err(), "an open bracket");
        assert!(parse("genre:metal)").is_err(), "a stray closing bracket");
        assert!(parse("\"unclosed").is_err(), "an open quotation mark");
        assert!(parse("year:abc").is_err(), "a year that is not one");

        // An unknown field names the ones that exist rather than shrugging.
        let error = parse("bogus:1").expect_err("unknown field");
        assert!(error.message.contains("not a field"), "{error}");
        assert!(error.message.contains("genre"), "{error}");
    }
}
