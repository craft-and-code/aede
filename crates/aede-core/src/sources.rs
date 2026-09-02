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
}

/// What a source says, for one kind of entity.
///
/// An enum rather than a flat bag of `(field, value)` strings: the display,
/// the query grammar and `doctor` all have to know what a field *means*, and a
/// generic bag pushes that knowledge into string literals scattered through
/// the program. [`crate::analysis::FileAnalysis`] made the same choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Facts {
    /// About an artist.
    Artist(ArtistFacts),
    /// About a release.
    Release(ReleaseFacts),
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
        }
    }
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
                Facts::Artist(a) => {
                    facts.set("area", opt_str(&a.area));
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
mod tests {
    use super::*;

    fn release(primary: &str, label: &str) -> Facts {
        Facts::Release(ReleaseFacts {
            primary_type: Some(primary.to_string()),
            secondary_types: vec!["Live".to_string()],
            first_released: Some("1973".to_string()),
            label: Some(label.to_string()),
        })
    }

    fn record(key: &str, source: &str, facts: Facts) -> SourceRecord {
        SourceRecord {
            key: key.to_string(),
            source: source.to_string(),
            source_id: Some("6a1b…".to_string()),
            fetched_at: 1_700_000_000,
            confidence: Confidence::Identified,
            facts,
        }
    }

    #[test]
    fn a_second_fetch_updates_and_does_not_duplicate() {
        // The point of keying on (entity, source): asking MusicBrainz twice
        // must leave one answer, not two that a reader has to arbitrate.
        let mut sources = Sources::default();
        let entity = EntityRef {
            kind: EntityKind::Release,
            key: "pink floyd|dark side|/music".to_string(),
        };

        assert!(!sources.set(record(
            &entity.key,
            MUSICBRAINZ,
            release("Album", "Harvest")
        )));
        assert!(sources.set(record(&entity.key, MUSICBRAINZ, release("Album", "EMI"))));
        assert_eq!(sources.records.len(), 1, "one source, one answer");
        assert_eq!(
            sources.get(&entity, MUSICBRAINZ).map(|r| &r.facts),
            Some(&release("Album", "EMI")),
            "the newer answer replaced the older one"
        );

        // A second source is a second row: two of them disagreeing is
        // information, and merging them would destroy it.
        sources.set(record(&entity.key, "discogs", release("Album", "Capitol")));
        assert_eq!(sources.records.len(), 2);
        assert_eq!(sources.about(&entity).count(), 2);
    }

    #[test]
    fn a_source_can_be_forgotten_without_touching_the_others() {
        let mut sources = Sources::default();
        sources.set(record("a", MUSICBRAINZ, release("Album", "Harvest")));
        sources.set(record("b", MUSICBRAINZ, release("EP", "Harvest")));
        sources.set(record("a", "discogs", release("Album", "Capitol")));

        assert_eq!(sources.forget(MUSICBRAINZ), 2);
        assert_eq!(sources.records.len(), 1);
        assert_eq!(sources.records[0].source, "discogs");
    }

    #[test]
    fn two_kinds_sharing_a_key_are_two_entities() {
        // Keys are only unique within a kind: an artist and a release may
        // perfectly well be spelled the same, and reading one as the other
        // would attach a country to an album.
        let mut sources = Sources::default();
        sources.set(record("nirvana", MUSICBRAINZ, release("Album", "Sub Pop")));
        sources.set(record(
            "nirvana",
            MUSICBRAINZ,
            Facts::Artist(ArtistFacts {
                area: Some("United States".to_string()),
                ..Default::default()
            }),
        ));
        assert_eq!(sources.records.len(), 2, "two kinds, two rows");

        let artist = EntityRef {
            kind: EntityKind::Artist,
            key: "nirvana".to_string(),
        };
        assert_eq!(sources.about(&artist).count(), 1);
        assert!(matches!(
            sources.get(&artist, MUSICBRAINZ).map(|r| &r.facts),
            Some(Facts::Artist(_))
        ));
    }

    #[test]
    fn the_verdict_is_about_meaning_and_not_about_spelling() {
        // Reporting `Album` against `album` as a disagreement would teach the
        // reader to skip the report, which costs more than the report is worth.
        assert_eq!(verdict("Album", Some("album")), Verdict::Agrees);
        assert_eq!(verdict("Björk", Some("Bjork")), Verdict::Agrees);
        assert_eq!(
            verdict("The Beatles", Some("Beatles, The")),
            Verdict::Agrees
        );

        // And the limit of that, pinned rather than left to be discovered: an
        // ampersand is not the word "and" to `normalize`, so this reports a
        // difference. Widening `normalize` to quiet it would loosen the rule
        // that decides two artists are one, which costs far more.
        assert!(
            matches!(
                verdict("Rock & Roll", Some("Rock and Roll")),
                Verdict::Differs { .. }
            ),
            "known limit: punctuation is dropped, `&` is not read as a word"
        );

        assert_eq!(
            verdict("Album", Some("EP")),
            Verdict::Differs {
                theirs: "Album".to_string(),
                yours: "EP".to_string()
            },
            "a real difference names both sides so the reader can judge"
        );

        // No tag is not a disagreement: the source is adding, not contradicting.
        assert_eq!(verdict("Album", None), Verdict::NothingToCompare);
        assert_eq!(verdict("Album", Some("   ")), Verdict::NothingToCompare);
    }

    #[test]
    fn a_year_and_a_full_date_are_not_a_disagreement() {
        // The false alarm this would otherwise produce on nearly every album:
        // MusicBrainz answers a full date, a tag almost always holds a year.
        assert_eq!(verdict_date("1973-03-01", Some("1973")), Verdict::Agrees);
        assert_eq!(verdict_date("1973", Some("1973-03-01")), Verdict::Agrees);
        assert_eq!(
            verdict_date("1973-03-01", Some("1973-03-01")),
            Verdict::Agrees
        );

        // A real difference of year is still one, at either precision.
        assert!(matches!(
            verdict_date("1973-03-01", Some("1974")),
            Verdict::Differs { .. }
        ));
        // And two precise dates that differ are a disagreement, which is the
        // case a year-only comparison would have hidden.
        assert!(matches!(
            verdict_date("1973-03-01", Some("1973-03-24")),
            Verdict::Differs { .. }
        ));

        assert_eq!(verdict_date("1973", None), Verdict::NothingToCompare);
        // Not a date at all: falls back to the ordinary comparison rather than
        // inventing a year out of the first four characters.
        assert_eq!(verdict_date("unknown", Some("Unknown")), Verdict::Agrees);
    }

    #[test]
    fn an_answer_holding_nothing_is_not_the_absence_of_an_answer() {
        // "Asked, and MusicBrainz holds nothing about this artist" and "never
        // asked" are different states, and the layer exists to keep them apart.
        let empty = Facts::Artist(ArtistFacts::default());
        assert!(empty.is_empty());

        let mut sources = Sources::default();
        sources.set(record("someone", MUSICBRAINZ, empty));
        let entity = EntityRef {
            kind: EntityKind::Artist,
            key: "someone".to_string(),
        };
        assert!(
            sources.get(&entity, MUSICBRAINZ).is_some(),
            "the record exists, and says the source had nothing"
        );
    }

    #[test]
    fn a_round_trip_keeps_every_field() {
        let mut sources = Sources::default();
        sources.set(SourceRecord {
            key: "pink floyd|dark side|/music".to_string(),
            source: MUSICBRAINZ.to_string(),
            source_id: Some("f5093c06".to_string()),
            fetched_at: 1_700_000_123,
            confidence: Confidence::matched(72),
            facts: release("Album", "Harvest"),
        });
        sources.set(SourceRecord {
            key: "miles davis".to_string(),
            source: "discogs".to_string(),
            source_id: None,
            fetched_at: 1_700_000_456,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                area: Some("United States".to_string()),
                began: Some("1926-05-26".to_string()),
                ended: Some("1991-09-28".to_string()),
                active: Some(false),
                kind: Some("person".to_string()),
                disambiguation: Some("the trumpeter".to_string()),
                genres: vec!["jazz".to_string(), "cool jazz".to_string()],
                aliases: vec!["Miles Dewey Davis III".to_string()],
                wikidata: Some("https://www.wikidata.org/wiki/Q93341".to_string()),
                discogs: None,
                homepage: None,
            }),
        });

        let text = to_json(&sources).to_string_pretty();
        let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
        assert_eq!(back, sources, "written and read back are the same layer");
    }

    #[test]
    fn a_document_of_another_version_is_refused() {
        let mut root = Json::obj();
        root.set("format_version", (SOURCES_FORMAT_VERSION + 1).into());
        root.set("records", Json::Arr(vec![]));
        assert!(
            from_json(&root).is_err(),
            "a newer document is refused, not read approximately"
        );
    }

    #[test]
    fn a_row_this_build_cannot_read_is_skipped_and_the_rest_survives() {
        // The layer is additional by nature: one unreadable fetched fact must
        // not stop the program from starting.
        let text = format!(
            r#"{{"format_version":{SOURCES_FORMAT_VERSION},"records":[
                 {{"entity":"nonsense","source":"x","facts":{{}}}},
                 {{"entity":"genre:jazz","source":"x","facts":{{}}}},
                 {{"entity":"artist:miles davis","source":"musicbrainz",
                   "confidence":"identified","fetched_at":1,
                   "facts":{{"area":"United States"}}}}
               ]}}"#
        );
        let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("a layer");
        assert_eq!(back.records.len(), 1, "the readable row survived alone");
        assert_eq!(back.records[0].key, "miles davis");
    }
}
