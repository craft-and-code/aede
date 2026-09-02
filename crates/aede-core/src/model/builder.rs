//! Turning scanned files into a graph of entities.
//!
//! Construction is **deterministic**: the files are sorted by path before
//! anything else happens, so two scans of the same library produce exactly the
//! same identifiers. Without that there is neither a readable diff between two
//! catalogs nor a reproducible test.
//!
//! The work is one pass per file, with every name interned as it is met, and a
//! `finalize` step at the end for what can only be known once everything is in:
//! an album's year, its cover, the order of its tracks.

use std::collections::{HashMap, HashSet};

use crate::tags::RawTags;
use crate::text;

use super::relations::rebuild_relations;
use super::{
    Artist, AudioFile, Catalog, Credit, EntityKind, Genre, GenreLink, Id, IntegrityRecord, Label,
    Release, Track,
};

// --------------------------------------------------------------------------
// Building the catalog
// --------------------------------------------------------------------------

/// A file read by the scanner, before entity matching.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Absolute path of the file that was read.
    pub path: String,
    /// Size in bytes at the moment of the scan.
    pub size: u64,
    /// Modification date, in seconds since the Unix epoch; with `size`, it is
    /// what lets a later scan leave an unchanged file alone.
    pub mtime: u64,
    /// Tags and stream properties, exactly as the decoder handed them over.
    pub tags: RawTags,
    /// Cover art found in the folder (`cover.jpg`, `folder.png`…).
    pub folder_cover: Option<String>,
    /// A `.lrc` file sitting beside this one, when the walk saw one.
    pub sidecar: Option<String>,
    /// Integrity verdict carried over from the previous catalog, when the file
    /// was reused unchanged. A freshly read file has none: it has to be checked
    /// again.
    pub integrity: Option<IntegrityRecord>,
}

/// Assembles the graph from the files that were read.
///
/// Processing order is deterministic — files are sorted by path first — so two
/// scans of the same library produce exactly the same identifiers. Without
/// that, neither a readable diff nor a reproducible test is possible.
pub fn build(mut scanned: Vec<ScannedFile>, roots: Vec<String>, scanned_at: u64) -> Catalog {
    scanned.sort_by(|a, b| a.path.cmp(&b.path));

    let mut builder = Builder::new(roots, scanned_at);
    for item in &scanned {
        builder.add_file(item);
    }
    builder.finish()
}

/// Interning state shared by every step of the construction.
///
/// Each map answers "have I already seen this name?" in constant time. Doing
/// it with a linear scan made construction quadratic on real libraries.
struct Builder {
    catalog: Catalog,
    artists: HashMap<String, Id>,
    labels: HashMap<String, Id>,
    genres: HashMap<String, Id>,
    releases: HashMap<String, Id>,
    credits: HashSet<CreditKey>,
    release_genres: HashSet<(Id, Id)>,
}

/// Everything one file contributes, resolved before the entities are wired up.
struct FileEntities {
    file_id: Id,
    artist_ids: Vec<Id>,
    album_artist_ids: Vec<Id>,
    release_id: Option<Id>,
    is_compilation: bool,
    /// The disc the file's own folder announces, when it announces one, so
    /// that a rip which put each disc in its own folder and left the tag
    /// empty still knows which disc a track is on.
    disc_from_folder: Option<u32>,
}

impl Builder {
    fn new(roots: Vec<String>, scanned_at: u64) -> Builder {
        Builder {
            catalog: Catalog {
                roots,
                scanned_at,
                ..Default::default()
            },
            artists: HashMap::new(),
            labels: HashMap::new(),
            genres: HashMap::new(),
            releases: HashMap::new(),
            credits: HashSet::new(),
            release_genres: HashSet::new(),
        }
    }

    /// Turns one scanned file into a file row, a track, and all the entities
    /// and links its tags imply.
    fn add_file(&mut self, item: &ScannedFile) {
        let entities = self.add_entities(item);
        let track_id = self.add_track(item, &entities);
        self.add_credits(item, &entities, track_id);
        self.add_labels(item, &entities);
        self.add_genres(item, &entities, track_id);
    }

    /// Records the physical file and resolves the artists and the release.
    fn add_entities(&mut self, item: &ScannedFile) -> FileEntities {
        let tags = &item.tags;
        let file_id = self.catalog.files.len() as Id;
        let file = AudioFile {
            id: file_id,
            path: item.path.clone(),
            size: item.size,
            mtime: item.mtime,
            properties: tags.properties.clone(),
            has_embedded_art: tags.has_embedded_art,
            tags: tags.fields.clone(),
            lyrics_path: item.sidecar.clone(),
            integrity: item.integrity.clone(),
        };
        // Where the *release* lives, which is not always where the file does.
        // A box set laid out as `Album/Disc 1`, `Album/Disc 2` is one album:
        // the disc folder is a subdivision of the release, not another edition
        // of it, and keying the release on it split one soundtrack into two
        // albums of the same name each numbering its tracks from one.
        let file_folder = file.folder().to_string();
        let disc_from_folder = text::disc_folder(text::file_name(&file_folder));
        let folder = match disc_from_folder {
            Some(_) => text::folder(&file_folder).to_string(),
            None => file_folder.clone(),
        };
        self.catalog.files.push(file);

        let artist_names = split_all(tags.all("artist"));
        let album_artist_names = split_all(tags.all("albumartist"));

        let artist_ids: Vec<Id> = artist_names.iter().map(|n| self.intern_artist(n)).collect();
        let album_artist_ids: Vec<Id> = album_artist_names
            .iter()
            .filter(|n| !is_various_artists(n))
            .map(|n| self.intern_artist(n))
            .collect();

        // MBIDs encountered along the way enrich the primary artist.
        if let (Some(mbid), Some(&first)) = (tags.first("musicbrainz_artistid"), artist_ids.first())
            && let Some(artist) = self.catalog.artists.get_mut(first as usize)
        {
            artist.mbid.get_or_insert_with(|| mbid.to_string());
        }

        let is_compilation = tags.first("compilation").is_some()
            || album_artist_names.iter().any(|n| is_various_artists(n));
        let release_id = self.release_for(
            item,
            &folder,
            &artist_ids,
            &album_artist_ids,
            is_compilation,
        );

        FileEntities {
            file_id,
            artist_ids,
            album_artist_ids,
            release_id,
            is_compilation,
            disc_from_folder,
        }
    }

    /// Finds or creates the release this file belongs to.
    fn release_for(
        &mut self,
        item: &ScannedFile,
        folder: &str,
        artist_ids: &[Id],
        album_artist_ids: &[Id],
        is_compilation: bool,
    ) -> Option<Id> {
        let tags = &item.tags;
        let title = tags.first("album").map(|s| s.trim().to_string())?;

        let album_artist = if is_compilation {
            // A compilation has no album artist: that is what distinguishes it
            // from an ordinary album.
            None
        } else {
            // With no declared album artist, fall back to the track artist.
            album_artist_ids
                .first()
                .copied()
                .or_else(|| artist_ids.first().copied())
        };

        let owner = if is_compilation {
            "__va__".to_string()
        } else {
            album_artist
                .and_then(|id| self.catalog.artist(id).map(|a| a.key.clone()))
                .unwrap_or_default()
        };
        // The folder separates two same-named albums by the same artist, for
        // instance two editions of one record.
        let key = format!("{}|{}|{}", text::normalize(&title), owner, folder);

        if let Some(&id) = self.releases.get(&key) {
            // A field missing from the first track may show up on another one.
            if let Some(release) = self.catalog.releases.get_mut(id as usize) {
                if release.cover_path.is_none() {
                    release.cover_path = item.folder_cover.clone();
                }
                if release.catalog_number.is_none() {
                    release.catalog_number = tags.first("catalognumber").map(String::from);
                }
                if release.mbid.is_none() {
                    release.mbid = tags.first("musicbrainz_albumid").map(String::from);
                }
            }
            return Some(id);
        }

        let id = self.catalog.releases.len() as Id;
        self.catalog.releases.push(Release {
            id,
            key: text::normalize(&title),
            title,
            album_artist_id: album_artist,
            date: None,
            year: None,
            label_ids: Vec::new(),
            catalog_number: tags.first("catalognumber").map(String::from),
            barcode: tags.first("barcode").map(String::from),
            media: tags.first("media").map(String::from),
            mbid: tags.first("musicbrainz_albumid").map(String::from),
            release_group_mbid: tags.first("musicbrainz_releasegroupid").map(String::from),
            is_compilation,
            folder: folder.to_string(),
            cover_path: item.folder_cover.clone(),
            track_ids: Vec::new(),
        });
        self.releases.insert(key, id);
        Some(id)
    }

    /// Creates the track and attaches it to its release.
    fn add_track(&mut self, item: &ScannedFile, entities: &FileEntities) -> Id {
        let tags = &item.tags;
        let title = tags
            .first("title")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| title_from_filename(&item.path));

        let (track_no, _) = tags
            .first("tracknumber")
            .map(text::parse_track_number)
            .unwrap_or((None, None));
        let (disc_no, _) = tags
            .first("discnumber")
            .map(text::parse_track_number)
            .unwrap_or((None, None));
        // A rip that put each disc in its own folder and left the tag empty is
        // common, and without this the two discs would both be disc one — two
        // track 1s in one release, which is worse than the split it replaces.
        // The tag still wins where there is one: it is what the person who
        // made the file said.
        let disc_no = disc_no.or(entities.disc_from_folder);

        let track_id = self.catalog.tracks.len() as Id;
        self.catalog.tracks.push(Track {
            id: track_id,
            file_id: entities.file_id,
            release_id: entities.release_id,
            title,
            disc_no,
            track_no: track_no.or_else(|| track_from_filename(&item.path)),
            duration_ms: tags.properties.duration_ms,
            isrc: tags.first("isrc").map(String::from),
            mbid: tags.first("musicbrainz_recordingid").map(String::from),
        });

        if let Some(rid) = entities.release_id
            && let Some(release) = self.catalog.releases.get_mut(rid as usize)
        {
            release.track_ids.push(track_id);
        }
        track_id
    }

    /// Records who did what on this track, and who signs the album.
    fn add_credits(&mut self, item: &ScannedFile, entities: &FileEntities, track_id: Id) {
        for &artist_id in &entities.artist_ids {
            self.push_credit(artist_id, EntityKind::Track, track_id, "main");
        }
        for role in ROLE_TAGS {
            for value in item.tags.all(role) {
                for name in text::split_artists(value) {
                    let id = self.intern_artist(&name);
                    self.push_credit(id, EntityKind::Track, track_id, role);
                }
            }
        }
        if let Some(rid) = entities.release_id
            && !entities.is_compilation
        {
            for &artist_id in &entities.album_artist_ids {
                self.push_credit(artist_id, EntityKind::Release, rid, "album");
            }
        }
    }

    fn add_labels(&mut self, item: &ScannedFile, entities: &FileEntities) {
        let Some(rid) = entities.release_id else {
            return;
        };
        for value in item.tags.all("label") {
            let label_id = self.intern_label(value);
            if let Some(release) = self.catalog.releases.get_mut(rid as usize)
                && !release.label_ids.contains(&label_id)
            {
                release.label_ids.push(label_id);
            }
        }
    }

    /// Attaches genres to the track, and to its release the first time round.
    fn add_genres(&mut self, item: &ScannedFile, entities: &FileEntities, track_id: Id) {
        for value in item.tags.all("genre") {
            for name in value.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                let genre_id = self.intern_genre(name);
                self.catalog.genre_links.push(GenreLink {
                    genre_id,
                    entity_kind: EntityKind::Track,
                    entity_id: track_id,
                });
                if let Some(rid) = entities.release_id
                    && self.release_genres.insert((genre_id, rid))
                {
                    self.catalog.genre_links.push(GenreLink {
                        genre_id,
                        entity_kind: EntityKind::Release,
                        entity_id: rid,
                    });
                }
            }
        }
    }

    fn intern_artist(&mut self, name: &str) -> Id {
        let key = text::normalize(name);
        if let Some(&id) = self.artists.get(&key) {
            return id;
        }
        let id = self.catalog.artists.len() as Id;
        self.catalog.artists.push(Artist {
            id,
            name: name.trim().to_string(),
            sort_name: text::sort_name(name.trim()),
            key: key.clone(),
            mbid: None,
        });
        self.artists.insert(key, id);
        id
    }

    fn intern_label(&mut self, name: &str) -> Id {
        let key = text::normalize(name);
        if let Some(&id) = self.labels.get(&key) {
            return id;
        }
        let id = self.catalog.labels.len() as Id;
        self.catalog.labels.push(Label {
            id,
            name: name.trim().to_string(),
            key: key.clone(),
        });
        self.labels.insert(key, id);
        id
    }

    fn intern_genre(&mut self, name: &str) -> Id {
        let key = text::normalize(name);
        if let Some(&id) = self.genres.get(&key) {
            return id;
        }
        let id = self.catalog.genres.len() as Id;
        self.catalog.genres.push(Genre {
            id,
            name: name.trim().to_string(),
            key: key.clone(),
        });
        self.genres.insert(key, id);
        id
    }

    /// Adds a credit unless the same one is already recorded.
    fn push_credit(&mut self, artist_id: Id, kind: EntityKind, id: Id, role: &str) {
        if self.credits.insert((artist_id, kind, id, role.to_string())) {
            self.catalog.credits.push(Credit {
                artist_id,
                entity_kind: kind,
                entity_id: id,
                role: role.to_string(),
            });
        }
    }

    fn finish(mut self) -> Catalog {
        finalize(&mut self.catalog);
        self.catalog
    }
}

/// Tag names read as credits, each one giving its role its name.
const ROLE_TAGS: &[&str] = &[
    "composer",
    "conductor",
    "remixer",
    "lyricist",
    "performer",
    "producer",
    "engineer",
];

/// Key of a credit, used to reject duplicates in constant time.
type CreditKey = (Id, EntityKind, Id, String);

fn split_all(values: &[String]) -> Vec<String> {
    values.iter().flat_map(|v| text::split_artists(v)).collect()
}

/// `true` for the placeholder names that stand for "no album artist".
///
/// Recording them as artists would pollute every count and every listing.
fn is_various_artists(name: &str) -> bool {
    matches!(
        text::normalize(name).as_str(),
        "various artists" | "various" | "va" | "artistes divers" | "divers" | "multi interpretes"
    )
}

/// Final pass: track ordering, inferred release dates, collaboration graph.
fn finalize(catalog: &mut Catalog) {
    // Order the tracks of each release.
    let order: Vec<(Id, Vec<Id>)> = catalog
        .releases
        .iter()
        .map(|release| {
            let mut ids = release.track_ids.clone();
            ids.sort_by_key(|&id| {
                let t = &catalog.tracks[id as usize];
                (
                    t.disc_no.unwrap_or(1),
                    t.track_no.unwrap_or(u32::MAX),
                    t.title.clone(),
                )
            });
            (release.id, ids)
        })
        .collect();
    for (id, ids) in order {
        catalog.releases[id as usize].track_ids = ids;
    }

    // Release year: the earliest one announced by its tracks.
    let mut years: HashMap<Id, (Option<String>, Option<u32>)> = HashMap::new();
    for track in &catalog.tracks {
        let Some(rid) = track.release_id else {
            continue;
        };
        let file = &catalog.files[track.file_id as usize];
        let raw = file
            .first_tag("originaldate")
            .or_else(|| file.first_tag("date"))
            .unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        let year = text::extract_year(raw);
        let entry = years.entry(rid).or_insert((None, None));
        if entry.1.is_none() || (year.is_some() && year < entry.1) {
            *entry = (Some(raw.to_string()), year);
        }
    }
    for (rid, (date, year)) in years {
        if let Some(release) = catalog.releases.get_mut(rid as usize) {
            release.date = date;
            release.year = year;
        }
    }

    rebuild_relations(catalog);
}

/// Without a `title` tag, fall back to the file name, stripped of its leading
/// track number and its extension.
fn title_from_filename(path: &str) -> String {
    let name = text::file_name(path);
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let trimmed = stem
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches([' ', '-', '.', '_'])
        .trim();
    if trimmed.is_empty() {
        stem.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Track number inferred from a file name that starts with digits.
fn track_from_filename(path: &str) -> Option<u32> {
    let name = text::file_name(path);
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    digits.parse().ok().filter(|&n| n > 0)
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
