//! Deterministic ordering of query results, with absent numeric values last.

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
    /// Main artist, by filing name. Uncredited tracks precede named artists in
    /// ascending order and follow them in descending order.
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
/// Ties retain the selection's input order, which is catalog order for query
/// results. The same query therefore keeps rows in the same places twice
/// running — without which `--offset` would show a track twice and hide another.
pub fn sort(tracks: &mut [Id], sort: Sort, context: &Context) {
    match sort.key {
        SortKey::Catalog => {
            if sort.descending {
                tracks.reverse();
            }
        }
        SortKey::Title => sort_cached(tracks, sort.descending, |track| {
            context
                .catalog
                .track(track)
                .map(|row| text::normalize(&row.title))
        }),
        SortKey::Artist => sort_cached(tracks, sort.descending, |track| {
            main_artist(context, track).map(|artist| artist.sort_name.as_str())
        }),
        SortKey::Album => {
            sort_cached(tracks, sort.descending, |track| {
                album_position(context, track)
            });
        }
        SortKey::Year | SortKey::Duration | SortKey::Size | SortKey::Rating | SortKey::Played => {
            let Some(field) = sort_field(sort.key) else {
                return;
            };
            sort_numbers(tracks, sort.descending, &field, context);
        }
    }
}

fn sort_cached<Key: Ord>(tracks: &mut [Id], descending: bool, mut key: impl FnMut(Id) -> Key) {
    // Normalizing a title during every comparison repeats allocations and
    // text processing O(n log n) times. The cached-key sort computes it once
    // per result and preserves input order for equal keys in both directions.
    if descending {
        tracks.sort_by_cached_key(|&track| std::cmp::Reverse(key(track)));
    } else {
        tracks.sort_by_cached_key(|&track| key(track));
    }
}

fn sort_numbers(tracks: &mut [Id], descending: bool, field: &Field, context: &Context) {
    use std::cmp::Ordering;

    let mut keyed: Vec<_> = tracks
        .iter()
        .map(|&track| (track, number_of(field, context, track)))
        .collect();
    keyed.sort_by(|(_, left), (_, right)| {
        // "Unknown" is not "smallest", and it is not "largest" either: a track
        // with nothing to compare goes last **whichever way round the sort was
        // asked**, which is why this sits outside the reversal. Sorting by year
        // must not open with everything nobody ever tagged.
        match (left, right) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => {
                let order = left.partial_cmp(right).unwrap_or(Ordering::Equal);
                if descending { order.reverse() } else { order }
            }
        }
    });
    for (track, (ordered, _)) in tracks.iter_mut().zip(keyed) {
        *track = ordered;
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
