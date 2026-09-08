//! Which spellings of a name are the same person, decided before any of them
//! becomes an entity.
//!
//! # The problem, and its two halves
//!
//! The catalog builds an artist per **normalised name**, so a library holding
//! `Ozzy Osbourne` on one album and `O. Osbourne` on another holds two
//! musicians. Every count, every listing and every page is wrong by one, and no
//! amount of comparing the two strings can fix it safely: matching on a
//! fragment of a name would merge Angus Young with Neil Young, which is worse
//! than the fault it cures.
//!
//! **The half that has an exact answer.** A great many libraries have been
//! through **Picard**, and those files carry `MUSICBRAINZ_ARTISTID`. Two
//! spellings under one identifier are not a resemblance to be judged — they are
//! the same artist, said so by the only authority there is on the question.
//! That half needs no heuristic at all.
//!
//! **The half nobody outside can answer.** Old rips, downloads, a friend's
//! drive: nobody on earth knows that a particular `O. Osbourne` is Ozzy except
//! the person whose disk it is. So the program does not guess — it asks, with
//! `aede merge`, and keeps the answer in `user.json` beside everything else
//! that was said rather than derived. `doctor` **suggests** pairs worth looking
//! at and applies none of them.
//!
//! Both halves arrive here, and they are resolved together rather than one
//! after the other, for the reason in [`aliases`].
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
//! Where a person has said so, the one they named. Otherwise the one that names
//! the most tracks, ties broken by the normalised key so that two runs over one
//! library answer alike. Either way it is **a key that already existed**, so
//! merging can only ever shrink the set of keys: anything filed under the
//! surviving key in `user.json` or `sources.json` keeps pointing at it.

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

/// What the owner of the disk says, for the files no identifier can answer for.
///
/// Both keys normalised, which is the form `user::SameArtist` holds them in and
/// the form every merge in this program is decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// The spelling that gives way.
    pub spelling: String,
    /// The spelling it is to be filed under.
    pub filed_as: String,
}

/// Reads the aliases out of everything the scan saw and everything the owner
/// said.
///
/// `said` is every unambiguous (identifier, name) pair in the library, in any
/// order; a file contributes one for its artist and one for its album artist
/// when each is unambiguous, and nothing when it is not. `chosen` is what the
/// person typed.
///
/// **The two are resolved together, not one on top of the other.** Laying a
/// person's statement over a finished table is where this goes wrong: if the
/// files elect `o osbourne` — perfectly possible, the count decides — and the
/// person says `o osbourne` gives way to `Ozzy`, overwriting one row leaves the
/// other standing and the two names point at each other for ever. Every
/// statement, from either source, is therefore read as *these spellings are one
/// artist*; the spellings are gathered into groups; and each group elects one
/// survivor, which cannot loop because there is exactly one per group.
pub fn aliases(said: impl Iterator<Item = Said>, chosen: &[Chosen]) -> Aliases {
    // How often each spelling was seen, per identifier. Counted rather than
    // collected into a set, because the count is what decides which spelling
    // survives where nobody has said otherwise, and a set would throw it away.
    let mut seen: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for Said { mbid, name } in said {
        let key = text::normalize(&name);
        if mbid.trim().is_empty() || key.is_empty() {
            continue;
        }
        *seen.entry(mbid).or_default().entry(key).or_insert(0) += 1;
    }

    // How many tracks each spelling names, whatever identifier it came under.
    // All that is left of the counts once the groups are formed, and the
    // tie-break of last resort.
    let mut tracks: HashMap<String, usize> = HashMap::new();
    for spellings in seen.values() {
        for (key, count) in spellings {
            *tracks.entry(key.clone()).or_insert(0) += count;
        }
    }

    let mut groups = Groups::default();
    for spellings in seen.values() {
        // One spelling is not an alias of anything. Joining it to itself would
        // be harmless and pointless; skipping keeps the table to the names that
        // really are somebody's alias.
        let mut keys = spellings.keys();
        if let Some(first) = keys.next() {
            for other in keys {
                groups.join(first, other);
            }
        }
    }
    // A person saying two names are one artist has said something no file can
    // contradict, and it joins the same groups: their statement decides *which*
    // spelling survives, below, not whether the two belong together.
    for Chosen { spelling, filed_as } in chosen {
        if spelling.is_empty() || filed_as.is_empty() || spelling == filed_as {
            continue;
        }
        groups.join(spelling, filed_as);
    }

    // A spelling somebody said gives way can never be the survivor, and one
    // they named as the destination is the survivor unless they also said it
    // gives way — which is how `A → B` followed by `B → C` lands everybody on
    // `C` without either statement having to know about the other.
    let gives_way: Vec<&str> = chosen.iter().map(|c| c.spelling.as_str()).collect();
    let named: Vec<&str> = chosen.iter().map(|c| c.filed_as.as_str()).collect();

    let mut aliases = Aliases::new();
    for mut ranked in groups.members() {
        if ranked.len() < 2 {
            continue;
        }
        // Ranked once, so both rules below read the same list: most tracks
        // first, then the normalised key. Sorting on the key rather than on
        // iteration order matters — a `HashMap` promises none, and a catalog
        // that named an artist differently on Tuesday would be unusable.
        ranked.sort_by(|a, b| {
            tracks
                .get(b)
                .unwrap_or(&0)
                .cmp(tracks.get(a).unwrap_or(&0))
                .then(a.cmp(b))
        });
        // Where the statements cancel out — `A → B` and `B → A`, a person
        // contradicting themselves — nobody is elected by name and the count
        // decides, as it does when nobody has said anything at all. The two
        // names still become one artist, which is the part they agreed on.
        let winner = ranked
            .iter()
            .find(|k| named.contains(&k.as_str()) && !gives_way.contains(&k.as_str()))
            .unwrap_or(&ranked[0])
            .clone();
        for loser in ranked {
            if loser != winner {
                aliases.insert(loser, winner.clone());
            }
        }
    }
    aliases
}

/// The name a spelling is filed under: its alias, or itself.
pub fn filed_as<'a>(aliases: &'a Aliases, key: &'a str) -> &'a str {
    aliases.get(key).map(String::as_str).unwrap_or(key)
}

/// Spellings gathered into the sets that name one artist.
///
/// A union-find, and the reason for it is the one in [`aliases`]: statements
/// arrive as *pairs*, from two sources that know nothing of each other, and
/// turning pairs into one winner per name is exactly what a disjoint-set
/// structure does — with no path that can produce a cycle, whatever order the
/// pairs arrive in.
#[derive(Default)]
struct Groups {
    /// Each spelling's parent, walked up to the root that stands for its group.
    parent: HashMap<String, String>,
}

impl Groups {
    /// Puts two spellings in one group.
    fn join(&mut self, one: &str, other: &str) {
        let (a, b) = (self.root(one), self.root(other));
        if a != b {
            // The smaller key becomes the root, so a group's root depends on
            // its members rather than on the order the pairs arrived in. Which
            // member is the root says nothing about which spelling wins — that
            // is decided per group in `aliases` — but a stable root is what
            // makes this structure testable.
            let (root, child) = if a < b { (a, b) } else { (b, a) };
            self.parent.insert(child, root);
        }
    }

    /// The root of the group a spelling belongs to, adding it if it is new.
    fn root(&mut self, key: &str) -> String {
        let mut at = key.to_string();
        while let Some(up) = self.parent.get(&at) {
            if up == &at {
                break;
            }
            at = up.clone();
        }
        self.parent
            .entry(key.to_string())
            .or_insert_with(|| at.clone());
        at
    }

    /// Every group, as the spellings it holds.
    fn members(mut self) -> Vec<Vec<String>> {
        let keys: Vec<String> = self.parent.keys().cloned().collect();
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for key in keys {
            let root = self.root(&key);
            grouped.entry(root).or_default().push(key);
        }
        grouped.into_values().collect()
    }
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
