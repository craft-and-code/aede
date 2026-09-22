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
    Recording, Release, ReleaseGroup, Track, Work,
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
    /// Fingerprint carried over the same way, and dropped for the same reason:
    /// a file that changed is different audio, and its old fingerprint
    /// describes something that is no longer there.
    pub fingerprint: Option<crate::fingerprint::Fingerprint>,
}

/// Assembles the graph from the files that were read.
///
/// Processing order is deterministic — files are sorted by path first — so two
/// scans of the same library produce exactly the same identifiers. Without
/// that, neither a readable diff nor a reproducible test is possible.
pub fn build(
    mut scanned: Vec<ScannedFile>,
    roots: Vec<String>,
    scanned_at: u64,
    chosen: &[super::identity::Chosen],
) -> Catalog {
    scanned.sort_by(|a, b| a.path.cmp(&b.path));

    // **Read before anything is interned.** Two spellings under one MusicBrainz
    // identifier are one artist, and which spelling survives is decided by the
    // library as a whole — the most frequent one — so it cannot be settled file
    // by file as the walk goes. `chosen` is what the owner of the disk said
    // about the files no identifier can answer for, and it is resolved in the
    // same pass rather than applied over it. See [`super::identity`].
    let aliases = super::identity::aliases(scanned.iter().flat_map(named_once), chosen);

    let mut builder = Builder::new(roots, scanned_at, aliases);
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
    /// Spellings that are somebody else's, worked out before the walk.
    aliases: super::identity::Aliases,
    artists: HashMap<String, Id>,
    labels: HashMap<String, Id>,
    genres: HashMap<String, Id>,
    releases: HashMap<String, Id>,
    release_groups: HashMap<String, Id>,
    recordings: HashMap<String, Id>,
    works: HashMap<String, Id>,
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
    fn new(roots: Vec<String>, scanned_at: u64, aliases: super::identity::Aliases) -> Builder {
        Builder {
            catalog: Catalog {
                roots,
                scanned_at,
                ..Default::default()
            },
            aliases,
            artists: HashMap::new(),
            labels: HashMap::new(),
            genres: HashMap::new(),
            releases: HashMap::new(),
            release_groups: HashMap::new(),
            recordings: HashMap::new(),
            works: HashMap::new(),
            credits: HashSet::new(),
            release_genres: HashSet::new(),
        }
    }

    /// Turns one scanned file into a file row, a track, and all the entities
    /// and links its tags imply.
    fn add_file(&mut self, item: &ScannedFile) {
        let entities = self.add_entities(item);
        let track_id = self.add_track(item, &entities);
        let recording_id = self.catalog.tracks[track_id as usize].recording_id;
        self.add_works(item, recording_id);
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
            fingerprint: item.fingerprint.clone(),
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

        let artist_names = credited(tags, "artists", "artist");
        let album_artist_names = credited(tags, "albumartists", "albumartist");

        let artist_ids: Vec<Id> = artist_names
            .iter()
            .map(|n| {
                let id = self.intern_artist(n);
                self.name_artist(id, n);
                id
            })
            .collect();
        let album_artist_ids: Vec<Id> = album_artist_names
            .iter()
            .filter(|n| !is_various_artists(n))
            .map(|n| {
                let id = self.intern_artist(n);
                self.name_artist(id, n);
                id
            })
            .collect();

        // MBIDs encountered along the way enrich the artist they name — both
        // of them, since an album artist's identifier used to be read by
        // nothing at all and is exactly as good as a track artist's.
        for (ids, tag) in [
            (&artist_ids, "musicbrainz_artistid"),
            (&album_artist_ids, "musicbrainz_albumartistid"),
        ] {
            // One name and one identifier, or nothing — the same rule
            // `named_once` applies, and for the same reason: with two of
            // either, position is not evidence of which belongs to which.
            let (Some(mbid), [id]) = (only_one(tags.all(tag)), ids.as_slice()) else {
                continue;
            };
            if let Some(artist) = self.catalog.artists.get_mut(*id as usize) {
                artist.mbid.get_or_insert_with(|| mbid.to_string());
            }
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
            let missing_group = self
                .catalog
                .releases
                .get(id as usize)
                .filter(|release| release.release_group_id.is_none())
                .map(|release| release.title.clone());
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
            if let Some(title) = missing_group
                && let Some(group_id) =
                    self.release_group_for(&title, tags.first("musicbrainz_releasegroupid"))
            {
                self.catalog.releases[id as usize].release_group_id = Some(group_id);
                self.catalog.release_groups[group_id as usize]
                    .release_ids
                    .push(id);
            }
            return Some(id);
        }

        let id = self.catalog.releases.len() as Id;
        let release_group_mbid = tags.first("musicbrainz_releasegroupid").map(String::from);
        let release_group_id = self.release_group_for(&title, release_group_mbid.as_deref());
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
            release_group_mbid,
            release_group_id,
            is_compilation,
            folder: folder.to_string(),
            cover_path: item.folder_cover.clone(),
            track_ids: Vec::new(),
        });
        if let Some(group_id) = release_group_id {
            self.catalog.release_groups[group_id as usize]
                .release_ids
                .push(id);
        }
        self.releases.insert(key, id);
        Some(id)
    }

    /// Resolves a release group only from its explicit MusicBrainz identifier.
    fn release_group_for(&mut self, title: &str, mbid: Option<&str>) -> Option<Id> {
        let mbid = mbid?.trim();
        if mbid.is_empty() {
            return None;
        }
        if let Some(&id) = self.release_groups.get(mbid) {
            return Some(id);
        }
        let id = self.catalog.release_groups.len() as Id;
        self.catalog.release_groups.push(ReleaseGroup {
            id,
            title: title.to_string(),
            key: text::normalize(title),
            mbid: mbid.to_string(),
            release_ids: Vec::new(),
        });
        self.release_groups.insert(mbid.to_string(), id);
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

        let recording_id = self.recording_for(item, &title);
        let track_id = self.catalog.tracks.len() as Id;
        self.catalog.tracks.push(Track {
            id: track_id,
            file_id: entities.file_id,
            release_id: entities.release_id,
            recording_id,
            title,
            disc_no,
            track_no: track_no.or_else(|| track_from_filename(&item.path)),
            duration_ms: tags.properties.duration_ms,
            isrc: tags.first("isrc").map(String::from),
            mbid: tags.first("musicbrainz_recordingid").map(String::from),
        });
        self.catalog.recordings[recording_id as usize]
            .track_ids
            .push(track_id);

        if let Some(rid) = entities.release_id
            && let Some(release) = self.catalog.releases.get_mut(rid as usize)
        {
            release.track_ids.push(track_id);
        }
        track_id
    }

    /// Finds the recorded performance this placement carries, or gives an
    /// un-identified placement a recording of its own. A shared title is not
    /// enough to merge music: only an explicit MusicBrainz recording ID is.
    fn recording_for(&mut self, item: &ScannedFile, title: &str) -> Id {
        let mbid = item
            .tags
            .first("musicbrainz_recordingid")
            .map(str::to_owned);
        let key = match &mbid {
            Some(id) => format!("mbid:{id}"),
            None => format!("local:{}", item.path),
        };
        if let Some(&id) = self.recordings.get(&key) {
            return id;
        }
        let id = self.catalog.recordings.len() as Id;
        self.catalog.recordings.push(Recording {
            id,
            title: title.to_string(),
            key: text::normalize(title),
            isrc: item.tags.first("isrc").map(String::from),
            mbid,
            track_ids: Vec::new(),
            work_ids: Vec::new(),
        });
        self.recordings.insert(key, id);
        id
    }

    /// Connects one recording to every explicitly identified work its tags
    /// name. Several works are allowed: a medley is not forced into one title.
    fn add_works(&mut self, item: &ScannedFile, recording_id: Id) {
        for (position, mbid) in item.tags.all("musicbrainz_workid").iter().enumerate() {
            let mbid = mbid.trim();
            if mbid.is_empty() {
                continue;
            }
            let work_id = match self.works.get(mbid) {
                Some(&id) => id,
                None => {
                    let title = item
                        .tags
                        .all("work")
                        .get(position)
                        .map(String::as_str)
                        .or_else(|| item.tags.first("work"))
                        .unwrap_or(&self.catalog.recordings[recording_id as usize].title);
                    let id = self.catalog.works.len() as Id;
                    self.catalog.works.push(Work {
                        id,
                        title: title.to_string(),
                        key: text::normalize(title),
                        mbid: mbid.to_string(),
                        recording_ids: Vec::new(),
                    });
                    self.works.insert(mbid.to_string(), id);
                    id
                }
            };
            let recording = &mut self.catalog.recordings[recording_id as usize];
            if !recording.work_ids.contains(&work_id) {
                recording.work_ids.push(work_id);
                self.catalog.works[work_id as usize]
                    .recording_ids
                    .push(recording_id);
            }
        }
    }

    /// Records who did what on this track, and who signs the album.
    fn add_credits(&mut self, item: &ScannedFile, entities: &FileEntities, track_id: Id) {
        for &artist_id in &entities.artist_ids {
            self.push_credit(artist_id, EntityKind::Track, track_id, "main");
        }
        for role in ROLE_TAGS {
            // The whole tag at once, not one value at a time: whether a value
            // restates the others can only be asked of the list it belongs to,
            // and `PERFORMER` is where a collaboration is habitually written
            // out twice — once per musician, then once as the joint credit.
            for name in credited_under(&item.tags, role) {
                let id = self.intern_artist(&name);
                self.push_credit(id, EntityKind::Track, track_id, role);
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
        for (position, value) in item.tags.all("label").iter().enumerate() {
            let mbid = item
                .tags
                .all("musicbrainz_labelid")
                .get(position)
                .map(String::as_str)
                .or_else(|| item.tags.first("musicbrainz_labelid"));
            let label_id = self.intern_label(value, mbid);
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

    /// The artist this spelling belongs to, creating them the first time.
    ///
    /// A spelling MusicBrainz places under another one is filed there, and the
    /// row is **named for the spelling it is filed under** rather than for
    /// whichever file was read first: the key and the displayed name have to
    /// agree, or a listing shows `O. Osbourne` and a query for `ozzy osbourne`
    /// finds it, which is worse than either alone.
    fn intern_artist(&mut self, name: &str) -> Id {
        let spelling = text::normalize(name);
        let key = super::identity::filed_as(&self.aliases, &spelling).to_string();
        if let Some(&id) = self.artists.get(&key) {
            return id;
        }
        // `name` is one of the spellings; the surviving key is another. Naming
        // the row from the key would print a normalised string — lower case,
        // no punctuation — where a reader expects a name, so the *first file
        // carrying the surviving spelling* supplies it. Until one is read, the
        // spelling in hand stands in, and is replaced below.
        let display = match key == spelling {
            true => name.trim().to_string(),
            false => key.clone(),
        };
        // The other spellings that were filed here, kept so the merge can be
        // seen: two rows becoming one is a decision, and a reader who cannot
        // see it cannot tell it from a folder that was never scanned.
        let mut aliases: Vec<String> = self
            .aliases
            .iter()
            .filter(|(_, under)| **under == key)
            .map(|(spelling, _)| spelling.clone())
            .collect();
        aliases.sort();
        let id = self.catalog.artists.len() as Id;
        self.catalog.artists.push(Artist {
            id,
            name: display.clone(),
            sort_name: text::sort_name(&display),
            key: key.clone(),
            mbid: None,
            aliases,
        });
        self.artists.insert(key, id);
        id
    }

    /// Gives an artist the spelling their surviving key was taken from.
    ///
    /// Called with every name read, and it only ever acts once: the row keeps
    /// the first *unnormalised* form of the winning spelling that the walk
    /// meets. Without it a merged artist would be displayed by their key, which
    /// is a matching form and not a name.
    fn name_artist(&mut self, id: Id, name: &str) {
        let Some(artist) = self.catalog.artists.get_mut(id as usize) else {
            return;
        };
        if artist.name != artist.key || text::normalize(name) != artist.key {
            return;
        }
        artist.name = name.trim().to_string();
        artist.sort_name = text::sort_name(name.trim());
    }

    fn intern_label(&mut self, name: &str, mbid: Option<&str>) -> Id {
        let key = text::normalize(name);
        if let Some(&id) = self.labels.get(&key) {
            if self.catalog.labels[id as usize].mbid.is_none() {
                self.catalog.labels[id as usize].mbid = mbid.map(str::to_owned);
            }
            return id;
        }
        let id = self.catalog.labels.len() as Id;
        self.catalog.labels.push(Label {
            id,
            name: name.trim().to_string(),
            key: key.clone(),
            mbid: mbid.map(str::to_owned),
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

/// The unambiguous (identifier, name) pairs one file states.
///
/// A file speaks only where it names **exactly one** artist and carries
/// **exactly one** identifier for them. A tag naming two artists arrives as one
/// string that `split_artists` cuts in two, while the identifiers arrive as
/// their own list in an order nothing guarantees to match — pairing those by
/// position would file an identifier against whichever name sorted first. Both
/// the artist and the album artist are read, since either is evidence.
fn named_once(item: &ScannedFile) -> Vec<super::identity::Said> {
    let mut said = Vec::new();
    for (plural, single, tag) in [
        ("artists", "artist", "musicbrainz_artistid"),
        ("albumartists", "albumartist", "musicbrainz_albumartistid"),
    ] {
        // Resolved exactly as the credit itself is, or the two would disagree
        // about how many artists a file names — and this pre-pass would call a
        // collaboration one artist while the walk called it two.
        let names = credited(&item.tags, plural, single);
        let (Some(mbid), [name]) = (only_one(item.tags.all(tag)), names.as_slice()) else {
            continue;
        };
        if is_various_artists(name) {
            continue;
        }
        said.push(super::identity::Said {
            mbid: mbid.to_string(),
            name: name.clone(),
        });
    }
    said
}

/// The artists a file credits, from the tag written for the purpose.
///
/// **`ARTISTS` before `ARTIST`, and it is not a nicety.** `ARTIST` is one
/// string holding however many names, and the conventions for joining them do
/// not survive contact with a real library: `Rob Zombie & Ozzy Osbourne` is two
/// artists, `Simon & Garfunkel` is one, and nothing in either string says
/// which. `ARTISTS` is the tag taggers write with **one value per artist** for
/// exactly this reason, and a file that carries it has already answered the
/// question — no separator to guess at, no band name to shatter.
///
/// Each value still goes through [`text::split_artists`], because a tagger that
/// writes `ARTISTS` as one joined value is not rare either, and splitting a
/// value that holds one name gives that name back.
fn credited(tags: &RawTags, plural: &str, single: &str) -> Vec<String> {
    let named = credited_under(tags, plural);
    match named.is_empty() {
        false => named,
        true => credited_under(tags, single),
    }
}

/// Every artist one tag names, split and freed of the values that only restate
/// the others.
///
/// The scope matters and is the whole point: [`text::without_restatements`]
/// judges a name against the *rest of its own tag*, so the list has to be
/// gathered before anything is dropped from it. See that function for why a
/// joint credit sitting beside its members is a duplicate rather than a band.
fn credited_under(tags: &RawTags, tag: &str) -> Vec<String> {
    text::without_restatements(split_all(tags.all(tag)))
}

/// The only value there is, when there is exactly one.
fn only_one(values: &[String]) -> Option<&str> {
    match values {
        [one] if !one.trim().is_empty() => Some(one.trim()),
        _ => None,
    }
}

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
