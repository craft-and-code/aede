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
//!
//! The module is divided by what the code *does* with the graph, which is the
//! only division that holds up. This file is the vocabulary — the entities and
//! the tables they live in — and beside it sit the three verbs:
//!
//! | Module | What it does |
//! |---|---|
//! | [`query`] | Reads the graph: every question a command asks of it |
//! | [`builder`] | Turns scanned files into entities, deterministically |
//! | [`relations`] | Derives the typed links between entities |
//!
//! The division is not cosmetic. [`query`] takes `&self` throughout, so a
//! lookup that tried to change something would not compile; [`builder`] is the
//! only place identifiers are handed out; [`relations`] holds everything that
//! is *inferred* rather than read, which is why it carries a version of its own.
//!
//! What the rest of the program calls is re-exported here, so `model::build`,
//! `model::ScannedFile` and `model::rebuild_relations` keep naming the same
//! things they always did — the split is internal, and no caller had to change.

use std::collections::BTreeMap;

use crate::audit;
use crate::fingerprint;
use crate::tags::AudioProperties;
use crate::text;

pub mod builder;
pub mod query;
pub mod relations;

pub use builder::{ScannedFile, build};
pub use query::{SearchHit, TitleMatch};
pub use relations::{DUPLICATE, OTHER_EDITION, RELATION_RULES, rebuild_relations};

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
    /// A genre. Never carried by a credit or a relation — a genre performs
    /// nothing — but an entity all the same, and one a user can have an
    /// opinion about.
    Genre,
}

impl EntityKind {
    /// Lowercase spelling used on disk and in the polymorphic tables.
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Artist => "artist",
            EntityKind::Release => "release",
            EntityKind::Track => "track",
            EntityKind::Label => "label",
            EntityKind::Genre => "genre",
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
            "genre" => EntityKind::Genre,
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
    /// A `.lrc` sitting beside the file, when there is one.
    ///
    /// The **path**, not the text. Lyrics carried by a tag are already in the
    /// catalog — raw tags are kept per file — but a sidecar is not in the file,
    /// and storing its contents as though it were a tag would make the catalog
    /// lie about what the file says. It is one small text file next to the
    /// music; reading it when somebody asks costs nothing and is always
    /// current.
    pub lyrics_path: Option<String>,
    /// What the last integrity check concluded, if one was ever run.
    ///
    /// `None` means "not checked", which is deliberately distinct from
    /// [`audit::integrity::Verdict::NothingToCheck`]: the first can change, the
    /// second cannot. The verdict travels with the file entry, so a scan that
    /// reuses the entry keeps it and a scan that re-reads the file drops it —
    /// a modified file has to be checked again.
    pub integrity: Option<IntegrityRecord>,
    /// The acoustic fingerprint of this file, once one has been computed.
    ///
    /// Beside the integrity verdict and for the same reason: both are Aède's
    /// own conclusions about the **bytes**, worked out rather than fetched,
    /// and both are expensive enough that recomputing them on every scan would
    /// be unthinkable. So both travel with the file entry — a scan that reuses
    /// the entry keeps them, a scan that re-reads the file drops them, because
    /// a modified file is different audio and its old fingerprint describes
    /// something that is no longer there.
    ///
    /// What AcoustID *says* when shown one is a different kind of thing —
    /// somebody else's claim — and lives in `sources.json` with every other
    /// source's.
    pub fingerprint: Option<fingerprint::Fingerprint>,
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
        text::folder(&self.path)
    }

    /// Last path component, extension included.
    pub fn file_name(&self) -> &str {
        text::file_name(&self.path)
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
/// Until M1 only what the tags allow us to infer is available: collaboration
/// (two artists credited on the same track). MusicBrainz will then bring
/// `member_of`, `founded`, `signed_to`… — the last of which needs a period on
/// the link, since a line-up is a fact with dates.
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
    /// Nature of the link; only `collaborated` exists until M1.
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
    /// Folders never read, however deep under a root they sit.
    ///
    /// Kept beside the roots rather than passed to each scan, because a plain
    /// `aede scan` re-reads every root: an exclusion that had to be retyped
    /// would be forgotten precisely when it mattered. Canonical, like the
    /// roots, so the comparison against a walked path is the same string
    /// comparison everywhere else in the program makes.
    pub excluded: Vec<String>,
    /// Date of the last scan (Unix epoch, seconds).
    pub scanned_at: u64,
    /// The files that were read, in the order the scan settled on.
    pub files: Vec<AudioFile>,
    /// Deep analyses imported from another tool, one row per path and source.
    ///
    /// Optional by nature: a catalog without a single one works exactly the
    /// same. They are kept apart from the files rather than merged into them,
    /// so that a measurement always answers "who says so".
    ///
    /// Keyed by path, which means some of them may describe files the catalog
    /// does not hold — a library analysed before it was ever scanned. Those
    /// wait here until a scan brings the file in.
    pub analyses: Vec<crate::analysis::FileAnalysis>,
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

/// Fixtures shared by the tests of every sibling module.
///
/// One library described once: a catalog built by hand in four files would
/// drift, and a test asserting on a different library than the one it names is
/// worse than no test.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// First release whose title matches, for tests that expect exactly one.
    ///
    /// Deliberately not on `Catalog`: production code goes through
    /// [`Catalog::find_releases`], which never hides the fact that several
    /// albums matched. A convenience that picks one silently belongs to the
    /// tests that already know there is only one.
    pub(crate) fn first_release<'a>(catalog: &'a Catalog, title: &str) -> Option<&'a Release> {
        catalog.find_releases(title).0.into_iter().next()
    }

    use crate::tags::RawTags;

    pub(crate) fn track(path: &str, fields: &[(&str, &str)], duration_ms: u64) -> ScannedFile {
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
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    }

    pub(crate) fn example_catalog() -> Catalog {
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
}
