//! Where the artists on the shelf are from.
//!
//! # Why this is not a field of the catalog
//!
//! There is no widely used tag for an artist's country. `RELEASECOUNTRY`
//! exists, but it answers a different question — the country a *release* came
//! out in — and answers this one wrongly: an American pressing of a French
//! band is not an American band. So the fact comes from MusicBrainz, which
//! means it lives in the attributed layer and not in the graph.
//!
//! That makes this the first thing in the program a listing can be filtered by
//! that **the catalog does not know**. The consequence is worth stating plainly
//! rather than discovering: a library that has never run `aede fetch` has no
//! countries at all, and every command here says so instead of answering with
//! an empty table that looks like a library of stateless musicians.
//!
//! # The vocabulary is read from the data
//!
//! No table of countries is written down here. What `aede countries` lists is
//! what MusicBrainz actually said about *these* artists, the way
//! `Catalog::roles_in_use` reads the roles from the credits rather than from a
//! fixed list. A country nobody on the shelf is from is not a country this
//! program has an opinion about.
//!
//! The price of that, stated rather than discovered: MusicBrainz answers the
//! **most specific area** it holds for an artist, which is usually a country
//! and is sometimes a county or a city. A reader's shelf produced a row for
//! *County Antrim*. These are the places the source named, and the listing says
//! so; the ISO code is the only thing in the table that an authority assigned,
//! which is why it has a column to itself.
//!
//! # Short forms, and where they are allowed to come from
//!
//! Nobody wants to type "United Kingdom". The temptation is a table of
//! synonyms — UK, GB, Great Britain, Royaume-Uni — and it is a slope with no
//! bottom: whose vernacular, in which language, and who maintains it. So a
//! country is resolved in four steps, and **each one is derived from the
//! source or from the name itself**, never from a list this program invented:
//!
//! 1. the **name**, exactly: `united kingdom`;
//! 2. the **ISO code** the source states: `gb`, `fr`, `us`;
//! 3. the **initials** of a multi-word name: `uk`, `nz` — computed from the
//!    name, so no country needs to be known about in advance, and offered only
//!    where they name one place on this shelf — see `keep_usable_initials`;
//! 4. any **substring** of the name: `kingdom`, `united`, which may reach
//!    several and says so.
//!
//! `USA` and `Royaume-Uni` are therefore refused, and that is the intended
//! answer rather than a gap: neither is the source's name nor its code, and
//! this program speaks English throughout. The error names `aede countries`,
//! which lists every form that works — because what a screen displays must be
//! what the parser accepts.
//!
//! An unmatched value is an error naming the listing — the same contract
//! `--genre`, `--label` and `--role` already have.

use std::collections::BTreeMap;

use crate::model::{Catalog, EntityKind, Id, TitleMatch};
use crate::sources::{Facts, Sources};
use crate::text;
use crate::user::EntityRef;

/// The country an artist is from, as the layer holds it.
///
/// `None` covers three quite different things — never fetched, fetched and
/// MusicBrainz has no area, or matched to nobody — and callers that need to
/// tell them apart read the record itself. What they share is that this
/// program cannot say where the artist is from, which is what a filter needs.
pub fn country_of(catalog: &Catalog, held: &Sources, artist: Id) -> Option<String> {
    said_about(catalog, held, artist).0
}

/// The country and its code, as the layer holds them for one artist.
fn said_about(catalog: &Catalog, held: &Sources, artist: Id) -> (Option<String>, Option<String>) {
    let Some(entity) = EntityRef::of(catalog, EntityKind::Artist, artist) else {
        return (None, None);
    };
    let Some(record) = held.get(&entity, crate::sources::MUSICBRAINZ) else {
        return (None, None);
    };
    match &record.facts {
        Facts::Artist(facts) => {
            let keep = |value: &Option<String>| {
                value
                    .clone()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            };
            (keep(&facts.area), keep(&facts.country_code))
        }
        _ => (None, None),
    }
}

/// One country, and who on the shelf is from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// As the source spells it: "France", "United Kingdom".
    pub name: String,
    /// Normalised, for matching. See [`text::normalize`].
    pub key: String,
    /// The ISO code, lowercased, when any record carries one.
    ///
    /// Absent for a place whose artists were all fetched before the code was
    /// kept, and absent for a place MusicBrainz gave no code for. **This is
    /// the only thing here that an authority assigned**, which is why it has a
    /// column of its own: [`Place::initials`] beside it in one column made
    /// `US` — the initials of "United States" — read as an ISO code, and made
    /// a country with no code look like a country with no short form.
    pub code: Option<String>,
    /// The initials of a multi-word name, when they name this place alone.
    ///
    /// Computed by [`countries`] over the whole set rather than from the name
    /// on its own, because whether a short form is *usable* is a property of
    /// the set: `keep_usable_initials` says what disqualifies one.
    pub initials: Option<String>,
    /// The artists from there, in catalog order.
    pub artists: Vec<Id>,
}

/// The initials of a multi-word name: "United Kingdom" → `uk`.
///
/// `None` for a one-word name, where the initial would be a single letter
/// matching half the world.
fn derive_initials(key: &str) -> Option<String> {
    let letters: String = key
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect();
    (letters.chars().count() > 1).then_some(letters)
}

impl Place {
    /// Every short form a reader may type for this place. Empty when the name
    /// is the only way in.
    ///
    /// The code first: it is the one an authority assigns. There is no guard
    /// here against the two being the same word — `US` is both the code and the
    /// initials of "United States" — because `keep_usable_initials` has
    /// already dropped initials that repeat their own place's code. Deduplicating
    /// at the point of display would have meant deduplicating at every point of
    /// display, and one of them was going to be forgotten: the table, which
    /// shows the two in separate columns, duly printed `US` twice.
    pub fn short_forms(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(code) = &self.code {
            out.push(code.to_uppercase());
        }
        if let Some(letters) = &self.initials {
            out.push(letters.to_uppercase());
        }
        out
    }
}

/// Every country the layer knows about, most artists first.
///
/// Ties broken by name so that two runs over the same library answer in the
/// same order — a listing that shuffles itself between runs cannot be diffed,
/// and this one is meant to be read twice.
pub fn countries(catalog: &Catalog, held: &Sources) -> Vec<Place> {
    let mut by_name: BTreeMap<String, Place> = BTreeMap::new();
    for artist in &catalog.artists {
        let (Some(name), code) = said_about(catalog, held, artist.id) else {
            continue;
        };
        let place = by_name.entry(name.clone()).or_insert_with(|| Place {
            key: text::normalize(&name),
            name,
            code: None,
            initials: None,
            artists: Vec::new(),
        });
        // One artist fetched since the code was kept is enough to give the
        // whole country its code: the code is a property of the place, not of
        // whoever happened to be asked about first.
        if place.code.is_none() {
            place.code = code.map(|c| c.to_ascii_lowercase());
        }
        place.artists.push(artist.id);
    }
    let mut out: Vec<Place> = by_name.into_values().collect();
    keep_usable_initials(&mut out);
    out.sort_by(|a, b| {
        b.artists
            .len()
            .cmp(&a.artists.len())
            .then(a.name.cmp(&b.name))
    });
    out
}

/// Gives each place its initials, but only where they name that place alone.
///
/// **Derived is not the same as usable.** The initials of a name are derived
/// from it and invented by nobody, which is what the rule about short forms
/// asks for — and it is not enough. MusicBrainz answers the most specific area
/// it holds for an artist, so this list is not all countries: a reader's shelf
/// produced a row for *County Antrim*, whose initials are `CA`, in a library
/// that also holds Canadian artists. `--country CA` then meant two things, and
/// the listing showed the collision as though it were a helpful abbreviation.
///
/// So initials are dropped when they would answer for anything else:
///
/// - when **two places** derive the same ones — neither may claim them;
/// - when they are **some other place's ISO code** — the code was assigned by
///   an authority and the initials by this function, and a tie between the two
///   is not a tie;
/// - when they are **this place's own code**, which is not a collision but a
///   repetition: `US` is both, and a reader shown it in a `Code` column and an
///   `Also` column reads two facts where there is one. Dropped here rather than
///   where it is printed, because it was printed in three places and the third
///   forgot.
///
/// A place that loses them keeps its name and its own code; nothing becomes
/// unreachable, only unambiguous. And the collision is a property of *this*
/// library: a shelf with no Canadian artist keeps `CA` for County Antrim,
/// which is correct there and would be wrong to forbid on the strength of a
/// world atlas this program does not hold.
fn keep_usable_initials(places: &mut [Place]) {
    let codes: std::collections::BTreeSet<String> =
        places.iter().filter_map(|p| p.code.clone()).collect();
    let mut derived: BTreeMap<String, usize> = BTreeMap::new();
    for place in places.iter() {
        if let Some(letters) = derive_initials(&place.key) {
            *derived.entry(letters).or_insert(0) += 1;
        }
    }
    for place in places.iter_mut() {
        let Some(letters) = derive_initials(&place.key) else {
            continue;
        };
        let shared = derived.get(&letters).is_some_and(|&n| n > 1);
        let its_own_code = place.code.as_deref() == Some(letters.as_str());
        let someone_elses_code = codes.contains(&letters) && !its_own_code;
        if !shared && !someone_elses_code && !its_own_code {
            place.initials = Some(letters);
        }
    }
}

/// How many of these places carry no ISO code.
///
/// The number behind the one message this listing owes a reader whose codes are
/// all missing: the field is newer than their fetch, and nothing on screen says
/// so — the column is simply empty, which reads as "this program has no codes"
/// rather than "this catalog has not been asked since".
pub fn without_code(places: &[Place]) -> usize {
    places.iter().filter(|p| p.code.is_none()).count()
}

/// Every country a typed value reaches, and how it reached them.
///
/// Four steps, each derived from the source or from the name — the reasoning
/// is on the module. The first three are reported as [`TitleMatch::Exact`]
/// because each names exactly one country: a code and a set of initials are
/// identifiers, not guesses, and telling a reader their `uk` "partly matched"
/// would be false modesty about a certainty.
///
/// Only the substring step widens, and it says so: `united` reaches both the
/// United Kingdom and the United States, and the caller is told so the answer
/// can name them rather than pretend one was asked for.
pub fn find(places: &[Place], name: &str) -> (Vec<Place>, TitleMatch) {
    let key = text::normalize(name);
    if key.is_empty() {
        return (Vec::new(), TitleMatch::Exact);
    }
    for by in [
        |p: &Place, key: &str| p.key == key,
        |p: &Place, key: &str| p.code.as_deref() == Some(key),
        |p: &Place, key: &str| p.initials.as_deref() == Some(key),
    ] {
        let found: Vec<Place> = places.iter().filter(|p| by(p, &key)).cloned().collect();
        if !found.is_empty() {
            return (found, TitleMatch::Exact);
        }
    }
    (
        places
            .iter()
            .filter(|p| p.key.contains(&key))
            .cloned()
            .collect(),
        TitleMatch::Partial,
    )
}

/// How many artists on the shelf have been asked about at all.
///
/// The denominator behind every message here. "No country matches" means one
/// thing in a library that has been fetched and quite another in one that has
/// not, and a reader shown the same sentence for both learns nothing —
/// the same distinction the cover pass had to be taught four times over.
pub fn asked_about(catalog: &Catalog, held: &Sources) -> usize {
    catalog
        .artists
        .iter()
        .filter(|artist| {
            EntityRef::of(catalog, EntityKind::Artist, artist.id)
                .and_then(|entity| held.get(&entity, crate::sources::MUSICBRAINZ).cloned())
                .is_some()
        })
        .count()
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
