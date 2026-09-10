//! What other sources say about the library.
//!
//! The catalog holds what the files say. [`crate::user`] holds what the user
//! says. This holds what somebody else says — MusicBrainz first, a plugin
//! later — and it is a third store rather than a column in either of the other
//! two, for reasons written out in `docs/design/attribution.md`.
//!
//! The rule the whole module exists to enforce: **a fetched value sits beside
//! the tag, never on top of it.** Writing a fetched genre into the catalog's
//! `genre` would cost three things at once — saying where the value came from,
//! noticing that the source and the tag differ, and undoing it — and it would
//! not even survive, because a scan rebuilds the catalog from the files.
//!
//! Nothing here touches the network. This is the receptacle; filling it from
//! MusicBrainz is a later step, and the layer is testable without a single
//! request.

use crate::json::Json;
use crate::model::EntityKind;
use crate::user::EntityRef;
use std::path::Path;

/// Version of the `sources.json` document this build writes and accepts.
pub const SOURCES_FORMAT_VERSION: u32 = 1;

/// Name of the file, inside the data folder that holds the catalog.
pub const SOURCES_FILE: &str = "sources.json";

/// The source name MusicBrainz records carry.
pub const MUSICBRAINZ: &str = "musicbrainz";

// --------------------------------------------------------------------------
// How firmly a record is attached
// --------------------------------------------------------------------------

/// How the record was attached to the entity it describes.
///
/// Kept because the roadmap's rule for M1 is that a file matched to a release
/// approximately is never treated as certain. A value reached by asking for a
/// known identifier and a value reached by guessing from a name are different
/// claims, and a layer that stored them identically could not offer the
/// review that makes an approximate match acceptable in the first place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// The source was asked about this exact thing, by identifier.
    Identified,
    /// Reached by matching names and metadata, scored from 0 to 100.
    Matched(u8),
}

impl Confidence {
    /// A matched confidence, with the score bounded to 0–100.
    ///
    /// Bounded here rather than trusted from the caller: a score above 100
    /// would print as one and sort above a certainty, which is exactly the
    /// confusion this type exists to prevent.
    pub fn matched(score: u8) -> Confidence {
        Confidence::Matched(score.min(100))
    }

    /// `true` when the record was reached by identifier rather than by guess.
    pub fn is_certain(self) -> bool {
        matches!(self, Confidence::Identified)
    }
}

// --------------------------------------------------------------------------
// What a source says
// --------------------------------------------------------------------------

/// A piece of prose taken from somewhere, with what its licence obliges.
///
/// **One value, not four fields.** Wikipedia is CC BY-SA: the text may be
/// reused, and attribution has to travel with it. A `text` field beside a
/// separate optional `url` would make it possible — and therefore eventually
/// certain — to hold the words without the credit: one code path that fills
/// the first and forgets the second, one export that copies one and not the
/// other, and the project is quietly out of compliance.
///
/// So they are the same value. There is no way to store the sentence without
/// storing where it came from, in which language, and under what terms,
/// because the type does not offer one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prose {
    /// The words themselves.
    pub text: String,
    /// The page they came from, which is the attribution a reader can follow.
    pub url: String,
    /// Language code of that page, so a reader knows what they are getting.
    pub lang: String,
    /// The licence, named as the source names it — `CC BY-SA 4.0`.
    ///
    /// **Stored per record, not looked up from a constant.** It is the same
    /// string for every Wikipedia article today, so carrying a copy on each row
    /// looks like waste — about twenty-four bytes, ten kilobytes over a large
    /// library. It buys two things that a constant read at display time cannot.
    ///
    /// It is a **fact about the fetch**, like [`SourceRecord::fetched_at`]: it
    /// says what these words were taken under, not what the current build
    /// believes they would be taken under now. Wikimedia has already moved once
    /// — CC BY-SA 3.0 to 4.0 — and with a constant that upgrade silently
    /// relabels every paragraph fetched before it.
    ///
    /// And `Prose` is not Wikipedia's. It is prose with its terms, so the next
    /// source of a biography — a plugin, a label, a discography site under
    /// quite different terms — files it in this same field, and the record says
    /// which. Moving the licence out to the source that usually supplies it
    /// would make the second source a special case of the first.
    pub licence: String,
}

impl Prose {
    /// The line that has to appear wherever the text does.
    ///
    /// The **language is named**, and that is not decoration. This text is
    /// fetched in one language and kept in one language, so a reader who wanted
    /// French and is reading English has no way to tell whether their
    /// preference was ignored, whether the article does not exist in French, or
    /// whether the prose was fetched before they expressed a preference at all.
    /// The field was stored from the first version and displayed nowhere — the
    /// same dead end `source_id` and the fingerprints had. Now the screen says
    /// which language it is, and the way to ask for another is one line below.
    pub fn credit(&self) -> String {
        format!("{} — in {} — {}", self.url, self.lang, self.licence)
    }
}

/// A picture of an artist, as a source answered it.
///
/// Just the address, deliberately as thin as [`Prose`] is thick. `Prose`
/// cannot exist without stating the page, the language and the licence
/// because the words cannot be shown without them; a picture is written to
/// disk as a file and shown as one, and the file itself is what a viewer
/// looks at — the record here exists only so that `fetch --portraits` does
/// not ask the same question of the same artist on every run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Where the image was downloaded from.
    pub url: String,
}

/// What a source says about one artist.
///
/// Every field is optional: a source may not hold it, and an empty answer is
/// not an error. None of these have a tag counterpart — there is no widely
/// used tag for an artist's country — so this is knowledge the files simply
/// do not carry, rather than a second opinion about something they do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtistFacts {
    /// Country or area of origin, as the source names it.
    pub area: Option<String>,
    /// The same country as a two-letter code, when the source gives one.
    ///
    /// Kept **beside** the name rather than instead of it, because the source
    /// states both and they answer different needs: `area` is what a reader
    /// sees, the code is what they can type. It used to be read only as a
    /// fallback for a missing name and otherwise dropped — a fact the source
    /// had stated, thrown away because another field happened to be filled.
    ///
    /// ISO 3166-1 alpha-2, so the United Kingdom is `GB` and not `UK`.
    pub country_code: Option<String>,
    /// Formation date: a year, or a fuller date when the source has one.
    pub began: Option<String>,
    /// When it ended, for a group that has.
    pub ended: Option<String>,
    /// Whether it is still going, when the source says so.
    ///
    /// Distinct from [`ArtistFacts::ended`] being absent, and the distinction
    /// is the whole point: a band with no end date may be one that never
    /// stopped, or one nobody has filled in. `Some(true)` is an answer,
    /// `None` is a silence, and a reader shown the same thing for both learns
    /// nothing.
    pub active: Option<bool>,
    /// `person`, `group`, `orchestra`, `choir`… as the source classifies it.
    pub kind: Option<String>,
    /// The short phrase a source uses to tell two same-named artists apart —
    /// "US industrial metal band", "UK folk singer", and sometimes something
    /// as unhelpful as "the band".
    ///
    /// **Not a description**, and it must not be labelled as one: it exists to
    /// separate two entries that share a name, so it is written against
    /// whatever the other entry is. Shown as a note for that reason.
    pub disambiguation: Option<String>,
    /// Genres, as the source's editors voted them, most agreed first.
    ///
    /// Beside the genre tag rather than over it: what a crowd calls a record
    /// and what its tags call it are two answers, and the second is the one
    /// the user chose to write.
    pub genres: Vec<String>,
    /// Other names the same artist is known by.
    pub aliases: Vec<String>,
    /// Wikidata entity page, when the source links to one.
    ///
    /// The one link that leads somewhere else: Wikidata is how a real article
    /// is reached in the reader's own language, so it is stored under its own
    /// name rather than lost in a list of URLs.
    pub wikidata: Option<String>,
    /// Discogs page, when the source links to one.
    pub discogs: Option<String>,
    /// The artist's own site, when they have one the source knows about.
    pub homepage: Option<String>,
    /// A few sentences about the artist, with the attribution its licence
    /// requires — see [`Prose`].
    ///
    /// MusicBrainz never fills this: it holds identifiers and relationships,
    /// not prose. It comes from an encyclopaedia, and it is stored as one
    /// inseparable value for the reason written on the type.
    pub summary: Option<Prose>,
    /// Every record the source credits to this artist — see [`KnownRelease`].
    ///
    /// **On the artist, and deliberately not on the releases.** A release key
    /// is `artist|title|folder`, and an album you do not own has no folder, so
    /// a record you are missing cannot be an entity of this layer at all. It is
    /// instead a fact *about the artist*: "MusicBrainz credits fourteen albums
    /// to this name".
    ///
    /// Which of them are missing from the shelf is then **derived on read**, by
    /// comparing that list with the catalog — the same rule the whole layer
    /// follows for agreement with a tag. Storing "missing" would go stale the
    /// moment the album is bought, and the catalog would hold a claim it had
    /// stopped being able to justify.
    pub discography: Vec<KnownRelease>,
    /// Who played in this band, or which bands this person played in — see
    /// [`Membership`].
    ///
    /// One list, not two, because MusicBrainz states **one** relation and the
    /// two readings are the same fact seen from either end. Splitting it here
    /// would mean deciding at write time which question a reader was going to
    /// ask, and getting it wrong for a person who is both — a soloist with a
    /// backing band under their own name is exactly that.
    pub members: Vec<Membership>,
    /// A picture of the artist, when a source holds an address for one — see
    /// [`Picture`].
    ///
    /// The address, never the bytes. The image itself is written to disk —
    /// beside the music when one folder holds every album of theirs, an
    /// `assets/` folder otherwise — and rediscovered from there, the same way
    /// `cover.jpg` is never registered anywhere either. What is kept here is
    /// only the fact that a source was asked and what it answered, so a later
    /// run does not ask again for nothing: see `fetch --portraits`.
    pub portrait: Option<Picture>,
}

impl ArtistFacts {
    /// The line-up: everyone the source says played in **this** artist.
    pub fn line_up(&self) -> impl Iterator<Item = &Membership> {
        self.members.iter().filter(|m| m.side == Side::Player)
    }

    /// The other way round: the bands **this** artist played in.
    pub fn bands(&self) -> impl Iterator<Item = &Membership> {
        self.members.iter().filter(|m| m.side == Side::Group)
    }

    /// Who the source places in this band in a given year, and how many
    /// members it cannot place at all.
    ///
    /// **This is the whole reason the dates are worth fetching**: a line-up is
    /// a fact about an artist, an album is a fact about a year, and crossing
    /// them answers the question a listener actually asks — who was in the band
    /// when this record came out.
    ///
    /// Derived when read, never stored. A stored line-up would be a claim the
    /// catalog had stopped being able to justify the moment either side
    /// changed, and it is the same rule the missing-albums list already
    /// follows.
    ///
    /// A row is placed only when the source's own dates put it there: an
    /// undated membership, or one the source says has ended without saying
    /// when, is **counted rather than shown**. Including it would put a
    /// musician on a record they may not be on, which is the one mistake here
    /// that matters; dropping it in silence would be a filter the reader
    /// cannot see.
    pub fn line_up_in(&self, year: u32) -> (Vec<&Membership>, usize) {
        let mut placed = Vec::new();
        let mut undated = 0;
        for member in self.line_up() {
            match member.covers(year) {
                Some(true) => placed.push(member),
                Some(false) => {}
                None => undated += 1,
            }
        }
        (placed, undated)
    }
}

/// Which end of a membership the named artist is.
///
/// A membership is one relation between two artists, and MusicBrainz returns
/// it on both of them with a direction. Losing that direction is not a cosmetic
/// mistake: it puts Judas Priest in the list of Ozzy Osbourne's members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The named artist played **in** the artist this record describes, which
    /// therefore is a band.
    Player,
    /// The named artist **is** the band, and the artist this record describes
    /// played in it.
    Group,
}

/// One spell in a band, as a source dates it.
///
/// **A person can appear twice**, and the second row is not a duplicate: Ozzy
/// Osbourne was in Black Sabbath from 1968 to 1979 and again from 1997, and a
/// list that folded the two into one span would claim he was there in 1985.
/// MusicBrainz states them as two relations and they are kept as two rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// The other artist's MusicBrainz identifier, which is what makes a later
    /// fetch about them an update rather than a second opinion.
    pub mbid: String,
    /// The other artist's name, as the source spells it.
    pub name: String,
    /// Which end of the relation that name is — see [`Side`].
    pub side: Side,
    /// How the source names the relationship, in its own words: `member of
    /// band`, `instrumental supporting musician`.
    ///
    /// Kept verbatim and shown verbatim, the same rule
    /// [`KnownRelease::stated_type`] follows. The two are not the same fact — a
    /// founding member and a guitarist hired for one tour are both on a record
    /// and only one of them was in the band — and paraphrasing them into one
    /// word would throw away the distinction MusicBrainz took the trouble to
    /// draw.
    pub kind: String,
    /// What the source lists against the relationship.
    ///
    /// **Not "instruments", although most of them are.** Measured on a real
    /// answer: Judas Priest's line-up carries `["guitar family", "original"]`
    /// on one row, where `original` marks an original member and is not an
    /// instrument at all. Calling this field `instruments` would put a lie in
    /// a column header, so it carries the source's own word and its own
    /// values.
    pub attributes: Vec<String>,
    /// When it started: a year, or a fuller date when the source has one.
    pub began: Option<String>,
    /// When it stopped.
    pub ended: Option<String>,
    /// Whether the source says it is over.
    ///
    /// The same distinction [`ArtistFacts::active`] draws, and it matters more
    /// here: a membership with no end date may be a current one or one nobody
    /// has filled in, and printing "1968–" for both tells the reader something
    /// the source never said. `Some(false)` is "still in the band".
    pub over: Option<bool>,
}

impl Membership {
    /// Whether this membership was running in a given year: `Some(true)`,
    /// `Some(false)`, or `None` where the source's dates cannot say.
    ///
    /// The three-way answer is the point. A membership with no start date
    /// places nobody, and one the source says has **ended without saying
    /// when** places nobody either — the end is somewhere, and "somewhere"
    /// cannot be compared with a year. Both come back `None`, so a caller has
    /// to decide what to do about them rather than being handed a `false` that
    /// looks like knowledge.
    pub fn covers(&self, year: u32) -> Option<bool> {
        let started = self.year(self.began.as_deref())?;
        if started > year {
            return Some(false);
        }
        match (self.year(self.ended.as_deref()), self.over) {
            (Some(stopped), _) => Some(stopped >= year),
            // Still in the band, said so by the source.
            (None, Some(false)) => Some(true),
            // Over, with no date for it, or nothing said at all.
            (None, _) => None,
        }
    }

    /// The year out of a MusicBrainz date, which is `1968` or `1968-02-13`.
    fn year(&self, date: Option<&str>) -> Option<u32> {
        date?.get(..4)?.parse().ok()
    }

    /// The years as a reader recognises them: `1968–1979`, `1968–` for a
    /// current member, `1968` where the source only knows a start and does not
    /// say whether it ended.
    ///
    /// **Never invents an end.** A dash trailing into nothing is a claim that
    /// the membership is still going, and it is one MusicBrainz makes with
    /// `ended: false` rather than with a missing date.
    pub fn years(&self) -> String {
        let began = self.began.as_deref().unwrap_or("");
        match (&self.ended, self.over) {
            (Some(end), _) => format!("{began}–{end}"),
            (None, Some(false)) => format!("{began}–"),
            // Said to be over, with no date for it: the reader is told that
            // much rather than being shown an open range that would be wrong.
            (None, Some(true)) => format!("{began}–?"),
            (None, None) => began.to_string(),
        }
    }
}

/// One record a source credits to an artist, whether or not you own it.
///
/// Enough to recognise it and to show it: the identifier that makes a second
/// fetch an update, the title, when it first appeared, and what kind of record
/// it is. Not a [`ReleaseFacts`], which is what a source says about an album
/// *in the catalog* — this is a name on a list, and the difference matters:
/// one is attached to files, the other is precisely the one that is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRelease {
    /// Release-group identifier, which is what the catalog's own
    /// `release_group_mbid` can be compared against without guessing.
    pub mbid: String,
    /// Title as the source spells it.
    pub title: String,
    /// When the album first appeared — the year a reader recognises it by.
    pub first_released: Option<String>,
    /// Album, Single, EP, Broadcast, Other.
    pub primary_type: Option<String>,
    /// Compilation, Soundtrack, Live, Remix, Demo…
    pub secondary_types: Vec<String>,
}

impl KnownRelease {
    /// `true` for a studio album: primary type Album, and nothing else.
    ///
    /// The filter that makes a completeness report readable rather than a wall.
    /// A discography holds every single, every live recording, every
    /// compilation somebody assembled, and reporting all of them as *missing*
    /// would be true and useless — nobody's shelf holds every single ever
    /// pressed, and a report that is mostly noise is a report nobody opens.
    ///
    /// Applied here, on what came back, rather than trusted to the request: a
    /// server-side filter narrows what travels, but only this decides what is
    /// shown, so a parameter the service ignores costs bandwidth and not
    /// correctness.
    pub fn is_studio_album(&self) -> bool {
        self.primary_type.as_deref() == Some("Album") && self.secondary_types.is_empty()
    }

    /// What the source calls this record, in the source's own words.
    ///
    /// For a report that has left something out to be able to say **why**. The
    /// words are MusicBrainz's — `Album · Live`, `Single`, `Album · Compilation`
    /// — rather than a vocabulary of our own, because the reader's next move is
    /// often to go and correct the type on the page where it is set, and a
    /// paraphrase would send them looking for a word that is not written there.
    ///
    /// A record nobody has typed at all says so: it is not a studio album
    /// either, and having no type is exactly why it was left out.
    pub fn stated_type(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(primary) = &self.primary_type {
            parts.push(primary);
        }
        parts.extend(self.secondary_types.iter().map(String::as_str));
        match parts.is_empty() {
            true => "no type".to_string(),
            false => parts.join(" · "),
        }
    }

    /// The year, for a date that may be a year, a month or a full date.
    pub fn year(&self) -> Option<&str> {
        self.first_released.as_deref().map(|d| &d[..4.min(d.len())])
    }
}

/// What a source says about one release.
///
/// Unlike the artist fields, these do have tag counterparts — Picard writes
/// `RELEASETYPE`, `DATE` and `LABEL` — which is what makes a release the place
/// where agreement and disagreement can actually be observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseFacts {
    /// Album, Single, EP, Broadcast, Other — the release group's primary type.
    pub primary_type: Option<String>,
    /// Compilation, Soundtrack, Live, Remix, Demo… any number of them.
    pub secondary_types: Vec<String>,
    /// First release date of the group, which is not the date of this edition:
    /// a 2011 remaster of a 1973 album was first released in 1973.
    pub first_released: Option<String>,
    /// Label, as the source names it.
    pub label: Option<String>,
    /// Address of the front image, when a source holds one.
    ///
    /// The address rather than the picture: a catalog is a description of a
    /// library, and megabytes of artwork inside `sources.json` would make every
    /// load of it slower for something that belongs beside the music. The image
    /// is written into the album's folder, where the next scan discovers it
    /// exactly as it would one the user put there.
    ///
    /// Kept so that "asked, and there is a cover" can be told from "asked, and
    /// there is none" — the distinction this whole layer exists for — and so
    /// that a second run costs nothing.
    pub cover_art: Option<String>,
}

/// What a source says, for one kind of entity.
///
/// An enum rather than a flat bag of `(field, value)` strings: the display,
/// the query grammar and `doctor` all have to know what a field *means*, and a
/// generic bag pushes that knowledge into string literals scattered through
/// the program. [`crate::analysis::FileAnalysis`] made the same choice.
///
/// # The two variants are not the same size, on purpose
///
/// An artist carries twelve fields, one of them a paragraph; a release carries
/// four. Clippy points out that a `Facts` is therefore as large as its larger
/// variant, and the usual remedy is to box that one. It is the wrong remedy
/// here: artist records are the majority of this layer, so boxing them puts a
/// heap allocation and a pointer chase on the common path in order to shrink
/// the rare one. The whole store is a few hundred rows of a few hundred bytes
/// — under a hundred kilobytes — and it is read once at start-up.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Facts {
    /// About an artist.
    Artist(ArtistFacts),
    /// About a release.
    Release(ReleaseFacts),
    /// About one recorded performance, identified by its sound.
    Track(TrackFacts),
}

impl Facts {
    /// The kind of entity these facts can describe.
    ///
    /// This is what makes a record's entity kind and its contents impossible
    /// to contradict each other: the kind is derived from the facts rather
    /// than stored beside them.
    pub fn kind(&self) -> EntityKind {
        match self {
            Facts::Artist(_) => EntityKind::Artist,
            Facts::Release(_) => EntityKind::Release,
            Facts::Track(_) => EntityKind::Track,
        }
    }

    /// `true` when the source answered but said nothing at all.
    ///
    /// Distinct from having no record: "asked, and it holds nothing about
    /// this" and "never asked" are two different states, and the whole layer
    /// exists to keep them apart.
    pub fn is_empty(&self) -> bool {
        match self {
            Facts::Artist(a) => a == &ArtistFacts::default(),
            Facts::Release(r) => r == &ReleaseFacts::default(),
            Facts::Track(t) => t == &TrackFacts::default(),
        }
    }
}

/// What a source says about one recorded performance.
///
/// **A recording, not a release.** AcoustID answers what is *playing*, and the
/// same performance sits on the original album, the compilation and the
/// deluxe reissue alike — so this identifies the track and says nothing about
/// which pressing the file was ripped from, which the tags answer better.
///
/// Everything here arrives as [`Confidence::Matched`], never `Identified`: a
/// fingerprint match is a strong guess and is wrong in ways that are easy to
/// picture — two masterings of one recording fingerprint alike, and a very
/// short or silent track matches a great deal. The score is kept so that a
/// reader can see what they are being told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackFacts {
    /// The MusicBrainz recording identifier the source named.
    pub recording: Option<String>,
    /// How sure the source said it was, as a percentage.
    ///
    /// Rounded on the way in, from the fraction the service states. A
    /// stored float compares badly — two records that are the same answer
    /// would not be equal — and the difference between 98.12 % and 98 % is
    /// one nobody acts on. What a reader wants is a number to put beside a
    /// tag.
    pub score: Option<u8>,
    /// The title, as the source spells it.
    pub title: Option<String>,
    /// The artists credited, in the order given.
    pub artists: Vec<String>,
    /// One release group the recording appears on, when the source named one.
    pub album: Option<String>,
}

/// One entity, as one source describes it.
///
/// The key is the entity's [`EntityRef`] key rather than a catalog id, for the
/// reason annotations use the same scheme: ids are positions that a scan
/// renumbers, and a record may describe an entity the catalog does not hold
/// yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    /// The [`EntityRef`] key of the entity described.
    pub key: String,
    /// Who says so: [`MUSICBRAINZ`], or the name of a plugin.
    pub source: String,
    /// The identifier that source uses — an MBID — which is what turns a
    /// second fetch into an update rather than a duplicate.
    pub source_id: Option<String>,
    /// When it was fetched, in seconds since the Unix epoch, so a value can be
    /// shown as old rather than silently trusted forever.
    pub fetched_at: u64,
    /// How firmly this is attached to the entity.
    pub confidence: Confidence,
    /// What was said.
    pub facts: Facts,
}

impl SourceRecord {
    /// The entity this record describes.
    pub fn entity(&self) -> EntityRef {
        EntityRef {
            kind: self.facts.kind(),
            key: self.key.clone(),
        }
    }
}

// --------------------------------------------------------------------------
// The store
// --------------------------------------------------------------------------

/// Everything other sources have said, as it sits in `sources.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sources {
    /// One row per entity and source.
    pub records: Vec<SourceRecord>,
}

impl Sources {
    /// What one source says about one entity, if it has said anything.
    pub fn get(&self, entity: &EntityRef, source: &str) -> Option<&SourceRecord> {
        self.records
            .iter()
            .find(|r| r.source == source && r.key == entity.key && r.facts.kind() == entity.kind)
    }

    /// Everything every source says about one entity.
    ///
    /// Several rows are the point rather than a defect: two sources that
    /// disagree are two rows, and the disagreement is reported instead of
    /// being resolved behind the user's back.
    pub fn about<'a>(&'a self, entity: &'a EntityRef) -> impl Iterator<Item = &'a SourceRecord> {
        self.records
            .iter()
            .filter(move |r| r.key == entity.key && r.facts.kind() == entity.kind)
    }

    /// Files a record, replacing whatever that same source said before.
    ///
    /// Returns `true` when it replaced an existing row. A second fetch is an
    /// update, not a duplicate — but only for the same source: what
    /// MusicBrainz says never overwrites what a plugin said, which is the
    /// whole reason the source is part of the key.
    pub fn set(&mut self, record: SourceRecord) -> bool {
        let same = |r: &SourceRecord| {
            r.source == record.source
                && r.key == record.key
                && r.facts.kind() == record.facts.kind()
        };
        match self.records.iter().position(same) {
            Some(i) => {
                self.records[i] = record;
                true
            }
            None => {
                self.records.push(record);
                false
            }
        }
    }

    /// Drops everything one source ever said, and reports how much that was.
    ///
    /// The counterpart of an attributed layer: a value that can be traced to a
    /// source can also be removed by naming that source, without touching what
    /// anybody else said.
    pub fn forget(&mut self, source: &str) -> usize {
        let before = self.records.len();
        self.records.retain(|r| r.source != source);
        before - self.records.len()
    }

    /// Drops everything said about one entity, whoever said it.
    pub fn forget_entity(&mut self, entity: &EntityRef) -> usize {
        let before = self.records.len();
        self.records
            .retain(|r| !(r.key == entity.key && r.facts.kind() == entity.kind));
        before - self.records.len()
    }
}

// --------------------------------------------------------------------------
// How much of it the catalog can currently place
// --------------------------------------------------------------------------

/// How many records the catalog can place, and how many are waiting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attachment {
    /// Records whose entity is in the catalog.
    pub attached: usize,
    /// Records whose entity is not, and which are kept exactly as they are.
    pub waiting: usize,
}

/// Counts what this catalog can currently place.
///
/// Nothing is rewritten here, and that is the difference with
/// [`crate::user::reconcile`]. Annotations are reattached in place because a
/// note whose file moved is **lost** otherwise, so it is worth guessing from a
/// name and a size when exactly one file matches.
///
/// A fetched value is not in that position: it is re-fetchable. A release key
/// carries the folder the album sits in, so moving an album does break the
/// attachment — and the right answer there is to ask the source again, not to
/// guess. Guessing would risk filing what MusicBrainz said about one album
/// onto another to save a network call, which is a poor trade in a layer whose
/// whole promise is that every value can be traced to what it describes.
///
/// So a record that does not resolve simply waits, exactly as an imported
/// analysis waits for the file it describes to be scanned.
pub fn attachment(sources: &Sources, catalog: &crate::model::Catalog) -> Attachment {
    let mut report = Attachment::default();
    for record in &sources.records {
        match record.entity().resolve(catalog).is_some() {
            true => report.attached += 1,
            false => report.waiting += 1,
        }
    }
    report
}

// --------------------------------------------------------------------------
// The verdict, which is derived and never stored
// --------------------------------------------------------------------------

/// What a source's value amounts to, next to the tag it can be compared with.
///
/// **Derived on read, never stored.** A stored "agrees" goes stale the moment
/// the file is re-tagged, and the catalog would then hold a claim it has
/// stopped being able to justify. What is stored is the answer itself, whole;
/// this is computed from it. That is also what makes "does my tag still
/// match?" an offline question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The source and the tag say the same thing.
    Agrees,
    /// They do not, and both are named so the reader can judge.
    Differs {
        /// What the source says.
        theirs: String,
        /// What the tag says.
        yours: String,
    },
    /// There is no tag to compare against, so the source adds rather than
    /// contradicts.
    NothingToCompare,
}

/// Compares what a source says with what the tags say.
///
/// The comparison goes through [`crate::text::normalize`], the same
/// normalisation the catalog uses to decide that two spellings are one name —
/// case, diacritics, punctuation and a leading article. `Album` and `album`,
/// `Björk` and `Bjork`, `The Beatles` and `Beatles, The` are agreement, and
/// reporting them as disagreement would train the reader to skip the report.
///
/// What it does **not** fold is worth stating, because it is the case that
/// will produce the first false alarm: `&` and `and` are different words to
/// `normalize`, so `Rock & Roll` against `Rock and Roll` is reported as a
/// difference. Widening the normalisation is the wrong fix — it is the
/// catalog's identity function, and loosening it there to quiet a report here
/// would start merging artists. If the noise becomes real, the answer is a
/// comparison of its own, not a change to what counts as one name.
pub fn verdict(theirs: &str, yours: Option<&str>) -> Verdict {
    let Some(yours) = yours.map(str::trim).filter(|y| !y.is_empty()) else {
        return Verdict::NothingToCompare;
    };
    match crate::text::normalize(theirs) == crate::text::normalize(yours) {
        true => Verdict::Agrees,
        false => Verdict::Differs {
            theirs: theirs.trim().to_string(),
            yours: yours.to_string(),
        },
    }
}

/// The verdict for a field that holds a **set**, not a value: genres.
///
/// Comparing sets as strings is what produced the first false alarm of this
/// layer: MusicBrainz said `pop, dance-pop, electropop, europop`, the files said
/// `Rock, Pop`, and the report called that a disagreement. It is not. The tags
/// say the record is pop *and* rock; MusicBrainz says pop and three finer words
/// for it. Nobody is contradicting anybody.
///
/// So the rule is **overlap, not equality**. Genres are not exclusive and the
/// two sides are not even trying to answer with the same granularity — one is
/// what a crowd voted, the other is what one person typed. A shared name is
/// agreement; only two sets with nothing at all in common are a difference
/// worth showing.
///
/// Splitting is done here rather than by the caller because a genre tag is
/// written in every shape: several tag values, or one value holding
/// `Rock, Pop`, `Rock; Pop` or `Rock / Pop`. All of them mean a list.
pub fn verdict_set(theirs: &[String], yours: &[String]) -> Verdict {
    let split = |values: &[String]| -> Vec<String> {
        values
            .iter()
            .flat_map(|value| value.split([',', ';', '/']))
            .map(crate::text::normalize)
            .filter(|name| !name.is_empty())
            .collect()
    };
    let (mine, other) = (split(theirs), split(yours));
    if other.is_empty() {
        return Verdict::NothingToCompare;
    }
    if mine.iter().any(|name| other.contains(name)) {
        return Verdict::Agrees;
    }
    Verdict::Differs {
        theirs: theirs.join(", "),
        yours: yours.join(", "),
    }
}

/// Compares two dates written at different precisions.
///
/// The plain [`verdict`] is wrong for dates and would be wrong on nearly every
/// album: MusicBrainz answers `1973-03-01` where a tag almost always holds
/// `1973`, and reporting that as a disagreement would fill the report with
/// noise on the first run and teach the reader to stop looking.
///
/// So when either side gives only a year, only the years are compared. When
/// both are precise, they are compared as they are — two full dates that
/// differ really are a disagreement.
pub fn verdict_date(theirs: &str, yours: Option<&str>) -> Verdict {
    let Some(yours) = yours.map(str::trim).filter(|y| !y.is_empty()) else {
        return Verdict::NothingToCompare;
    };
    let theirs = theirs.trim();

    fn year(text: &str) -> Option<&str> {
        let head = text.get(..4)?;
        head.chars().all(|c| c.is_ascii_digit()).then_some(head)
    }
    let (Some(mine), Some(other)) = (year(theirs), year(yours)) else {
        return verdict(theirs, Some(yours));
    };

    // One side is a bare year: that is the precision of the comparison, not a
    // difference of opinion about the date.
    let bare = theirs.len() == 4 || yours.len() == 4;
    let same = match bare {
        true => mine == other,
        false => crate::text::normalize(theirs) == crate::text::normalize(yours),
    };
    match same {
        true => Verdict::Agrees,
        false => Verdict::Differs {
            theirs: theirs.to_string(),
            yours: yours.to_string(),
        },
    }
}

// --------------------------------------------------------------------------
// Persistence
// --------------------------------------------------------------------------

/// Where `sources.json` sits inside a data folder.
pub fn sources_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(SOURCES_FILE)
}

/// Writes the layer, atomically and pretty.
///
/// Pretty like `user.json` and unlike the catalog: this file is small, and it
/// is one a user may want to open to see what was fetched and from where.
pub fn save(sources: &Sources, path: &Path) -> Result<(), crate::store::StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, to_json(sources).to_string_pretty())?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Reads the layer; `Ok(None)` when nothing has ever been fetched.
pub fn load(path: &Path) -> Result<Option<Sources>, crate::store::StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value = crate::json::parse(&text).map_err(crate::store::StoreError::Parse)?;
    from_json(&value).map(Some)
}

/// A list of strings, as the document carries them.
fn strings(values: &[String]) -> Json {
    Json::Arr(values.iter().map(|v| Json::Str(v.clone())).collect())
}

/// Reads back what [`strings`] wrote; an absent field is an empty list, which
/// is what an older document carries for a field this build added.
fn read_strings(value: &Json, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Json::as_arr)
        .map(|a| a.iter().filter_map(Json::as_string).collect())
        .unwrap_or_default()
}

fn opt_str(value: &Option<String>) -> Json {
    match value {
        Some(s) => Json::Str(s.clone()),
        None => Json::Null,
    }
}

/// The document: a version, and one array of records.
pub fn to_json(sources: &Sources) -> Json {
    let mut root = Json::obj();
    root.set("format_version", SOURCES_FORMAT_VERSION.into());

    let records: Vec<Json> = sources
        .records
        .iter()
        .map(|r| {
            let mut o = Json::obj();
            o.set("entity", r.entity().to_token().into());
            o.set("source", r.source.clone().into());
            o.set("source_id", opt_str(&r.source_id));
            o.set("fetched_at", r.fetched_at.into());
            match r.confidence {
                Confidence::Identified => o.set("confidence", "identified".into()),
                Confidence::Matched(score) => {
                    o.set("confidence", "matched".into());
                    o.set("score", u32::from(score).into());
                }
            }
            let mut facts = Json::obj();
            match &r.facts {
                Facts::Track(t) => {
                    facts.set("recording", opt_str(&t.recording));
                    facts.set(
                        "score",
                        match t.score {
                            Some(per_cent) => u32::from(per_cent).into(),
                            None => Json::Null,
                        },
                    );
                    facts.set("title", opt_str(&t.title));
                    facts.set("artists", strings(&t.artists));
                    facts.set("album", opt_str(&t.album));
                }
                Facts::Artist(a) => {
                    facts.set("area", opt_str(&a.area));
                    facts.set("country_code", opt_str(&a.country_code));
                    facts.set("began", opt_str(&a.began));
                    facts.set("ended", opt_str(&a.ended));
                    facts.set(
                        "active",
                        match a.active {
                            Some(active) => Json::Bool(active),
                            None => Json::Null,
                        },
                    );
                    facts.set("kind", opt_str(&a.kind));
                    facts.set("disambiguation", opt_str(&a.disambiguation));
                    facts.set("genres", strings(&a.genres));
                    facts.set("aliases", strings(&a.aliases));
                    facts.set("wikidata", opt_str(&a.wikidata));
                    facts.set("discogs", opt_str(&a.discogs));
                    facts.set("homepage", opt_str(&a.homepage));
                    // Written as an array of objects rather than a flat list
                    // of identifiers: a reader shown "you are missing three
                    // albums" needs their titles and years, and re-fetching
                    // them from the identifiers would be a request per album
                    // for something already known.
                    facts.set(
                        "discography",
                        Json::Arr(
                            a.discography
                                .iter()
                                .map(|known| {
                                    let mut o = Json::obj();
                                    o.set("mbid", known.mbid.clone().into());
                                    o.set("title", known.title.clone().into());
                                    o.set("first_released", opt_str(&known.first_released));
                                    o.set("primary_type", opt_str(&known.primary_type));
                                    o.set("secondary_types", strings(&known.secondary_types));
                                    o
                                })
                                .collect(),
                        ),
                    );
                    facts.set(
                        "members",
                        Json::Arr(
                            a.members
                                .iter()
                                .map(|m| {
                                    let mut o = Json::obj();
                                    o.set("mbid", m.mbid.clone().into());
                                    o.set("name", m.name.clone().into());
                                    // Spelt out rather than written as a flag:
                                    // a hand-editable file where the direction
                                    // is `true` is a file nobody can correct.
                                    o.set(
                                        "side",
                                        match m.side {
                                            Side::Player => "player",
                                            Side::Group => "group",
                                        }
                                        .into(),
                                    );
                                    o.set("kind", m.kind.clone().into());
                                    o.set("attributes", strings(&m.attributes));
                                    o.set("began", opt_str(&m.began));
                                    o.set("ended", opt_str(&m.ended));
                                    o.set(
                                        "over",
                                        match m.over {
                                            Some(over) => Json::Bool(over),
                                            None => Json::Null,
                                        },
                                    );
                                    o
                                })
                                .collect(),
                        ),
                    );
                    facts.set(
                        "summary",
                        match &a.summary {
                            // Written as one object for the same reason it is
                            // read as one: a document cannot carry the words
                            // without the credit either.
                            Some(prose) => {
                                let mut o = Json::obj();
                                o.set("text", prose.text.clone().into());
                                o.set("url", prose.url.clone().into());
                                o.set("lang", prose.lang.clone().into());
                                o.set("licence", prose.licence.clone().into());
                                o
                            }
                            None => Json::Null,
                        },
                    );
                    facts.set(
                        "portrait",
                        match &a.portrait {
                            Some(picture) => {
                                let mut o = Json::obj();
                                o.set("url", picture.url.clone().into());
                                o
                            }
                            None => Json::Null,
                        },
                    );
                }
                Facts::Release(rel) => {
                    facts.set("primary_type", opt_str(&rel.primary_type));
                    facts.set(
                        "secondary_types",
                        Json::Arr(
                            rel.secondary_types
                                .iter()
                                .map(|t| Json::Str(t.clone()))
                                .collect(),
                        ),
                    );
                    facts.set("first_released", opt_str(&rel.first_released));
                    facts.set("label", opt_str(&rel.label));
                    facts.set("cover_art", opt_str(&rel.cover_art));
                }
            }
            o.set("facts", facts);
            o
        })
        .collect();
    root.set("records", Json::Arr(records));
    root
}

/// Reads back what [`to_json`] wrote.
///
/// A document of another version is refused rather than read approximately:
/// the same rule the catalog and `user.json` already follow.
pub fn from_json(value: &Json) -> Result<Sources, crate::store::StoreError> {
    use crate::store::StoreError;
    let found = value.field_u32("format_version").unwrap_or(0);
    if found != SOURCES_FORMAT_VERSION {
        return Err(StoreError::Version {
            found,
            expected: SOURCES_FORMAT_VERSION,
        });
    }

    let mut sources = Sources::default();
    let rows = value.get("records").and_then(Json::as_arr).unwrap_or(&[]);
    for row in rows {
        // A row naming an entity kind this build does not know, or carrying no
        // facts of a shape it understands, is skipped rather than fatal: the
        // layer is additional by nature, and refusing to start because one
        // fetched fact is unreadable would be a poor trade.
        let Some(entity) = row
            .field_str("entity")
            .and_then(|t| EntityRef::parse_token(&t))
        else {
            continue;
        };
        let facts = row.get("facts");
        let facts = match entity.kind {
            EntityKind::Artist => Facts::Artist(ArtistFacts {
                area: facts.and_then(|f| f.field_str("area")),
                // Absent from every record written before this field existed,
                // which is exactly what `Option` is for: those artists keep
                // their country and simply cannot be reached by its code
                // until the next fetch.
                country_code: facts.and_then(|f| f.field_str("country_code")),
                began: facts.and_then(|f| f.field_str("began")),
                ended: facts.and_then(|f| f.field_str("ended")),
                active: facts.and_then(|f| f.field_optional_bool("active")),
                kind: facts.and_then(|f| f.field_str("kind")),
                disambiguation: facts.and_then(|f| f.field_str("disambiguation")),
                genres: facts.map(|f| read_strings(f, "genres")).unwrap_or_default(),
                aliases: facts
                    .map(|f| read_strings(f, "aliases"))
                    .unwrap_or_default(),
                wikidata: facts.and_then(|f| f.field_str("wikidata")),
                discogs: facts.and_then(|f| f.field_str("discogs")),
                homepage: facts.and_then(|f| f.field_str("homepage")),
                // A row with no identifier is skipped: without it the only
                // way to tell "you have this one" from "you are missing it"
                // is the title, and two records share a title often enough
                // that a wish list assembled from titles alone is wrong.
                discography: facts
                    .and_then(|f| f.get("discography"))
                    .and_then(Json::as_arr)
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                Some(KnownRelease {
                                    mbid: row.field_str("mbid")?,
                                    title: row.field_str("title").unwrap_or_default(),
                                    first_released: row.field_str("first_released"),
                                    primary_type: row.field_str("primary_type"),
                                    secondary_types: read_strings(row, "secondary_types"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                members: facts
                    .and_then(|f| f.get("members"))
                    .and_then(Json::as_arr)
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| {
                                // No identifier and no direction, no
                                // membership: a name alone cannot say which
                                // end of the relation it is, and guessing
                                // would put a band among a person's members.
                                Some(Membership {
                                    mbid: row.field_str("mbid")?,
                                    name: row.field_str("name").unwrap_or_default(),
                                    side: match row.field_str("side")?.as_str() {
                                        "player" => Side::Player,
                                        "group" => Side::Group,
                                        _ => return None,
                                    },
                                    kind: row.field_str("kind").unwrap_or_default(),
                                    attributes: read_strings(row, "attributes"),
                                    began: row.field_str("began"),
                                    ended: row.field_str("ended"),
                                    over: row.get("over").and_then(Json::as_bool),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                // All four or none: a row that lost its attribution somewhere
                // is a row this build will not repeat.
                summary: facts.and_then(|f| f.get("summary")).and_then(|p| {
                    Some(Prose {
                        text: p.field_str("text")?,
                        url: p.field_str("url")?,
                        lang: p.field_str("lang")?,
                        licence: p.field_str("licence")?,
                    })
                }),
                // Absent from every record written before this field existed,
                // the same as `country_code` above: those artists simply have
                // not been asked for a portrait yet.
                portrait: facts.and_then(|f| f.get("portrait")).and_then(|p| {
                    Some(Picture {
                        url: p.field_str("url")?,
                    })
                }),
            }),
            EntityKind::Release => Facts::Release(ReleaseFacts {
                primary_type: facts.and_then(|f| f.field_str("primary_type")),
                secondary_types: facts
                    .and_then(|f| f.get("secondary_types"))
                    .and_then(Json::as_arr)
                    .map(|a| a.iter().filter_map(Json::as_string).collect())
                    .unwrap_or_default(),
                first_released: facts.and_then(|f| f.field_str("first_released")),
                label: facts.and_then(|f| f.field_str("label")),
                cover_art: facts.and_then(|f| f.field_str("cover_art")),
            }),
            EntityKind::Track => Facts::Track(TrackFacts {
                recording: facts.and_then(|f| f.field_str("recording")),
                // Read back as a percentage and clamped, so a hand-edited
                // file cannot put 4000 % beside somebody's tags.
                score: facts
                    .and_then(|f| f.field_u32("score"))
                    .map(|value| value.min(100) as u8),
                title: facts.and_then(|f| f.field_str("title")),
                artists: facts
                    .map(|f| read_strings(f, "artists"))
                    .unwrap_or_default(),
                album: facts.and_then(|f| f.field_str("album")),
            }),
            _ => continue,
        };

        let confidence = match row.field_str("confidence").as_deref() {
            Some("matched") => Confidence::matched(
                row.field_u32("score")
                    .unwrap_or(0)
                    .try_into()
                    .unwrap_or(u8::MAX),
            ),
            _ => Confidence::Identified,
        };

        sources.records.push(SourceRecord {
            key: entity.key,
            source: row.field_str("source").unwrap_or_default(),
            source_id: row.field_str("source_id"),
            fetched_at: row.field_u64("fetched_at").unwrap_or(0),
            confidence,
            facts,
        });
    }
    Ok(sources)
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
