use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::model::{Catalog, EntityKind, Id};

use super::graph::canonical_genre;

pub(super) const MAX_GENRES: usize = 8;
pub(super) const MAX_GENRE_BYTES: usize = 256;
const MAX_ARTISTS: usize = 8;
const MAX_LABELS: usize = 4;

#[derive(Clone, Debug, Default)]
pub(super) struct Profile {
    pub identity: Option<Id>,
    pub genre_names: Vec<Arc<str>>,
    pub genres: Vec<usize>,
    pub artists: Vec<Id>,
    pub album: Option<Id>,
    pub year: Option<u32>,
    pub labels: Vec<Id>,
    pub limited: bool,
}

/// Local evidence is kept by source kind so a compilation or prolific artist
/// cannot manufacture the same strong affinity as a multi-tagged recording.
#[derive(Clone, Debug, Default)]
pub(super) struct Contexts {
    pub direct: Vec<Vec<Arc<str>>>,
    pub releases: Vec<Vec<Arc<str>>>,
    pub artists: Vec<Vec<Arc<str>>>,
    pub collaborations: BTreeSet<(Id, Id)>,
}

pub(super) fn from_catalog(catalog: &Catalog, tracks: &[Option<Id>]) -> (Vec<Profile>, Contexts) {
    let mut direct_genres: BTreeMap<Id, BTreeSet<Arc<str>>> = BTreeMap::new();
    let mut release_genres: BTreeMap<Id, BTreeSet<Arc<str>>> = BTreeMap::new();
    let mut cached_genres: BTreeMap<Id, CachedGenre> = BTreeMap::new();
    let mut limited_genres = BTreeSet::new();
    for link in &catalog.genre_links {
        let names = match link.entity_kind {
            EntityKind::Track => direct_genres.entry(link.entity_id).or_default(),
            EntityKind::Release => release_genres.entry(link.entity_id).or_default(),
            _ => continue,
        };
        let cached = cached_genres
            .entry(link.genre_id)
            .or_insert_with(|| cache_genre(catalog, link.genre_id));
        if cached.limited {
            limited_genres.insert((link.entity_kind, link.entity_id));
        }
        if let Some(name) = &cached.name {
            names.insert(Arc::clone(name));
        }
    }
    let mut main_artists: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
    for credit in &catalog.credits {
        if credit.entity_kind == EntityKind::Track
            && matches!(credit.role.as_str(), "main" | "featured")
            && catalog.artist(credit.artist_id).is_some()
        {
            main_artists
                .entry(credit.entity_id)
                .or_default()
                .insert(credit.artist_id);
        }
    }

    let mut profiles = BTreeMap::new();
    let mut release_labels: BTreeMap<Id, (Vec<Id>, bool)> = BTreeMap::new();
    for &id in tracks.iter().flatten() {
        if profiles.contains_key(&id) {
            continue;
        }
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let release = track.release_id.and_then(|id| catalog.release(id));
        let direct = direct_genres.get(&id);
        let fallback = release.filter(|r| !r.is_compilation);
        let genres = direct.or_else(|| fallback.and_then(|r| release_genres.get(&r.id)));
        let mut artists: Vec<Id> = main_artists
            .get(&id)
            .map(|artists| artists.iter().take(MAX_ARTISTS).copied().collect())
            .unwrap_or_default();
        if artists.is_empty()
            && let Some(artist) = release.and_then(|r| r.album_artist_id)
        {
            artists.push(artist);
        }
        let (labels, labels_limited) = release
            .map(|r| {
                release_labels
                    .entry(r.id)
                    .or_insert_with(|| bounded_labels(&r.label_ids))
                    .clone()
            })
            .unwrap_or_default();
        let source_limited = if direct.is_some() {
            limited_genres.contains(&(EntityKind::Track, id))
        } else {
            fallback.is_some_and(|r| limited_genres.contains(&(EntityKind::Release, r.id)))
        };
        let limited = genres.is_some_and(|g| g.len() > MAX_GENRES)
            || source_limited
            || main_artists
                .get(&id)
                .is_some_and(|artists| artists.len() > MAX_ARTISTS)
            || labels_limited;
        profiles.insert(
            id,
            Profile {
                identity: Some(id),
                genre_names: genres
                    .map(|names| names.iter().take(MAX_GENRES).cloned().collect())
                    .unwrap_or_default(),
                artists,
                album: track.release_id,
                year: release.and_then(|r| r.year),
                labels,
                limited,
                ..Profile::default()
            },
        );
    }

    let mut contexts = Contexts::default();
    // One genre-set observation per release avoids treating a long album's
    // identical tags as dozens of independent corroborating witnesses.
    let mut observations = BTreeSet::new();
    let mut artist_genres: BTreeMap<Id, BTreeSet<Arc<str>>> = BTreeMap::new();
    for (&track_id, genres) in &direct_genres {
        let Some(track) = catalog.track(track_id) else {
            continue;
        };
        let names: Vec<_> = genres.iter().take(MAX_GENRES).cloned().collect();
        if observations.insert((track.release_id, names.clone())) {
            contexts.direct.push(names.clone());
        }
        if let Some(artists) = main_artists.get(&track_id) {
            for artist in artists.iter().take(MAX_ARTISTS) {
                artist_genres
                    .entry(*artist)
                    .or_default()
                    .extend(names.iter().cloned());
            }
        }
    }
    contexts.releases = release_genres
        .into_iter()
        .filter(|(id, genres)| {
            genres.len() <= MAX_GENRES && catalog.release(*id).is_some_and(|r| !r.is_compilation)
        })
        .map(|(_, genres)| genres.into_iter().collect())
        .collect();
    contexts.artists = artist_genres
        .into_values()
        .filter(|genres| genres.len() <= MAX_GENRES)
        .map(|genres| genres.into_iter().collect())
        .collect();
    contexts.collaborations = catalog
        .relations
        .iter()
        .filter(|r| {
            r.source_kind == EntityKind::Artist
                && r.target_kind == EntityKind::Artist
                && r.kind == "collaborated"
        })
        .map(|r| (r.source_id.min(r.target_id), r.source_id.max(r.target_id)))
        .collect();

    let selected = tracks
        .iter()
        .map(|id| {
            id.and_then(|id| profiles.get(&id).cloned())
                .unwrap_or_default()
        })
        .collect();
    (selected, contexts)
}

fn bounded_labels(labels: &[Id]) -> (Vec<Id>, bool) {
    // Keep the same lowest sorted unique IDs as full sort/dedup/truncate, but
    // with constant temporary space and one scan per selected release.
    let mut selected = Vec::with_capacity(MAX_LABELS);
    let mut limited = false;
    for &label in labels {
        match selected.binary_search(&label) {
            Ok(_) => {}
            Err(position) if selected.len() < MAX_LABELS => selected.insert(position, label),
            Err(position) => {
                limited = true;
                if position < MAX_LABELS {
                    selected.pop();
                    selected.insert(position, label);
                }
            }
        }
    }
    (selected, limited)
}

#[derive(Default)]
struct CachedGenre {
    name: Option<Arc<str>>,
    limited: bool,
}

fn cache_genre(catalog: &Catalog, id: Id) -> CachedGenre {
    let Some(genre) = catalog.genre(id) else {
        return CachedGenre::default();
    };
    // Reject oversized input before normalization can allocate several copies.
    // Short names are interned once and shared across placements and contexts.
    if genre.name.len() > MAX_GENRE_BYTES {
        return CachedGenre {
            name: None,
            limited: true,
        };
    }
    let name = canonical_genre(&genre.name);
    if name.len() > MAX_GENRE_BYTES {
        return CachedGenre {
            name: None,
            limited: true,
        };
    }
    CachedGenre {
        name: (!name.is_empty()).then(|| Arc::from(name)),
        limited: false,
    }
}
