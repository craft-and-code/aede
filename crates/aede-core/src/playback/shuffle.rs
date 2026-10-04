//! Deterministic, metadata-based musical journeys over a selection.
//!
//! Smart shuffle plans a permutation of **occurrences**, not a new selection.
//! It keeps duplicate playlist entries, never reads audio and never changes the
//! catalog. A small undirected style-affinity graph is supplemented by local
//! tag co-occurrences. Its links express playback heuristics, not a historical
//! genealogy or an objective measurement of how two recordings sound.
//!
//! The planner prefers short genre steps, varied artists and albums, and
//! candidates with an available onward step. An arbitrary finite selection can
//! have disconnected styles or exhaust its bridge recordings. It then includes
//! every remaining occurrence and reports the discontinuity explicitly. Missing
//! genre tags are reported separately rather than treated as musical similarity.

use std::collections::{BTreeSet, VecDeque};

use crate::model::{Catalog, Id};

#[path = "shuffle_graph.rs"]
mod graph;
#[path = "shuffle_profiles.rs"]
mod profiles;

use graph::GenreGraph;
use profiles::Profile;

/// Version of the deterministic metadata rules and order planner.
///
/// A seed reproduces an order for this version, the same selection and the same
/// catalog metadata. Retagging or changing the algorithm can change the order.
pub const ALGORITHM_VERSION: u32 = 1;

/// Largest preferred style step on the planner's `0..=1000` distance scale.
///
/// This is a heuristic threshold, not a measured acoustic distance. The planner
/// reports larger steps instead of dropping tracks or silently looping a subset.
pub const MAX_STYLE_STEP: u16 = 300;

/// Extra distance admitted around the nearest available genre-safe candidate.
///
/// A nearer bridge should be used before a more distant style is selected simply
/// because both fit the global ceiling. Seeded weighting still varies choices
/// inside this local neighborhood.
pub const NEIGHBORHOOD_WINDOW: u16 = 60;

const CANDIDATE_LIMIT: usize = 96;
const NEARBY_GENRES: usize = 12;
const PER_GENRE_CANDIDATES: usize = 6;
const RECENT_ARTISTS: usize = 5;
const RECENT_ALBUMS: usize = 3;

/// What could not be made into a smooth, fully informed journey.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SmartShuffleReport {
    /// Known-style transitions longer than [`MAX_STYLE_STEP`].
    pub style_breaks: usize,
    /// Transitions where at least one occurrence has no usable genre.
    pub unknown_transitions: usize,
    /// Selection occurrences without usable genre metadata.
    pub unknown_tracks: usize,
    /// Occurrences whose genre, artist or label metadata reached a safety limit.
    pub limited_tracks: usize,
    /// Distinct canonical genre names omitted from the bounded graph.
    pub omitted_genres: usize,
}

/// One reported style discontinuity in a planned order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StyleBreak {
    /// Position of `next` in [`SmartOrder::order`], including a cycle boundary.
    pub position: usize,
    /// Previous occurrence's index in the original selection.
    pub previous: usize,
    /// Next occurrence's index in the original selection.
    pub next: usize,
    /// Heuristic genre distance, or `None` when metadata is missing.
    pub distance: Option<u16>,
}

/// A reproducible occurrence permutation and its limitations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmartOrder {
    /// Indices into the original selection, each appearing exactly once.
    pub order: Vec<usize>,
    /// Summary suitable for a player or its diagnostics.
    pub report: SmartShuffleReport,
    /// Discontinuities, including unknown genre transitions.
    pub breaks: Vec<StyleBreak>,
}

/// Read-only metadata prepared once for repeated journey planning.
#[derive(Clone, Debug)]
pub struct SmartShuffle {
    profiles: Vec<Profile>,
    graph: GenreGraph,
    report: SmartShuffleReport,
}

impl SmartShuffle {
    /// Prepare a selection using catalog identities when they are available.
    ///
    /// `None` and invalid track IDs retain their occurrences with unknown
    /// metadata. Genres attached directly to a track take precedence over the
    /// release's union of genres, so a mixed compilation does not make all its
    /// tracks stylistically identical. The graph has at most 512 nodes; profiles
    /// retain at most eight genres, eight artists and four labels. Raw and
    /// normalized genre labels over 256 UTF-8 bytes are omitted before planning.
    /// Any limited metadata is counted in the resulting report.
    pub fn from_catalog(catalog: &Catalog, tracks: &[Option<Id>]) -> Self {
        let (mut profiles, contexts) = profiles::from_catalog(catalog, tracks);
        let graph = GenreGraph::new(&mut profiles, &contexts);
        let report = SmartShuffleReport {
            unknown_tracks: profiles.iter().filter(|p| p.genres.is_empty()).count(),
            limited_tracks: profiles.iter().filter(|p| p.limited).count(),
            omitted_genres: graph.omitted_genres,
            ..SmartShuffleReport::default()
        };
        Self {
            profiles,
            graph,
            report,
        }
    }

    /// Number of occurrences, including duplicates and uncatalogued files.
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Whether the original selection was empty.
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Genre distance between two selection occurrences.
    ///
    /// Returns `None` for an invalid index or missing genre metadata. Known but
    /// disconnected genres have distance `1000`. Artist, year and label scoring
    /// cannot override this genre distance or turn missing tags into evidence.
    pub fn transition_distance(&self, from: usize, to: usize) -> Option<u16> {
        self.graph
            .distance(self.profiles.get(from)?, self.profiles.get(to)?)
    }

    /// Plan one complete journey with an optional pinned first occurrence.
    ///
    /// `start_index` keeps a player's current occurrence first when changing
    /// mode. `previous_index` supplies the preceding cycle's final occurrence,
    /// so the next cycle prefers a smooth opening and reports its boundary too.
    /// Out-of-range optional indices are ignored. Every occurrence still appears
    /// once. No random source, wall clock, hash iteration or floating point is
    /// involved; all choices use the same SplitMix64 generator as classic shuffle.
    pub fn plan(
        &self,
        seed: u64,
        start_index: Option<usize>,
        previous_index: Option<usize>,
    ) -> SmartOrder {
        let remaining: Vec<usize> = (0..self.len()).collect();
        self.plan_selection(seed, start_index, previous_index, &remaining)
    }

    /// Plan only the still-unplayed occurrences after an interactive mode change.
    ///
    /// Indices refer to the original selection. Duplicate indices or indices
    /// outside that selection are errors; duplicate **track identities** remain
    /// valid when they are distinct playlist occurrences. Planning the remainder
    /// itself, rather than filtering a full plan afterwards, prevents already
    /// played bridge tracks from influencing a transition they cannot provide.
    pub fn plan_remaining(
        &self,
        seed: u64,
        remaining: &[usize],
        previous_index: Option<usize>,
    ) -> Result<SmartOrder, String> {
        let mut seen = BTreeSet::new();
        for &index in remaining {
            if index >= self.len() {
                return Err("smart shuffle occurrence is outside the selection".into());
            }
            if !seen.insert(index) {
                return Err("smart shuffle remainder repeats an occurrence index".into());
            }
        }
        Ok(self.plan_selection(seed, None, previous_index, remaining))
    }

    fn plan_selection(
        &self,
        seed: u64,
        start_index: Option<usize>,
        previous_index: Option<usize>,
        remaining: &[usize],
    ) -> SmartOrder {
        let mut priorities = remaining.to_vec();
        super::shuffle(&mut priorities, seed);
        let mut result = SmartOrder {
            order: Vec::with_capacity(remaining.len()),
            report: SmartShuffleReport {
                unknown_tracks: remaining
                    .iter()
                    .filter(|&&i| self.profiles[i].genres.is_empty())
                    .count(),
                limited_tracks: remaining
                    .iter()
                    .filter(|&&i| self.profiles[i].limited)
                    .count(),
                omitted_genres: self.report.omitted_genres,
                ..SmartShuffleReport::default()
            },
            breaks: Vec::new(),
        };
        if remaining.is_empty() {
            return result;
        }

        let mut previous = previous_index.filter(|&i| i < self.len());
        let mut pinned = start_index.filter(|i| remaining.contains(i));
        if result.report.unknown_tracks == remaining.len() {
            if let Some(pinned) = pinned
                && let Some(position) = priorities.iter().position(|&i| i == pinned)
            {
                priorities.swap(0, position);
            }
            for next in priorities {
                if let Some(previous) = previous {
                    self.record_transition(previous, next, &mut result);
                }
                result.order.push(next);
                previous = Some(next);
            }
            return result;
        }

        let mut pools = Pools::new(&self.profiles, &priorities, self.graph.len());
        let mut state = seed;
        let mut history = VecDeque::new();
        if let Some(previous) = previous {
            history.push_back(previous);
        }
        while !pools.remaining.is_empty() {
            let next = if let Some(index) = pinned.take() {
                index
            } else if let Some(previous) = previous {
                let candidates =
                    pools.candidates(&self.profiles[previous], &self.graph, &mut state);
                self.choose(previous, &candidates, &pools, &history, &mut state)
            } else {
                // Classic seeded priority gives an unbiased initial occurrence.
                priorities[0]
            };
            if let Some(previous) = previous {
                self.record_transition(previous, next, &mut result);
            }
            pools.remove(next, &self.profiles[next]);
            result.order.push(next);
            history.push_back(next);
            if history.len() > RECENT_ARTISTS {
                history.pop_front();
            }
            previous = Some(next);
        }
        result
    }

    fn record_transition(&self, previous: usize, next: usize, result: &mut SmartOrder) {
        let distance = self.transition_distance(previous, next);
        match distance {
            Some(distance) if distance <= MAX_STYLE_STEP => return,
            Some(_) => result.report.style_breaks += 1,
            None => result.report.unknown_transitions += 1,
        }
        result.breaks.push(StyleBreak {
            position: result.order.len(),
            previous,
            next,
            distance,
        });
    }

    fn choose(
        &self,
        previous: usize,
        candidates: &[usize],
        pools: &Pools,
        history: &VecDeque<usize>,
        state: &mut u64,
    ) -> usize {
        let radius = candidates
            .iter()
            .filter_map(|&i| self.transition_distance(previous, i))
            .min()
            .filter(|&d| d <= MAX_STYLE_STEP)
            .map(|d| d.saturating_add(NEIGHBORHOOD_WINDOW).min(MAX_STYLE_STEP));
        let has_known = candidates
            .iter()
            .any(|&i| self.transition_distance(previous, i).is_some());
        let mut weighted = Vec::with_capacity(candidates.len());
        let mut total = 0u64;
        for &next in candidates {
            let distance = self.transition_distance(previous, next);
            if radius.is_some_and(|radius| !distance.is_some_and(|d| d <= radius)) {
                continue;
            }
            if radius.is_none() && has_known && distance.is_none() {
                continue;
            }
            let weight = self.weight(previous, next, distance, pools, history);
            total += weight;
            weighted.push((next, weight));
        }
        // Pools always supplies at least one remaining candidate; positive
        // weights guarantee a choice without a platform-sensitive RNG.
        let mut draw = bounded_random(state, total);
        for (index, weight) in weighted {
            if draw < weight {
                return index;
            }
            draw -= weight;
        }
        candidates.first().copied().unwrap_or(previous)
    }

    fn weight(
        &self,
        previous: usize,
        next: usize,
        distance: Option<u16>,
        pools: &Pools,
        history: &VecDeque<usize>,
    ) -> u64 {
        let profile = &self.profiles[next];
        let before = &self.profiles[previous];
        let distance = u64::from(distance.unwrap_or(600));
        let closeness = 1_100 - distance;
        let mut weight = closeness * closeness;

        // Metadata can refine a genre-safe step, never permit a genre jump.
        if intersects(&profile.labels, &before.labels) {
            weight += weight / 10;
        }
        if let (Some(a), Some(b)) = (profile.year, before.year) {
            let gap = a.abs_diff(b).min(50);
            weight += weight * u64::from(50 - gap) / 500;
        }
        if self.graph.collaborators(profile, before) {
            weight += weight / 8;
        }

        for (age, &recent) in history.iter().rev().enumerate() {
            let recent = &self.profiles[recent];
            if !profile.artists.is_empty() && intersects(&profile.artists, &recent.artists) {
                weight = weight * (age as u64 + 1) / (age as u64 + 6);
            }
            if age < RECENT_ALBUMS && profile.album.is_some() && profile.album == recent.album {
                weight = weight * (age as u64 + 1) / (age as u64 + 5);
            }
        }
        if profile.identity.is_some()
            && history
                .iter()
                .any(|&recent| self.profiles[recent].identity == profile.identity)
        {
            weight /= 16;
        }

        if pools.remaining.len() > 1
            && pools.onward_distance(profile, next, &self.graph) > MAX_STYLE_STEP
        {
            // Preserve scarce exits when taking this occurrence now would
            // strand the next step. This bounded one-step lookahead is a
            // preference, not a Hamiltonian-path guarantee.
            weight /= 16;
        }
        weight.max(1)
    }
}

fn intersects<T: Ord>(left: &[T], right: &[T]) -> bool {
    let (mut a, mut b) = (0, 0);
    while a < left.len() && b < right.len() {
        match left[a].cmp(&right[b]) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

fn bounded_random(state: &mut u64, bound: u64) -> u64 {
    if bound <= 1 {
        return 0;
    }
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = super::next_random(state);
        if value >= threshold {
            return value % bound;
        }
    }
}

struct Pools {
    priorities: Vec<usize>,
    ranks: Vec<usize>,
    remaining: BTreeSet<usize>,
    genres: Vec<BTreeSet<usize>>,
    active_genres: BTreeSet<usize>,
    unknown: BTreeSet<usize>,
}

impl Pools {
    fn new(profiles: &[Profile], priorities: &[usize], genres: usize) -> Self {
        let mut pools = Self {
            priorities: priorities.to_vec(),
            ranks: vec![0; profiles.len()],
            remaining: (0..priorities.len()).collect(),
            genres: vec![BTreeSet::new(); genres],
            active_genres: BTreeSet::new(),
            unknown: BTreeSet::new(),
        };
        for (rank, &index) in priorities.iter().enumerate() {
            pools.ranks[index] = rank;
            if profiles[index].genres.is_empty() {
                pools.unknown.insert(rank);
            }
            for &genre in &profiles[index].genres {
                pools.genres[genre].insert(rank);
                pools.active_genres.insert(genre);
            }
        }
        pools
    }

    fn remove(&mut self, index: usize, profile: &Profile) {
        let rank = self.ranks[index];
        self.remaining.remove(&rank);
        self.unknown.remove(&rank);
        for &genre in &profile.genres {
            self.genres[genre].remove(&rank);
            if self.genres[genre].is_empty() {
                self.active_genres.remove(&genre);
            }
        }
    }

    fn candidates(&self, previous: &Profile, graph: &GenreGraph, state: &mut u64) -> Vec<usize> {
        let mut ranks = BTreeSet::new();
        let pivot = bounded_random(state, self.priorities.len() as u64) as usize;
        if previous.genres.is_empty() {
            sample_pool(&self.remaining, pivot, CANDIDATE_LIMIT, &mut ranks);
            return ranks
                .into_iter()
                .map(|rank| self.priorities[rank])
                .collect();
        }
        let mut nearby: Vec<(u16, usize)> = self
            .active_genres
            .iter()
            .map(|&genre| (graph.to_genre(previous, genre), genre))
            .collect();
        nearby.sort_unstable();
        for (_, genre) in nearby.into_iter().take(NEARBY_GENRES) {
            sample_pool(&self.genres[genre], pivot, PER_GENRE_CANDIDATES, &mut ranks);
        }
        sample_pool(&self.unknown, pivot, 8, &mut ranks);
        sample_pool(&self.remaining, pivot, 16, &mut ranks);
        ranks
            .into_iter()
            .take(CANDIDATE_LIMIT)
            .map(|rank| self.priorities[rank])
            .collect()
    }

    fn onward_distance(&self, profile: &Profile, index: usize, graph: &GenreGraph) -> u16 {
        let rank = self.ranks[index];
        let mut best = 1_000;
        for &source in &profile.genres {
            for &genre in graph.nearest(source) {
                let distance = graph.genre_distance(source, genre);
                if distance >= best {
                    break;
                }
                let pool = &self.genres[genre];
                if pool.len() > usize::from(pool.contains(&rank)) {
                    best = distance;
                    break;
                }
            }
        }
        best
    }
}

fn sample_pool(pool: &BTreeSet<usize>, pivot: usize, limit: usize, output: &mut BTreeSet<usize>) {
    for &rank in pool.range(pivot..).chain(pool.range(..pivot)).take(limit) {
        output.insert(rank);
    }
}

#[cfg(test)]
#[path = "shuffle_tests.rs"]
mod tests;
