//! Which spellings of a name are the same person, decided before any of them
//! becomes an entity.
//!
//! # The problem, and the half of it that has an exact answer
//!
//! The catalog builds an artist per **normalised name**, so a library holding
//! `Ozzy Osbourne` on one album and `O. Osbourne` on another holds two
//! musicians. Every count, every listing and every page is wrong by one, and no
//! amount of comparing the two strings can fix it safely: matching on a
//! fragment of a name would merge Angus Young with Neil Young, which is worse
//! than the fault it cures.
//!
//! But a great many libraries have been through **Picard**, and those files
//! carry `MUSICBRAINZ_ARTISTID`. Two spellings under one identifier are not a
//! resemblance to be judged — they are the same artist, said so by the only
//! authority there is on the question. **That half needs no heuristic at all**,
//! and it is the half this module does.
//!
//! The other half — files that never met MusicBrainz — cannot be answered from
//! outside: nobody on earth knows that a particular `O. Osbourne` is Ozzy
//! except the person whose disk it is. That is what a local alias file is for,
//! and it is a separate piece of work.
//!
//! # Only where the pairing is unambiguous
//!
//! A tag may name several artists at once, and the identifiers arrive as their
//! own list: `artist` may be one string that [`crate::text::split_artists`]
//! turns into two names, while `MUSICBRAINZ_ARTISTID` holds two values in an
//! order nothing guarantees to be the same. Pairing them by position would
//! attach an identifier to whichever name happened to sort first.
//!
//! So a file speaks only when it names **exactly one artist and exactly one
//! identifier**. That covers the overwhelming majority of tracks, and it is
//! never a guess. The rule this program follows everywhere: several equally
//! good answers are refused rather than arbitrated.
//!
//! # Which spelling survives
//!
//! The one that names the most tracks, ties broken by the normalised key so
//! that two runs over one library answer alike. It is derived from the library
//! rather than chosen — the shelf's own most frequent way of writing the name —
//! and it is deliberately **a key that already existed**, so merging can only
//! ever shrink the set of keys. Anything filed under the surviving key in
//! `user.json` or `sources.json` keeps pointing at it.

use std::collections::HashMap;

use crate::text;

/// The name every spelling of one artist is filed under.
///
/// Keys and values are normalised names. A name that is nobody's alias is
/// absent rather than mapped to itself, so the common case costs one lookup
/// that misses.
pub type Aliases = HashMap<String, String>;

/// What one file says about one artist, when it says it unambiguously.
///
/// Both halves come from the same file and the same tag pair, which is what
/// makes them evidence rather than a coincidence.
pub struct Said {
    /// The identifier the file carries.
    pub mbid: String,
    /// The one artist name it carries beside it.
    ///
    /// Owned rather than borrowed from the tags. The name a file *credits* is
    /// not always a string the file holds — a tag naming two artists is split
    /// before it gets here — so borrowing would mean handing back a slice of
    /// something the caller had just built. Two small allocations per file,
    /// against the cost of having read the file at all.
    pub name: String,
}

/// Reads the aliases out of everything the scan saw.
///
/// `said` is every unambiguous (identifier, name) pair in the library, in any
/// order; a file contributes one for its artist and one for its album artist
/// when each is unambiguous, and nothing when it is not.
pub fn aliases(said: impl Iterator<Item = Said>) -> Aliases {
    // How often each spelling was seen, per identifier. Counted rather than
    // collected into a set, because the count is what decides which spelling
    // survives and a set would throw it away.
    let mut seen: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for Said { mbid, name } in said {
        let key = text::normalize(&name);
        if mbid.trim().is_empty() || key.is_empty() {
            continue;
        }
        *seen.entry(mbid).or_default().entry(key).or_insert(0) += 1;
    }

    let mut aliases = Aliases::new();
    for spellings in seen.into_values() {
        if spellings.len() < 2 {
            // One spelling is not an alias of anything, and mapping it to
            // itself would make every lookup that misses look like one that
            // hit.
            continue;
        }
        // Most tracks first; then the normalised key, so that two runs over one
        // library agree. Sorting on the key rather than on iteration order
        // matters: a `HashMap` does not promise one, and a catalog that named
        // an artist differently on Tuesday would be unusable.
        let mut ranked: Vec<(String, usize)> = spellings.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let (winner, rest) = ranked.split_first().expect("at least two");
        for (loser, _) in rest {
            aliases.insert(loser.clone(), winner.0.clone());
        }
    }
    aliases
}

/// The name a spelling is filed under: its alias, or itself.
pub fn filed_as<'a>(aliases: &'a Aliases, key: &'a str) -> &'a str {
    aliases.get(key).map(String::as_str).unwrap_or(key)
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
