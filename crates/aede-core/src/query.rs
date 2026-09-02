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
        // A bare word is a question when it names a field that can be asked
        // one, and a search otherwise.
        if let Some(field) = field_named(word)
            && asks_whether_it_holds_anything(&field)
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

/// The same question, asked of the album or the artist instead of the track.
///
/// Every field the *user* writes carries a scope, and a bare `loved` means the
/// track's own. That is deliberate — five stars on an artist is not five stars
/// on a track — but it makes one answer badly misleading: somebody who marked
/// an **album** a favourite types `loved`, is told nothing matches, and
/// concludes the feature is broken. It is not; they asked a different question
/// from the one they meant.
///
/// So a caller that gets an empty result can ask the same question again at
/// another scope, and — if *that* answers — say which scope holds what was
/// written. The suggestion is only ever a suggestion: the query itself keeps
/// meaning exactly what it says.
///
/// Fields that carry no scope are left alone, so a mixed expression such as
/// `genre:metal loved` is rescoped only where rescoping means something.
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
#[path = "query_tests.rs"]
mod tests;
