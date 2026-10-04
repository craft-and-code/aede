use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::sync::Arc;

use crate::text;

use super::profiles::{Contexts, Profile};

const MAX_NODES: usize = 512;

/// These are deliberately undirected listening affinities, not parent/child
/// genre definitions or claims of historical descent. Local co-tags can add
/// affinities; the planner never fetches an external genre ontology.
/// Conventional meeting points include Third Stream (classical/jazz):
/// <https://necmusic.edu/on-campus/library/archives-and-special-collections/archival-collections/gunther-schuller/>.
/// Hip hop draws from several styles rather than a single lineage:
/// <https://www.loc.gov/collections/songs-of-america/articles-and-essays/musical-styles/popular-songs-of-the-day/hip-hop-rap/>.
/// The preferred neighbor pairs and numeric costs are planner design choices.
const STYLE_AFFINITIES: &[(&str, &str, u16)] = &[
    ("classical", "chamber music", 90),
    ("classical", "orchestral", 100),
    ("classical", "opera", 160),
    ("classical", "contemporary classical", 120),
    ("classical", "third stream", 180),
    ("classical", "soundtrack", 230),
    ("contemporary classical", "ambient", 230),
    ("third stream", "jazz", 180),
    ("third stream", "avant garde jazz", 160),
    ("jazz", "avant garde jazz", 130),
    ("jazz", "big band", 100),
    ("jazz", "swing", 120),
    ("jazz", "blues", 200),
    ("jazz", "jazz fusion", 140),
    ("jazz", "jazz funk", 170),
    ("jazz", "soul", 230),
    ("jazz", "jazz rap", 230),
    ("jazz fusion", "progressive rock", 220),
    ("jazz fusion", "jazz funk", 140),
    ("blues", "rhythm and blues", 180),
    ("blues", "blues rock", 150),
    ("blues", "folk", 230),
    ("blues", "country", 250),
    ("rhythm and blues", "soul", 110),
    ("rhythm and blues", "gospel", 190),
    ("rhythm and blues", "hip hop", 240),
    ("soul", "gospel", 150),
    ("soul", "neo soul", 90),
    ("soul", "funk", 160),
    ("neo soul", "hip hop", 200),
    ("funk", "jazz funk", 120),
    ("funk", "disco", 130),
    ("funk", "funk rock", 160),
    ("funk", "hip hop", 220),
    ("hip hop", "jazz rap", 120),
    ("hip hop", "rap", 90),
    ("hip hop", "trip hop", 170),
    ("hip hop", "electronic", 250),
    ("trip hop", "downtempo", 110),
    ("trip hop", "electronic", 170),
    ("folk", "folk rock", 130),
    ("folk", "country", 180),
    ("folk", "singer songwriter", 120),
    ("country", "country rock", 120),
    ("country rock", "rock", 180),
    ("blues rock", "rock", 100),
    ("folk rock", "rock", 170),
    ("rock", "pop rock", 140),
    ("rock", "progressive rock", 150),
    ("rock", "alternative rock", 120),
    ("rock", "hard rock", 130),
    ("rock", "punk", 180),
    ("rock", "funk rock", 170),
    ("alternative rock", "indie rock", 100),
    ("alternative rock", "grunge", 140),
    ("alternative rock", "post rock", 160),
    ("post rock", "ambient", 230),
    ("punk", "post punk", 150),
    ("post punk", "new wave", 160),
    ("hard rock", "heavy metal", 160),
    ("heavy metal", "metal", 90),
    ("heavy metal", "thrash metal", 140),
    ("heavy metal", "power metal", 150),
    ("heavy metal", "doom metal", 170),
    ("metal", "progressive metal", 150),
    ("progressive metal", "progressive rock", 180),
    ("thrash metal", "death metal", 180),
    ("death metal", "black metal", 230),
    ("death metal", "melodic death metal", 110),
    ("black metal", "symphonic metal", 240),
    ("symphonic metal", "orchestral", 280),
    ("pop rock", "pop", 140),
    ("pop", "synthpop", 130),
    ("pop", "dance pop", 150),
    ("pop", "soul", 240),
    ("new wave", "synthpop", 140),
    ("synthpop", "electronic", 160),
    ("electronic", "ambient", 180),
    ("electronic", "downtempo", 130),
    ("electronic", "house", 120),
    ("electronic", "techno", 130),
    ("electronic", "breakbeat", 150),
    ("electronic", "drum and bass", 180),
    ("house", "disco", 200),
    ("house", "dance pop", 200),
    ("house", "techno", 160),
    ("reggae", "dub", 120),
    ("reggae", "ska", 160),
    ("dub", "trip hop", 250),
    ("dub", "electronic", 260),
    ("ska", "punk", 220),
    ("latin", "latin jazz", 140),
    ("latin jazz", "jazz", 180),
    ("latin", "salsa", 130),
    ("latin", "bossa nova", 170),
    ("bossa nova", "jazz", 210),
];

pub(super) fn canonical_genre(name: &str) -> String {
    let normalized = text::normalize(name);
    match normalized.as_str() {
        "r b" | "r n b" | "rnb" | "rhythm blues" | "rhythm and blues" => "rhythm and blues".into(),
        "hiphop" | "hip hop music" => "hip hop".into(),
        "drum n bass" | "drum bass" | "dnb" | "d n b" => "drum and bass".into(),
        "synth pop" => "synthpop".into(),
        "trip hop music" | "triphop" => "trip hop".into(),
        "jazzfusion" | "fusion jazz" => "jazz fusion".into(),
        "classical music" | "classique" | "musique classique" => "classical".into(),
        "soundtracks" | "film score" | "film music" => "soundtrack".into(),
        "rap music" => "rap".into(),
        _ => normalized,
    }
}

#[derive(Clone, Debug)]
pub(super) struct GenreGraph {
    distances: Vec<Vec<u16>>,
    nearest: Vec<Vec<usize>>,
    collaborators: BTreeSet<(u32, u32)>,
    pub omitted_genres: usize,
}

impl GenreGraph {
    pub fn new(profiles: &mut [Profile], contexts: &Contexts) -> Self {
        let mut frequencies: BTreeMap<Arc<str>, usize> = BTreeMap::new();
        for profile in profiles.iter() {
            for genre in &profile.genre_names {
                *frequencies.entry(genre.clone()).or_default() += 1;
            }
        }
        let mut names: BTreeSet<Arc<str>> = STYLE_AFFINITIES
            .iter()
            .flat_map(|&(a, b, _)| [Arc::from(a), Arc::from(b)])
            .collect();
        let mut by_frequency: Vec<_> = frequencies.iter().collect();
        by_frequency.sort_by(|(a, ac), (b, bc)| bc.cmp(ac).then(a.cmp(b)));
        for (name, _) in by_frequency {
            if names.len() < MAX_NODES || names.contains(name) {
                names.insert(name.clone());
            }
        }
        let omitted_genres = frequencies
            .keys()
            .filter(|name| !names.contains(*name))
            .count();
        // Only selected tags need an arbitrary custom node. The static backbone
        // supplies empty stepping stones for distance, never playlist tracks.
        let names: Vec<_> = names.into_iter().collect();
        let indices: BTreeMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.as_ref(), index))
            .collect();
        let mut edges: Vec<BTreeMap<usize, u16>> = vec![BTreeMap::new(); names.len()];
        for &(a, b, distance) in STYLE_AFFINITIES {
            if let (Some(&a), Some(&b)) = (indices.get(a), indices.get(b)) {
                link(&mut edges, a, b, distance);
            }
        }
        // An unknown compound genre can meet a recognized complete-word suffix
        // ("technical death metal" -> "death metal"). No substring guessing
        // turns "metallic" into "metal" or "popcorn" into "pop".
        for (index, name) in names.iter().enumerate() {
            for (base, &other) in &indices {
                if name.as_ref() != *base
                    && (name
                        .strip_suffix(base)
                        .is_some_and(|prefix| prefix.ends_with(' '))
                        || name
                            .strip_prefix(base)
                            .is_some_and(|suffix| suffix.starts_with(' ')))
                {
                    link(&mut edges, index, other, 180);
                }
            }
        }
        let backbone: BTreeSet<usize> = STYLE_AFFINITIES
            .iter()
            .flat_map(|&(a, b, _)| [indices.get(a).copied(), indices.get(b).copied()])
            .flatten()
            .collect();
        let protected: Vec<_> = (0..names.len())
            .map(|start| shortest(&edges, start))
            .collect();
        learn(&mut edges, &indices, &contexts.direct, 140, None);
        learn(
            &mut edges,
            &indices,
            &contexts.releases,
            220,
            Some((&backbone, &protected)),
        );
        learn(
            &mut edges,
            &indices,
            &contexts.artists,
            240,
            Some((&backbone, &protected)),
        );

        for profile in profiles {
            profile.genres = profile
                .genre_names
                .iter()
                .filter_map(|name| indices.get(name.as_ref()).copied())
                .collect();
            profile.genres.sort_unstable();
            profile.genres.dedup();
            profile.limited |= profile.genres.len() != profile.genre_names.len();
        }
        let distances: Vec<Vec<u16>> = (0..names.len())
            .map(|start| shortest(&edges, start))
            .collect();
        let nearest = distances
            .iter()
            .map(|row| {
                let mut genres: Vec<_> = (0..row.len()).collect();
                genres.sort_unstable_by_key(|&genre| (row[genre], genre));
                genres
            })
            .collect();
        Self {
            distances,
            nearest,
            collaborators: contexts.collaborations.clone(),
            omitted_genres,
        }
    }

    pub fn len(&self) -> usize {
        self.distances.len()
    }

    pub fn to_genre(&self, profile: &Profile, genre: usize) -> u16 {
        profile
            .genres
            .iter()
            .map(|&source| self.distances[source][genre])
            .min()
            .unwrap_or(1_000)
    }

    pub fn nearest(&self, genre: usize) -> &[usize] {
        &self.nearest[genre]
    }

    pub fn genre_distance(&self, from: usize, to: usize) -> u16 {
        self.distances[from][to]
    }

    pub fn distance(&self, a: &Profile, b: &Profile) -> Option<u16> {
        if a.genres.is_empty() || b.genres.is_empty() {
            return None;
        }
        let mut best = 1_000u32;
        let mut total = 0u32;
        for (&source, target) in a
            .genres
            .iter()
            .map(|g| (g, b))
            .chain(b.genres.iter().map(|g| (g, a)))
        {
            let closest = target
                .genres
                .iter()
                .map(|&genre| u32::from(self.distances[source][genre]))
                .min()
                .unwrap_or(1_000);
            best = best.min(closest);
            total += closest;
        }
        let mean = total / (a.genres.len() + b.genres.len()) as u32;
        Some(((3 * best + mean) / 4) as u16)
    }

    pub fn collaborators(&self, a: &Profile, b: &Profile) -> bool {
        a.artists.iter().any(|&a| {
            b.artists
                .iter()
                .any(|&b| a != b && self.collaborators.contains(&(a.min(b), a.max(b))))
        })
    }
}

fn link(edges: &mut [BTreeMap<usize, u16>], a: usize, b: usize, distance: u16) {
    if a == b {
        return;
    }
    for (from, to) in [(a, b), (b, a)] {
        edges[from]
            .entry(to)
            .and_modify(|cost| *cost = (*cost).min(distance))
            .or_insert(distance);
    }
}

fn learn(
    edges: &mut [BTreeMap<usize, u16>],
    indices: &BTreeMap<&str, usize>,
    contexts: &[Vec<Arc<str>>],
    minimum: u16,
    protected: Option<(&BTreeSet<usize>, &[Vec<u16>])>,
) {
    let mut frequency = vec![0u32; indices.len()];
    let mut pairs: BTreeMap<(usize, usize), u32> = BTreeMap::new();
    for context in contexts {
        let mut genres: Vec<_> = context
            .iter()
            .filter_map(|name| indices.get(name.as_ref()).copied())
            .collect();
        genres.sort_unstable();
        genres.dedup();
        for &genre in &genres {
            frequency[genre] = frequency[genre].saturating_add(1);
        }
        for (position, &a) in genres.iter().enumerate() {
            for &b in &genres[position + 1..] {
                let count = pairs.entry((a, b)).or_default();
                *count = count.saturating_add(1);
            }
        }
    }
    for ((a, b), count) in pairs {
        if let Some((backbone, distances)) = protected {
            if count < 2 {
                continue;
            }
            // Albums and artists can span several unrelated styles. Their
            // weak co-occurrence must not erase a large established style
            // separation; only explicit multi-tagged tracks can do that.
            if backbone.contains(&a)
                && backbone.contains(&b)
                && distances[a][b] > super::MAX_STYLE_STEP
            {
                continue;
            }
        }
        // Integer Sørensen–Dice discounts genres carried by everything. Weak
        // release/artist associations cannot become a safe step on their own.
        let strength = (2u64 * u64::from(count) * 1_000)
            / (u64::from(frequency[a]) + u64::from(frequency[b])).max(1);
        let distance = minimum + ((1_000 - strength.min(1_000)) * 180 / 1_000) as u16;
        link(edges, a, b, distance);
    }
}

fn shortest(edges: &[BTreeMap<usize, u16>], start: usize) -> Vec<u16> {
    let mut distances = vec![1_000; edges.len()];
    distances[start] = 0;
    let mut heap = BinaryHeap::from([Reverse((0u16, start))]);
    while let Some(Reverse((distance, source))) = heap.pop() {
        if distance != distances[source] {
            continue;
        }
        for (&target, &cost) in &edges[source] {
            let next = distance.saturating_add(cost).min(1_000);
            if next < distances[target] {
                distances[target] = next;
                heap.push(Reverse((next, target)));
            }
        }
    }
    distances
}
