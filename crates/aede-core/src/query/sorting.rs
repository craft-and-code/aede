//! Deterministic ordering of query results, with absent values last.

use super::*;

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
    main_artist(context, track).map(|artist| artist.sort_name.clone())
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
