//! The data model: a graph of linked entities, not a list of files.
//!
//! This is the central architectural decision of the project. We do not model
//! "an album belongs to an artist" but "entities carry roles with respect to
//! one another". That is what will later make it possible to click on a
//! drummer and see their forty appearances, without redoing anything.
//!
//! Each `Vec` is indexed by identifier: `catalog.artists[id]` is artist `id`.
//! The move to SQLite (milestone M1) is mechanical — each `Vec` becomes a
//! table, described in `schema.sql`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::audit;

use crate::tags::{AudioProperties, RawTags};
use crate::text;

/// Dense index into the catalog's vectors: `catalog.artists[id]` is artist `id`.
pub type Id = u32;

/// Entity kind, for the polymorphic tables (credits, relations, genres).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// A person or a group.
    Artist,
    /// An album, in the release sense.
    Release,
    /// A single recording, paired with one file.
    Track,
    /// A record label.
    Label,
}

impl EntityKind {
    /// Lowercase spelling used on disk and in the polymorphic tables.
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Artist => "artist",
            EntityKind::Release => "release",
            EntityKind::Track => "track",
            EntityKind::Label => "label",
        }
    }

    /// Named `parse_kind` rather than `from_str`: this is not the standard
    /// trait, and the homonymy would be confusing.
    pub fn parse_kind(s: &str) -> Option<EntityKind> {
        Some(match s {
            "artist" => EntityKind::Artist,
            "release" => EntityKind::Release,
            "track" => EntityKind::Track,
            "label" => EntityKind::Label,
            _ => return None,
        })
    }
}

/// A file on disk, with its technical properties and its raw tags.
///
/// The raw tags are kept: they allow the whole graph to be rebuilt without
/// rereading the files, which is what makes incremental scanning possible.
#[derive(Debug, Clone, Default)]
pub struct AudioFile {
    /// Position in [`Catalog::files`]; this is what a track points back to.
    pub id: Id,
    /// Absolute path, with `/` separators, as the scanner walked it.
    pub path: String,
    /// Size in bytes; together with [`AudioFile::mtime`] it decides whether a
    /// later scan must read the file again or can reuse the stored tags.
    pub size: u64,
    /// Modification date, in seconds since the Unix epoch.
    pub mtime: u64,
    /// Description of the stream itself: codec, sample rate, duration.
    pub properties: AudioProperties,
    /// Whether cover art sits inside the file; when it does not, the release
    /// may still carry an image found beside it.
    pub has_embedded_art: bool,
    /// Tags as read, lowercase name to values, kept so that the whole graph
    /// can be rebuilt without touching the disk again.
    pub tags: BTreeMap<String, Vec<String>>,
    /// What the last integrity check concluded, if one was ever run.
    ///
    /// `None` means "not checked", which is deliberately distinct from
    /// [`audit::integrity::Verdict::NothingToCheck`]: the first can change, the
    /// second cannot. The verdict travels with the file entry, so a scan that
    /// reuses the entry keeps it and a scan that re-reads the file drops it —
    /// a modified file has to be checked again.
    pub integrity: Option<IntegrityRecord>,
}

/// An integrity verdict, with what produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityRecord {
    /// What the check concluded.
    pub verdict: audit::integrity::Verdict,
    /// How it was reached: `flac-frame-crc`, `ogg-page-crc`, or `none`.
    ///
    /// Stored because verdicts of different strengths will coexist: a frame
    /// checksum proves the container intact, an MD5 of the decoded audio will
    /// prove the music itself intact.
    pub method: String,
    /// When the check ran, in seconds since the Unix epoch.
    pub checked_at: u64,
}

impl AudioFile {
    /// Containing directory, the strongest grouping clue after the tags.
    pub fn folder(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }

    /// Last path component, extension included.
    pub fn file_name(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[i + 1..],
            None => &self.path,
        }
    }

    /// First value of a tag, for the many fields where repetition carries no
    /// meaning and only the leading value is wanted.
    pub fn first_tag(&self, key: &str) -> Option<&str> {
        self.tags
            .get(key)
            .and_then(|v| v.first())
            .map(|s| s.as_str())
    }
}

/// Person or group. The person/group distinction will come from MusicBrainz.
#[derive(Debug, Clone, Default)]
pub struct Artist {
    /// Position in [`Catalog::artists`]; credits and relations refer to it.
    pub id: Id,
    /// Name exactly as written in the tags, before normalization.
    pub name: String,
    /// Filing form, with a leading article moved to the end: `Beatles, The`.
    pub sort_name: String,
    /// Normalized matching key (see [`crate::text::normalize`]).
    pub key: String,
    /// MusicBrainz identifier, when one of the files carried it.
    pub mbid: Option<String>,
}

/// An album, in the sense of a "release": what MusicBrainz calls a *release*.
#[derive(Debug, Clone, Default)]
pub struct Release {
    /// Position in [`Catalog::releases`]; tracks point back to it.
    pub id: Id,
    /// Album title as written in the tags.
    pub title: String,
    /// Normalized title, which is what makes two spellings meet.
    pub key: String,
    /// Album artist; absent for multi-artist compilations.
    pub album_artist_id: Option<Id>,
    /// Earliest date announced by the tracks, kept in its tagged form.
    pub date: Option<String>,
    /// Year read out of `date`, for sorting and for filtering by decade.
    pub year: Option<u32>,
    /// Labels that issued this edition, in the order the tags named them.
    pub label_ids: Vec<Id>,
    /// Reference printed on the sleeve, which tells two pressings apart.
    pub catalog_number: Option<String>,
    /// Commercial barcode of the physical edition.
    pub barcode: Option<String>,
    /// Medium the edition was issued on: `CD`, `Vinyl`, `Digital Media`…
    pub media: Option<String>,
    /// MusicBrainz identifier of this precise edition.
    pub mbid: Option<String>,
    /// MusicBrainz identifier of the work every edition of it shares.
    pub release_group_mbid: Option<String>,
    /// Set when the tags declare a compilation or name a placeholder album
    /// artist; the same condition is what leaves `album_artist_id` empty.
    pub is_compilation: bool,
    /// Source folder: the most reliable grouping clue after the tags.
    pub folder: String,
    /// Cover art found next to the files, when none is embedded.
    pub cover_path: Option<String>,
    /// Its tracks, ordered by disc then by position once the build is done.
    pub track_ids: Vec<Id>,
}

/// A track: the pairing of a file with a position within a release.
#[derive(Debug, Clone, Default)]
pub struct Track {
    /// Position in [`Catalog::tracks`].
    pub id: Id,
    /// File this track was read from; there is exactly one per track.
    pub file_id: Id,
    /// Release it belongs to; absent when the file carried no `album` tag.
    pub release_id: Option<Id>,
    /// Title as tagged, or rebuilt from the file name when the tag is missing.
    pub title: String,
    /// Disc within a multi-disc set; a missing value is ordered as the first.
    pub disc_no: Option<u32>,
    /// Position on the disc, taken from the tags or, failing that, inferred
    /// from the digits opening the file name.
    pub track_no: Option<u32>,
    /// Playing time measured on the stream, not read from a tag.
    pub duration_ms: Option<u64>,
    /// Industry code identifying the recording across releases.
    pub isrc: Option<String>,
    /// MusicBrainz identifier of the recording.
    pub mbid: Option<String>,
}

/// A record company, as named by the `label` tag.
#[derive(Debug, Clone, Default)]
pub struct Label {
    /// Position in [`Catalog::labels`].
    pub id: Id,
    /// Name as written in the tags.
    pub name: String,
    /// Normalized key, so that spelling variants land on one label.
    pub key: String,
}

/// A genre, interned once however many entities carry it.
#[derive(Debug, Clone, Default)]
pub struct Genre {
    /// Position in [`Catalog::genres`].
    pub id: Id,
    /// Name as written in the tags.
    pub name: String,
    /// Normalized key, so that case and accent variants land on one genre.
    pub key: String,
}

/// "Who does what, on what". The heart of the graph.
#[derive(Debug, Clone)]
pub struct Credit {
    /// The artist being credited.
    pub artist_id: Id,
    /// Which table `entity_id` indexes.
    pub entity_kind: EntityKind,
    /// The track or release the artist is credited on.
    pub entity_id: Id,
    /// `main`, `featured`, `composer`, `conductor`, `remixer`, `lyricist`…
    ///
    /// [`is_performing_role`] splits these in two: heard on the recording, or
    /// merely behind it.
    pub role: String,
}

/// A typed and dated link between two entities.
///
/// At milestone M0 only what the tags allow us to infer is available:
/// collaboration (two artists credited on the same track). MusicBrainz will
/// then bring `member_of`, `founded`, `signed_to`…
#[derive(Debug, Clone)]
pub struct Relation {
    /// Which table `source_id` indexes.
    pub source_kind: EntityKind,
    /// Entity the link starts from.
    pub source_id: Id,
    /// Which table `target_id` indexes.
    pub target_kind: EntityKind,
    /// Entity the link reaches.
    pub target_id: Id,
    /// Nature of the link; only `collaborated` exists at milestone M0.
    pub kind: String,
    /// Number of observed occurrences: used to rank links by strength.
    pub weight: u32,
    /// Provenance: `tags`, `musicbrainz`, `manual`.
    pub source: String,
}

/// Attachment of a genre to an entity.
#[derive(Debug, Clone)]
pub struct GenreLink {
    /// The genre being attached.
    pub genre_id: Id,
    /// Which table `entity_id` indexes.
    pub entity_kind: EntityKind,
    /// The entity the genre is attached to.
    pub entity_id: Id,
}

/// Roles that mean the artist can be heard on the recording, as opposed to
/// having written or produced it.
///
/// The distinction drives the artist page: singing one guest verse on somebody
/// else's album must not put that album in your discography.
pub const PERFORMING_ROLES: &[&str] = &[
    "main",
    "album",
    "featured",
    "performer",
    "conductor",
    "remixer",
];

/// `true` when the role means the artist is audible on the recording.
pub fn is_performing_role(role: &str) -> bool {
    PERFORMING_ROLES.contains(&role)
}

/// The complete catalog.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Scanned root folders.
    pub roots: Vec<String>,
    /// Date of the last scan (Unix epoch, seconds).
    pub scanned_at: u64,
    /// The files that were read, in the order the scan settled on.
    pub files: Vec<AudioFile>,
    /// Artists, one entry per normalized key.
    pub artists: Vec<Artist>,
    /// Releases, one entry per title, owner and folder.
    pub releases: Vec<Release>,
    /// Tracks, one entry per file that carried readable tags.
    pub tracks: Vec<Track>,
    /// Labels, one entry per normalized key.
    pub labels: Vec<Label>,
    /// Genres, one entry per normalized key.
    pub genres: Vec<Genre>,
    /// Every "who did what, on what" observation, without repeats.
    pub credits: Vec<Credit>,
    /// Links between entities; symmetric ones are stored in both directions.
    pub relations: Vec<Relation>,
    /// Attachments of genres to tracks and to releases.
    pub genre_links: Vec<GenreLink>,
}

impl Catalog {
    /// The artist an id designates, or `None` when the id is out of range.
    pub fn artist(&self, id: Id) -> Option<&Artist> {
        self.artists.get(id as usize)
    }

    /// The release an id designates, or `None` when the id is out of range.
    pub fn release(&self, id: Id) -> Option<&Release> {
        self.releases.get(id as usize)
    }

    /// The track an id designates, or `None` when the id is out of range.
    pub fn track(&self, id: Id) -> Option<&Track> {
        self.tracks.get(id as usize)
    }

    /// The file an id designates, or `None` when the id is out of range.
    pub fn file(&self, id: Id) -> Option<&AudioFile> {
        self.files.get(id as usize)
    }

    /// The label an id designates, or `None` when the id is out of range.
    pub fn label(&self, id: Id) -> Option<&Label> {
        self.labels.get(id as usize)
    }

    /// The genre an id designates, or `None` when the id is out of range.
    pub fn genre(&self, id: Id) -> Option<&Genre> {
        self.genres.get(id as usize)
    }

    /// Total duration of the library.
    pub fn total_duration_ms(&self) -> u64 {
        self.tracks.iter().filter_map(|t| t.duration_ms).sum()
    }

    /// Total size on disk.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// The artist's own discography: releases they are the album artist of.
    ///
    /// This is what a listener means by "their albums". A guest appearance on
    /// somebody else's record belongs in [`Catalog::guest_appearances`].
    pub fn releases_as_album_artist(&self, artist_id: Id) -> Vec<Id> {
        self.releases
            .iter()
            .filter(|r| r.album_artist_id == Some(artist_id))
            .map(|r| r.id)
            .collect()
    }

    /// Releases the artist is audible on without being the album artist:
    /// featured vocals, a guest solo, one track on a compilation.
    pub fn guest_appearances(&self, artist_id: Id) -> Vec<Id> {
        let own: BTreeSet<Id> = self
            .releases_as_album_artist(artist_id)
            .into_iter()
            .collect();
        self.releases_for(artist_id, true)
            .into_iter()
            .filter(|id| !own.contains(id))
            .collect()
    }

    /// Releases where the artist only wrote or produced, without performing.
    pub fn writing_credits_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let heard: BTreeSet<Id> = self
            .releases_as_album_artist(artist_id)
            .into_iter()
            .chain(self.guest_appearances(artist_id))
            .collect();
        self.releases_for(artist_id, false)
            .into_iter()
            .filter(|id| !heard.contains(id))
            .collect()
    }

    /// Releases reached through the artist's credits, keeping either the
    /// performing roles or the writing ones.
    fn releases_for(&self, artist_id: Id, performing: bool) -> Vec<Id> {
        let mut out = BTreeSet::new();
        for credit in self.credits.iter().filter(|c| c.artist_id == artist_id) {
            if is_performing_role(&credit.role) != performing {
                continue;
            }
            let release_id = match credit.entity_kind {
                EntityKind::Release => Some(credit.entity_id),
                EntityKind::Track => self.track(credit.entity_id).and_then(|t| t.release_id),
                _ => None,
            };
            if let Some(id) = release_id {
                out.insert(id);
            }
        }
        out.into_iter().collect()
    }

    /// Tracks the artist is audible on, without duplicates.
    pub fn performed_tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id
                && credit.entity_kind == EntityKind::Track
                && is_performing_role(&credit.role)
            {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Tracks the artist is credited on for writing or production, and is not
    /// audible on.
    ///
    /// The counterpart of [`Catalog::performed_tracks_of_artist`]: together
    /// they cover every track credit, and a track where someone both plays and
    /// composes counts as performed only.
    pub fn written_tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let performed: BTreeSet<Id> = self
            .performed_tracks_of_artist(artist_id)
            .into_iter()
            .collect();
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id
                && credit.entity_kind == EntityKind::Track
                && !is_performing_role(&credit.role)
                && !performed.contains(&credit.entity_id)
            {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Every release the artist appears on, whatever their role.
    pub fn releases_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter().filter(|c| c.artist_id == artist_id) {
            match credit.entity_kind {
                EntityKind::Release => {
                    set.insert(credit.entity_id);
                }
                EntityKind::Track => {
                    if let Some(release_id) =
                        self.track(credit.entity_id).and_then(|t| t.release_id)
                    {
                        set.insert(release_id);
                    }
                }
                _ => {}
            }
        }
        set.into_iter().collect()
    }

    /// Tracks credited to an artist, without duplicates.
    ///
    /// A single artist may hold several roles on one track (performer **and**
    /// composer): without deduplication, every count would be inflated.
    pub fn tracks_of_artist(&self, artist_id: Id) -> Vec<Id> {
        let mut set = BTreeSet::new();
        for credit in self.credits.iter() {
            if credit.artist_id == artist_id && credit.entity_kind == EntityKind::Track {
                set.insert(credit.entity_id);
            }
        }
        set.into_iter().collect()
    }

    /// Artists credited on an entity, along with their role.
    pub fn credits_on(&self, kind: EntityKind, id: Id) -> Vec<(&Artist, &str)> {
        self.credits
            .iter()
            .filter(|c| c.entity_kind == kind && c.entity_id == id)
            .filter_map(|c| self.artist(c.artist_id).map(|a| (a, c.role.as_str())))
            .collect()
    }

    /// An artist's neighbours in the graph, from strongest link to weakest.
    pub fn neighbours_of_artist(&self, artist_id: Id) -> Vec<(&Artist, u32, &str)> {
        let mut out: Vec<(&Artist, u32, &str)> = self
            .relations
            .iter()
            .filter(|r| r.source_kind == EntityKind::Artist && r.source_id == artist_id)
            .filter_map(|r| {
                self.artist(r.target_id)
                    .map(|a| (a, r.weight, r.kind.as_str()))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        out
    }

    /// Tracks on which two artists are both credited in a performing role.
    ///
    /// This is what the weight of a `collaborated` relation counts, recomputed
    /// on demand rather than stored: the `credit` table already holds the
    /// answer, and a second copy is a second thing that can fall out of step.
    pub fn tracks_in_common(&self, a: Id, b: Id) -> Vec<Id> {
        let performing_on = |artist: Id| -> BTreeSet<Id> {
            self.credits
                .iter()
                .filter(|c| {
                    c.entity_kind == EntityKind::Track
                        && c.artist_id == artist
                        && is_performing_role(&c.role)
                })
                .map(|c| c.entity_id)
                .collect()
        };
        let (left, right) = (performing_on(a), performing_on(b));
        left.intersection(&right).copied().collect()
    }

    /// Releases tied to this one by a relation of the given kind.
    ///
    /// Used with [`DUPLICATE`] and [`OTHER_EDITION`] to answer "is this album
    /// here twice, and on purpose?".
    pub fn related_releases(&self, release_id: Id, kind: &str) -> Vec<Id> {
        self.relations
            .iter()
            .filter(|r| {
                r.source_kind == EntityKind::Release && r.source_id == release_id && r.kind == kind
            })
            .map(|r| r.target_id)
            .collect()
    }

    /// Genres attached to an entity.
    pub fn genres_of(&self, kind: EntityKind, id: Id) -> Vec<&Genre> {
        self.genre_links
            .iter()
            .filter(|g| g.entity_kind == kind && g.entity_id == id)
            .filter_map(|g| self.genre(g.genre_id))
            .collect()
    }

    /// Case- and accent-insensitive search, across every entity.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let needle = text::normalize(query);
        if needle.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<SearchHit> = Vec::new();

        let mut push = |kind: EntityKind, id: Id, name: &str, key: &str, detail: String| {
            let score = match () {
                _ if key == needle => 0u8,
                _ if key.starts_with(&needle) => 1,
                _ if key.contains(&needle) => 2,
                _ => return,
            };
            hits.push(SearchHit {
                kind,
                id,
                name: name.to_string(),
                detail,
                score,
            });
        };

        for a in &self.artists {
            push(EntityKind::Artist, a.id, &a.name, &a.key, String::new());
        }
        for r in &self.releases {
            let detail = r
                .album_artist_id
                .and_then(|id| self.artist(id))
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Various Artists".to_string());
            push(EntityKind::Release, r.id, &r.title, &r.key, detail);
        }
        for t in &self.tracks {
            let key = text::normalize(&t.title);
            let detail = t
                .release_id
                .and_then(|id| self.release(id))
                .map(|r| r.title.clone())
                .unwrap_or_default();
            push(EntityKind::Track, t.id, &t.title, &key, detail);
        }
        for l in &self.labels {
            push(EntityKind::Label, l.id, &l.name, &l.key, String::new());
        }

        hits.sort_by(|a, b| {
            a.score
                .cmp(&b.score)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.name.len().cmp(&b.name.len()))
                .then_with(|| a.name.cmp(&b.name))
        });
        hits.truncate(limit);
        hits
    }

    /// Finds an artist by name, up to normalization.
    pub fn find_artist(&self, name: &str) -> Option<&Artist> {
        let key = text::normalize(name);
        self.artists.iter().find(|a| a.key == key)
    }

    /// Finds a release by title, up to normalization.
    ///
    /// The first of [`Catalog::find_releases`], which means an exact title
    /// wins over an album that merely begins with it: `Danzig` is the 1988
    /// record, not whichever of `Danzig 4` or `Danzig II` the catalog happens
    /// to hold first.
    pub fn find_release(&self, title: &str) -> Option<&Release> {
        self.find_releases(title).0.into_iter().next()
    }

    /// Every release whose title matches, exactly or failing that partially.
    ///
    /// Same rule as [`Catalog::find_tracks`], and for the same reason: a
    /// command must not pick one answer out of several without saying so.
    /// Unlike tracks, though, two matching albums are two *different* albums —
    /// a shared prefix is not an ambiguity — which is why the exact match is
    /// what usually ends the search.
    pub fn find_releases(&self, title: &str) -> (Vec<&Release>, TitleMatch) {
        let key = text::normalize(title);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Release> = self.releases.iter().filter(|r| r.key == key).collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        let partial = self
            .releases
            .iter()
            .filter(|r| r.key.contains(&key))
            .collect();
        (partial, TitleMatch::Partial)
    }

    /// Every track carrying this title, up to normalization.
    ///
    /// A title is not an identifier: the same one legitimately comes back on
    /// the album, on a single and on a live record, and those are different
    /// recordings. All of them are returned, in catalog order.
    ///
    /// Exact matches win. Only when there is none does the search widen to the
    /// titles containing the text, so that a half-remembered title still leads
    /// somewhere; [`TitleMatch`] says which of the two happened.
    pub fn find_tracks(&self, title: &str) -> (Vec<&Track>, TitleMatch) {
        let key = text::normalize(title);
        if key.is_empty() {
            return (Vec::new(), TitleMatch::Exact);
        }
        let exact: Vec<&Track> = self
            .tracks
            .iter()
            .filter(|t| text::normalize(&t.title) == key)
            .collect();
        if !exact.is_empty() {
            return (exact, TitleMatch::Exact);
        }
        let partial = self
            .tracks
            .iter()
            .filter(|t| text::normalize(&t.title).contains(&key))
            .collect();
        (partial, TitleMatch::Partial)
    }
}

/// How [`Catalog::find_tracks`] reached its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleMatch {
    /// The titles are the given one, up to normalization.
    Exact,
    /// No title matched, so the ones containing the text were returned.
    Partial,
}

/// One answer returned by [`Catalog::search`], ready to be shown and followed.
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Table the hit lives in, which tells the caller where to navigate.
    pub kind: EntityKind,
    /// Identifier within that table.
    pub id: Id,
    /// Display form, unnormalized, as it should appear on screen.
    pub name: String,
    /// Context shown next to the name (album artist, track's album…).
    pub detail: String,
    score: u8,
}

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
            integrity: item.integrity.clone(),
        };
        let folder = file.folder().to_string();
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

/// Version of the rules that derive the `relation` table.
///
/// The relations are **inferred**, not read: they follow from the credits and
/// from the track lists. Changing how they are inferred makes a stored catalog
/// out of date without making it invalid, which is why this is not
/// `store::FORMAT_VERSION` — refusing to load would be out of proportion, and
/// would throw away integrity verdicts that cost hours to obtain. Bump this
/// instead, and every catalog rebuilds its relations on the next load.
pub const RELATION_RULES: u32 = 1;

/// Recomputes every inferred relation from the entities already in place.
///
/// Needs no disk access: the credits and the tracks hold everything. That is
/// exactly what keeping the raw tags per file was for.
pub fn rebuild_relations(catalog: &mut Catalog) {
    catalog.relations.clear();
    build_collaboration_graph(catalog);
    build_release_relations(catalog);
}

/// Names of the two relations that tie one album to another copy of itself.
pub const DUPLICATE: &str = "duplicate";
/// Same album, encoded differently: a deliberate second copy.
pub const OTHER_EDITION: &str = "other_edition";

/// Links the releases that are the same album twice.
///
/// The same album legitimately appears twice in a library — a hi-res copy
/// beside the CD rip, a FLAC beside the MP3 for the car — and illegitimately
/// too, when a folder was copied and forgotten. The model keeps them as two
/// releases either way, because they *are* two sets of files in two folders and
/// that is what one needs to act on. What was missing is the link between them,
/// and the reason for it.
///
/// The two are told apart by their audio, not by their folder: same album
/// artist, same title, same track list, and then
///
/// - the same quality on both sides — nothing distinguishes the copies, and one
///   of them is wasted space;
/// - a different quality — the second copy is there on purpose.
///
/// Anything else keeps the weaker `other_edition` link: a deluxe edition with
/// three bonus tracks is not a duplicate, but it is not unrelated either.
fn build_release_relations(catalog: &mut Catalog) {
    let mut groups: BTreeMap<(Option<Id>, String), Vec<Id>> = BTreeMap::new();
    for release in &catalog.releases {
        groups
            .entry((release.album_artist_id, release.key.clone()))
            .or_default()
            .push(release.id);
    }

    let mut links: Vec<(Id, Id, &'static str)> = Vec::new();
    for ids in groups.values().filter(|ids| ids.len() > 1) {
        for (i, &left) in ids.iter().enumerate() {
            for &right in &ids[i + 1..] {
                // Two albums sharing a name are not necessarily the same
                // album: without a matching track list there is nothing
                // reliable to say, and MusicBrainz will settle it at M1.
                if !same_track_list(catalog, left, right) {
                    continue;
                }
                let kind =
                    if quality_fingerprint(catalog, left) == quality_fingerprint(catalog, right) {
                        DUPLICATE
                    } else {
                        OTHER_EDITION
                    };
                links.push((left, right, kind));
            }
        }
    }

    for (left, right, kind) in links {
        // Symmetric, like the collaboration graph: navigation works from
        // either side.
        for (source, target) in [(left, right), (right, left)] {
            catalog.relations.push(Relation {
                source_kind: EntityKind::Release,
                source_id: source,
                target_kind: EntityKind::Release,
                target_id: target,
                kind: kind.to_string(),
                weight: 1,
                source: "tags".into(),
            });
        }
    }
}

/// `true` when two releases hold the same tracks.
///
/// Positions and titles have to match exactly; durations only have to be
/// **close**. Two rips of one disc differ by a few hundred milliseconds, and a
/// transcode to a lossy format shifts the end of a track further still — but a
/// live rendition of the same song differs by minutes. Three seconds is the
/// tolerance the duplicate-track check uses, for the same reason.
fn same_track_list(catalog: &Catalog, left: Id, right: Id) -> bool {
    let (left, right) = (track_list(catalog, left), track_list(catalog, right));
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1 && a.2.abs_diff(b.2) <= 3_000)
}

/// Positions, titles and durations of a release, in a comparable order.
fn track_list(catalog: &Catalog, release_id: Id) -> Vec<(u32, String, u64)> {
    let Some(release) = catalog.release(release_id) else {
        return Vec::new();
    };
    let mut out: Vec<(u32, String, u64)> = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .map(|t| {
            (
                t.track_no.unwrap_or(0),
                text::normalize(&t.title),
                t.duration_ms.unwrap_or(0),
            )
        })
        .collect();
    out.sort();
    out
}

/// How the release is encoded, which is what separates a wasted copy from a
/// second one kept on purpose.
fn quality_fingerprint(catalog: &Catalog, release_id: Id) -> BTreeSet<String> {
    let Some(release) = catalog.release(release_id) else {
        return BTreeSet::new();
    };
    release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| catalog.file(t.file_id))
        .map(|f| f.properties.quality_label())
        .collect()
}

/// Two artists credited on the same track are considered to have
/// collaborated. The weight counts the shared tracks: that is what allows
/// ranking by "played the most with".
fn build_collaboration_graph(catalog: &mut Catalog) {
    let mut per_track: HashMap<Id, BTreeSet<Id>> = HashMap::new();
    for credit in &catalog.credits {
        // Only performers: sharing a composer does not mean two artists ever
        // met, let alone played together.
        if credit.entity_kind == EntityKind::Track && is_performing_role(&credit.role) {
            per_track
                .entry(credit.entity_id)
                .or_default()
                .insert(credit.artist_id);
        }
    }

    let mut weights: BTreeMap<(Id, Id), u32> = BTreeMap::new();
    for artists in per_track.values() {
        let list: Vec<Id> = artists.iter().copied().collect();
        for (i, &a) in list.iter().enumerate() {
            for &b in &list[i + 1..] {
                *weights.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    for ((a, b), weight) in weights {
        // The relation is symmetric: it is stored in both directions so that
        // navigation is direct from either side.
        catalog.relations.push(Relation {
            source_kind: EntityKind::Artist,
            source_id: a,
            target_kind: EntityKind::Artist,
            target_id: b,
            kind: "collaborated".into(),
            weight,
            source: "tags".into(),
        });
        catalog.relations.push(Relation {
            source_kind: EntityKind::Artist,
            source_id: b,
            target_kind: EntityKind::Artist,
            target_id: a,
            kind: "collaborated".into(),
            weight,
            source: "tags".into(),
        });
    }
}

/// Without a `title` tag, fall back to the file name, stripped of its leading
/// track number and its extension.
fn title_from_filename(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
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
    let name = path.rsplit('/').next().unwrap_or(path);
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    digits.parse().ok().filter(|&n| n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::RawTags;

    fn track(path: &str, fields: &[(&str, &str)], duration_ms: u64) -> ScannedFile {
        let mut tags = RawTags::default();
        for (k, v) in fields {
            tags.insert(k, *v);
        }
        tags.properties.duration_ms = Some(duration_ms);
        tags.properties.codec = "flac".into();
        tags.properties.lossless = true;
        ScannedFile {
            path: path.to_string(),
            size: 1_000_000,
            mtime: 0,
            tags,
            folder_cover: None,
            integrity: None,
        }
    }

    fn example_catalog() -> Catalog {
        build(
            vec![
                track(
                    "/m/Metallica/Ride the Lightning/01 Fight Fire with Fire.flac",
                    &[
                        ("title", "Fight Fire with Fire"),
                        ("artist", "Metallica"),
                        ("album", "Ride the Lightning"),
                        ("albumartist", "Metallica"),
                        ("date", "1984"),
                        ("genre", "Thrash Metal"),
                        ("tracknumber", "1/8"),
                        ("label", "Megaforce"),
                    ],
                    100_000,
                ),
                track(
                    "/m/Metallica/Ride the Lightning/02 Ride the Lightning.flac",
                    &[
                        ("title", "Ride the Lightning"),
                        ("artist", "Metallica"),
                        ("album", "Ride the Lightning"),
                        ("albumartist", "Metallica"),
                        ("date", "1984"),
                        ("tracknumber", "2/8"),
                    ],
                    120_000,
                ),
                track(
                    "/m/Various/Duos/01 Sous le vent.flac",
                    &[
                        ("title", "Sous le vent"),
                        ("artist", "Garou feat. Céline Dion"),
                        ("album", "Duos"),
                        ("compilation", "1"),
                        ("date", "2001"),
                    ],
                    90_000,
                ),
            ],
            vec!["/m".to_string()],
            0,
        )
    }

    #[test]
    fn the_shared_tracks_match_the_collaboration_weight() {
        // The weight of a `collaborated` relation is a count; the tracks it
        // counts must be reachable, or the graph cannot be walked.
        let c = example_catalog();
        let garou = c.find_artist("Garou").expect("Garou present");
        let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
        let shared = c.tracks_in_common(garou.id, celine.id);
        let (_, weight, _) = c.neighbours_of_artist(garou.id)[0];
        assert_eq!(shared.len() as u32, weight, "count and list must agree");
        assert_eq!(
            c.track(shared[0]).map(|t| t.title.as_str()),
            Some("Sous le vent")
        );
        // A composer credit is not a collaboration, and neither is being alone.
        assert!(c.tracks_in_common(garou.id, garou.id).len() == 1);
    }

    #[test]
    fn an_exact_album_title_wins_over_the_ones_beginning_with_it() {
        // "Danzig" used to match "Danzig 4" and return whichever came first in
        // the catalog: an arbitrary answer, given without saying so.
        let album = |folder: &str, title: &'static str| {
            track(
                &format!("{folder}/01.flac"),
                &[
                    ("title", "A song"),
                    ("artist", "Danzig"),
                    ("albumartist", "Danzig"),
                    ("album", title),
                ],
                120_000,
            )
        };
        let c = build(
            vec![
                album("/m/4", "Danzig 4"),
                album("/m/1", "Danzig"),
                album("/m/2", "Danzig II: Lucifuge"),
            ],
            vec!["/m".into()],
            0,
        );

        let (exact, kind) = c.find_releases("Danzig");
        assert_eq!(kind, TitleMatch::Exact);
        assert_eq!(exact.len(), 1, "one album is titled exactly that");
        assert_eq!(exact[0].title, "Danzig");
        assert_eq!(
            c.find_release("Danzig").map(|r| r.title.as_str()),
            Some("Danzig")
        );

        // With no exact title, every match is returned rather than one of them.
        let (partial, kind) = c.find_releases("danzig i");
        assert_eq!(kind, TitleMatch::Partial);
        assert_eq!(partial.len(), 1);
        // Normalization trims the query, so a trailing space is not a way to
        // ask for "the longer titles"; a fragment is.
        let (several, kind) = c.find_releases("anzig");
        assert_eq!(kind, TitleMatch::Partial);
        assert_eq!(several.len(), 3, "every title containing it");
    }

    #[test]
    fn the_same_album_twice_is_linked_and_qualified() {
        // Same album in two folders: a copy when the encoding matches, another
        // edition when it does not. The model keeps both — they are two sets of
        // files — and says which case it is.
        let make = |folder: &str, codec: &str, rate: u32| {
            let mut f = track(
                &format!("{folder}/01.flac"),
                &[
                    ("title", "Brand New God"),
                    ("artist", "Danzig"),
                    ("albumartist", "Danzig"),
                    ("album", "Danzig 4"),
                    ("tracknumber", "1"),
                ],
                120_000,
            );
            f.tags.properties.codec = codec.to_string();
            f.tags.properties.sample_rate = Some(rate);
            f.tags.properties.bit_depth = Some(if rate > 48_000 { 24 } else { 16 });
            f
        };
        let c = build(
            vec![
                make("/m/A", "flac", 44_100),
                make("/m/B", "flac", 44_100),
                make("/m/C", "flac", 96_000),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(c.releases.len(), 3, "three folders, three releases");
        let a = c.releases[0].id;
        let copies = c.related_releases(a, DUPLICATE);
        assert_eq!(copies.len(), 1, "one identical copy");
        let others = c.related_releases(a, OTHER_EDITION);
        assert_eq!(others.len(), 1, "one differently encoded copy");
        assert_ne!(copies[0], others[0]);
        // Symmetric: the link is navigable from either side.
        assert!(c.related_releases(copies[0], DUPLICATE).contains(&a));
    }

    #[test]
    fn every_track_sharing_a_title_is_returned() {
        // The studio version and the live one are two different recordings
        // that happen to share a name; neither may hide the other.
        let c = build(
            vec![
                track(
                    "/m/a/01 Ride the Lightning.flac",
                    &[
                        ("title", "Ride the Lightning"),
                        ("artist", "Metallica"),
                        ("album", "Ride the Lightning"),
                    ],
                    120_000,
                ),
                track(
                    "/m/b/03 Ride the Lightning.flac",
                    &[
                        ("title", "Ride the Lightning"),
                        ("artist", "Metallica"),
                        ("album", "Live Shit"),
                    ],
                    400_000,
                ),
            ],
            vec!["/m".to_string()],
            0,
        );
        let (found, kind) = c.find_tracks("ride the lightning");
        assert_eq!(kind, TitleMatch::Exact, "the title matches as written");
        assert_eq!(found.len(), 2, "both recordings are returned");
        let durations: Vec<Option<u64>> = found.iter().map(|t| t.duration_ms).collect();
        assert!(durations.contains(&Some(400_000)), "the live one is there");
    }

    #[test]
    fn a_partial_title_widens_the_search_only_as_a_last_resort() {
        let c = example_catalog();
        let (exact, kind) = c.find_tracks("Fight Fire with Fire");
        assert_eq!(kind, TitleMatch::Exact);
        assert_eq!(exact.len(), 1);

        let (partial, kind) = c.find_tracks("fight fire");
        assert_eq!(kind, TitleMatch::Partial, "no title is exactly that");
        assert_eq!(partial.len(), 1);

        let (nothing, _) = c.find_tracks("nothing of the sort");
        assert!(nothing.is_empty());
    }

    #[test]
    fn entities_are_deduplicated() {
        let c = example_catalog();
        // Metallica appears only once despite two tracks.
        assert_eq!(
            c.artists.iter().filter(|a| a.name == "Metallica").count(),
            1
        );
        assert_eq!(c.releases.len(), 2);
        assert_eq!(c.tracks.len(), 3);
    }

    #[test]
    fn tracks_are_ordered_within_the_release() {
        let c = example_catalog();
        let album = c.find_release("Ride the Lightning").expect("album found");
        let titles: Vec<&str> = album
            .track_ids
            .iter()
            .map(|&id| c.track(id).unwrap().title.as_str())
            .collect();
        assert_eq!(titles, ["Fight Fire with Fire", "Ride the Lightning"]);
        assert_eq!(album.year, Some(1984));
    }

    #[test]
    fn featuring_creates_two_artists_and_one_link() {
        let c = example_catalog();
        let garou = c.find_artist("Garou").expect("Garou present");
        let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
        let neighbours = c.neighbours_of_artist(garou.id);
        assert_eq!(neighbours.len(), 1);
        assert_eq!(neighbours[0].0.id, celine.id);
        assert_eq!(neighbours[0].1, 1, "one track in common");
        // The link is indeed symmetric.
        assert_eq!(c.neighbours_of_artist(celine.id)[0].0.id, garou.id);
    }

    #[test]
    fn various_artists_is_not_an_artist() {
        let c = build(
            vec![track(
                "/m/Various/Hits/01 Song.flac",
                &[
                    ("title", "Song"),
                    ("artist", "Performer"),
                    ("album", "Hits"),
                    ("albumartist", "Various Artists"),
                ],
                60_000,
            )],
            vec!["/m".into()],
            0,
        );
        assert!(c.find_artist("Various Artists").is_none());
        assert_eq!(c.artists.len(), 1, "only the performer must exist");
        let hits = c.find_release("Hits").unwrap();
        assert!(hits.is_compilation);
        assert_eq!(hits.album_artist_id, None);
    }

    #[test]
    fn compilation_has_no_album_artist() {
        let c = example_catalog();
        let duos = c.find_release("Duos").expect("compilation found");
        assert!(duos.is_compilation);
        assert_eq!(duos.album_artist_id, None);
    }

    #[test]
    fn interning_reuses_entities_and_keeps_ids_contiguous() {
        let mut b = Builder::new(vec!["/m".into()], 0);
        let first = b.intern_artist("The Beatles");
        assert_eq!(
            first,
            b.intern_artist("Beatles, The"),
            "normalization matches them"
        );
        assert_eq!(b.intern_artist("Björk"), 1, "a new name takes the next id");
        assert_eq!(b.intern_label("Columbia"), 0);
        assert_eq!(b.intern_genre("Jazz"), 0);
        let catalog = b.finish();
        assert_eq!(catalog.artists.len(), 2);
        for (index, artist) in catalog.artists.iter().enumerate() {
            assert_eq!(artist.id as usize, index, "ids index the vector");
        }
    }

    #[test]
    fn the_same_credit_is_never_recorded_twice() {
        let mut b = Builder::new(vec![], 0);
        let artist = b.intern_artist("Miles Davis");
        b.push_credit(artist, EntityKind::Track, 0, "main");
        b.push_credit(artist, EntityKind::Track, 0, "main");
        b.push_credit(artist, EntityKind::Track, 0, "composer");
        let catalog = b.finish();
        assert_eq!(
            catalog.credits.len(),
            2,
            "same role once, different role kept"
        );
    }

    #[test]
    fn placeholder_album_artists_are_recognised() {
        for name in ["Various Artists", "various", "VA", "Artistes divers"] {
            assert!(is_various_artists(name), "{name} should be a placeholder");
        }
        for name in ["Various Cruelties", "Miles Davis"] {
            assert!(!is_various_artists(name), "{name} is a real artist");
        }
    }

    #[test]
    fn two_editions_in_two_folders_stay_two_releases() {
        // Same title, same artist, different folder: two pressings of one
        // record must not be merged into a single release.
        let fields = [
            ("title", "So What"),
            ("artist", "Miles Davis"),
            ("albumartist", "Miles Davis"),
            ("album", "Kind of Blue"),
        ];
        let c = build(
            vec![
                track("/m/Miles Davis/Kind of Blue/01.flac", &fields, 1000),
                track(
                    "/m/Miles Davis/Kind of Blue (2011 remaster)/01.flac",
                    &fields,
                    1000,
                ),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(c.releases.len(), 2, "the folder tells the editions apart");
        assert_eq!(c.artists.len(), 1, "but the artist is shared");
    }

    #[test]
    fn a_guest_appearance_is_not_part_of_the_discography() {
        // Exactly the shape that made "The Sinister Urge" show up under Ozzy
        // Osbourne: he sings one track on a Rob Zombie album.
        let c = build(
            vec![
                track(
                    "/m/Rob Zombie/The Sinister Urge/04 Never Gonna Stop.flac",
                    &[
                        ("title", "Never Gonna Stop"),
                        ("artist", "Rob Zombie"),
                        ("albumartist", "Rob Zombie"),
                        ("album", "The Sinister Urge"),
                        ("date", "2001"),
                    ],
                    60_000,
                ),
                track(
                    "/m/Rob Zombie/The Sinister Urge/05 Iron Head.flac",
                    &[
                        ("title", "Iron Head"),
                        ("artist", "Rob Zombie"),
                        ("performer", "Ozzy Osbourne"),
                        ("albumartist", "Rob Zombie"),
                        ("album", "The Sinister Urge"),
                        ("date", "2001"),
                    ],
                    60_000,
                ),
                track(
                    "/m/Ozzy Osbourne/Blizzard of Ozz/01 I Dont Know.flac",
                    &[
                        ("title", "I Don't Know"),
                        ("artist", "Ozzy Osbourne"),
                        ("albumartist", "Ozzy Osbourne"),
                        ("album", "Blizzard of Ozz"),
                        ("date", "1980"),
                    ],
                    60_000,
                ),
            ],
            vec!["/m".into()],
            0,
        );

        let ozzy = c
            .find_artist("Ozzy Osbourne")
            .expect("Ozzy is in the catalog");
        let own: Vec<&str> = c
            .releases_as_album_artist(ozzy.id)
            .iter()
            .filter_map(|&id| c.release(id))
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(
            own,
            ["Blizzard of Ozz"],
            "the discography holds his own albums only"
        );

        let guest: Vec<&str> = c
            .guest_appearances(ozzy.id)
            .iter()
            .filter_map(|&id| c.release(id))
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(
            guest,
            ["The Sinister Urge"],
            "the guest album is listed apart"
        );

        // The old, undifferentiated view still returns both.
        assert_eq!(c.releases_of_artist(ozzy.id).len(), 2);
    }

    #[test]
    fn a_writing_credit_is_neither_discography_nor_appearance() {
        let c = build(
            vec![track(
                "/m/Ozzy Osbourne/Blizzard of Ozz/01 Crazy Train.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("albumartist", "Ozzy Osbourne"),
                    ("album", "Blizzard of Ozz"),
                    ("composer", "Randy Rhoads"),
                    ("date", "1980"),
                ],
                60_000,
            )],
            vec!["/m".into()],
            0,
        );

        let rhoads = c
            .find_artist("Randy Rhoads")
            .expect("the composer is an entity");
        assert!(c.releases_as_album_artist(rhoads.id).is_empty());
        assert!(c.guest_appearances(rhoads.id).is_empty());
        assert_eq!(c.writing_credits_of_artist(rhoads.id).len(), 1);
        assert!(c.performed_tracks_of_artist(rhoads.id).is_empty());
    }

    #[test]
    fn navigation_from_artist_to_albums() {
        let c = example_catalog();
        let metallica = c.find_artist("Metallica").unwrap();
        let albums = c.releases_of_artist(metallica.id);
        assert_eq!(albums.len(), 1);
        assert_eq!(c.release(albums[0]).unwrap().title, "Ride the Lightning");
    }

    #[test]
    fn search_is_accent_insensitive() {
        let c = example_catalog();
        let hits = c.search("celine", 10);
        assert!(
            hits.iter().any(|h| h.name == "Céline Dion"),
            "got: {hits:?}"
        );
    }

    #[test]
    fn genres_attached_to_track_and_album() {
        let c = example_catalog();
        let album = c.find_release("Ride the Lightning").unwrap();
        let genres = c.genres_of(EntityKind::Release, album.id);
        assert_eq!(genres.len(), 1);
        assert_eq!(genres[0].name, "Thrash Metal");
    }

    #[test]
    fn title_inferred_from_filename() {
        assert_eq!(title_from_filename("/m/01 - So What.flac"), "So What");
        assert_eq!(title_from_filename("/m/So What.flac"), "So What");
        assert_eq!(track_from_filename("/m/07 - Blue in Green.flac"), Some(7));
        assert_eq!(track_from_filename("/m/Blue in Green.flac"), None);
    }

    #[test]
    fn deterministic_build() {
        let a = example_catalog();
        let b = example_catalog();
        let names_a: Vec<&str> = a.artists.iter().map(|x| x.name.as_str()).collect();
        let names_b: Vec<&str> = b.artists.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names_a, names_b, "identifiers must be stable");
    }
}
