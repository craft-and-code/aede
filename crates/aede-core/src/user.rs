//! What the user writes: favourites, ratings, notes, tags, listening history.
//!
//! This is the only data in the program that cannot be recovered. Lose the
//! catalog and a scan rebuilds it in a minute; lose this and it is gone. Two
//! consequences run through the whole module.
//!
//! **It lives in its own file**, never in the catalog. The catalog is derived
//! from the disk and written whole; a rating that changes on a keystroke has no
//! business rewriting a library.
//!
//! **It is never keyed by a catalog identifier.** Those are positions in a
//! vector that every scan renumbers — the lesson the imported analyses already
//! taught, at the cost of a rewrite. A [`EntityRef`] names a thing the way the
//! thing names itself, and an annotation whose target is missing is **kept
//! waiting, never dropped**: a folder renamed between two scans must not cost
//! anybody a note they wrote.
//!
//! Favourites, ratings, notes and tags look like four features and are one:
//! something a person said about an entity. One record per target, so copying
//! everything said about one album onto another is a record copy rather than a
//! loop over four kinds.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Catalog, EntityKind, Id};
use crate::text;

/// Version of the user file on disk, independent of the catalog's.
///
/// The two change for different reasons and at different times, so they are
/// counted separately. Reading a newer file must fail loudly rather than lose
/// what it did not understand.
pub const USER_FORMAT_VERSION: u32 = 1;

/// Name of the user file inside the data folder.
pub const USER_FILE: &str = "user.json";

/// Whose opinion it is.
///
/// There is one owner today and there will be several: the Subsonic surface has
/// accounts by definition, and starred items are per user in that API. The
/// field is here from the first version so that arriving at that point is a new
/// value rather than a migration — the single-user case being the multi-user
/// case with one user, on the same code path, exercised on every run.
pub type UserRef = String;

/// The owner of a library nobody has named yet.
pub const LOCAL_USER: &str = "local";

/// What separates the three parts of a release key.
pub const RELEASE_KEY_SEPARATOR: char = '|';

/// A stable way to name an entity, independent of any scan.
///
/// The key is what the thing calls itself rather than where it currently sits:
/// a path for a track, the release key for an album, the normalized name for
/// the rest. At M1 the MusicBrainz identifier becomes a better key still and
/// these become the fallback.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityRef {
    /// Which kind of thing is named.
    pub kind: EntityKind,
    /// Its stable key, in the spelling [`EntityRef`] builds.
    pub key: String,
}

impl EntityRef {
    /// Builds a reference by hand, for a key already in the right shape.
    pub fn new(kind: EntityKind, key: impl Into<String>) -> EntityRef {
        EntityRef {
            kind,
            key: key.into(),
        }
    }

    /// How the reference is written in one token: `track:/music/a.flac`.
    ///
    /// The kind comes first and the key may itself contain colons — a path
    /// does — so the split is on the **first** one only.
    pub fn to_token(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.key)
    }

    /// Reads back what [`EntityRef::to_token`] wrote.
    pub fn parse_token(text: &str) -> Option<EntityRef> {
        let (kind, key) = text.split_once(':')?;
        Some(EntityRef {
            kind: EntityKind::parse_kind(kind)?,
            key: key.to_string(),
        })
    }

    /// The reference naming an entity of a catalog, if that entity exists.
    pub fn of(catalog: &Catalog, kind: EntityKind, id: Id) -> Option<EntityRef> {
        let key = match kind {
            // The path is what the file calls itself, and it survives every
            // rescan that leaves the file where it is.
            EntityKind::Track => catalog
                .track(id)
                .and_then(|t| catalog.file(t.file_id))
                .map(|f| f.path.clone())?,
            // The same three parts the release is built on, so a release
            // recognised as itself by a scan is recognised here too.
            EntityKind::Release => {
                let release = catalog.release(id)?;
                let artist = release
                    .album_artist_id
                    .and_then(|a| catalog.artist(a))
                    .map(|a| a.key.clone())
                    .unwrap_or_default();
                let folder = release
                    .track_ids
                    .first()
                    .and_then(|&t| catalog.track(t))
                    .and_then(|t| catalog.file(t.file_id))
                    .map(|f| text::folder(&f.path).to_string())
                    .unwrap_or_default();
                format!(
                    "{artist}{RELEASE_KEY_SEPARATOR}{}{RELEASE_KEY_SEPARATOR}{folder}",
                    text::normalize(&release.title)
                )
            }
            EntityKind::Artist => catalog.artist(id).map(|a| a.key.clone())?,
            EntityKind::Label => catalog.label(id).map(|l| l.key.clone())?,
            EntityKind::Genre => catalog.genres.get(id as usize).map(|g| g.key.clone())?,
        };
        Some(EntityRef { kind, key })
    }

    /// The entity a reference names in this catalog, if it holds one.
    pub fn resolve(&self, catalog: &Catalog) -> Option<Id> {
        match self.kind {
            EntityKind::Track => catalog
                .tracks
                .iter()
                .find(|t| catalog.file(t.file_id).is_some_and(|f| f.path == self.key))
                .map(|t| t.id),
            EntityKind::Release => catalog
                .releases
                .iter()
                .find(|r| EntityRef::of(catalog, EntityKind::Release, r.id).as_ref() == Some(self))
                .map(|r| r.id),
            EntityKind::Artist => catalog
                .artists
                .iter()
                .find(|a| a.key == self.key)
                .map(|a| a.id),
            EntityKind::Label => catalog
                .labels
                .iter()
                .find(|l| l.key == self.key)
                .map(|l| l.id),
            EntityKind::Genre => catalog
                .genres
                .iter()
                .find(|g| g.key == self.key)
                .map(|g| g.id),
        }
    }

    /// How the thing is called on screen, or the key when it is gone.
    pub fn display_name(&self, catalog: &Catalog) -> String {
        let named = self.resolve(catalog).and_then(|id| match self.kind {
            EntityKind::Track => catalog.track(id).map(|t| t.title.clone()),
            EntityKind::Release => catalog.release(id).map(|r| r.title.clone()),
            EntityKind::Artist => catalog.artist(id).map(|a| a.name.clone()),
            EntityKind::Label => catalog.label(id).map(|l| l.name.clone()),
            EntityKind::Genre => catalog.genres.get(id as usize).map(|g| g.name.clone()),
        });
        named.unwrap_or_else(|| match self.kind {
            // A path is unreadable in a column; its file name is not.
            EntityKind::Track => text::file_name(&self.key).to_string(),
            _ => self.key.clone(),
        })
    }
}

/// Everything one person said about one entity.
///
/// One record per target rather than one per fact: a favourite, a rating, a
/// note and a set of tags are four ways of having an opinion, not four
/// features, and keeping them together is what makes copying what was said
/// about an album onto another album a single operation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Annotation {
    /// Whose opinion it is.
    pub owner: UserRef,
    /// What it is about.
    pub target: EntityRef,
    /// A favourite.
    pub loved: bool,
    /// One to five stars, or nothing said.
    pub rating: Option<u8>,
    /// Free text, as long as the user cares to make it.
    pub note: Option<String>,
    /// Free labels: "to rip again", "vinyl", "for the car".
    pub tags: BTreeSet<String>,
    /// When it was first written (Unix epoch, seconds).
    pub created_at: u64,
    /// When it was last touched.
    pub updated_at: u64,
}

impl Default for EntityRef {
    fn default() -> EntityRef {
        EntityRef {
            kind: EntityKind::Track,
            key: String::new(),
        }
    }
}

impl Annotation {
    /// `true` when the record says nothing at all and can be dropped.
    ///
    /// Removing the last thing said about an album must remove the record, not
    /// leave an empty one that a listing would then have to filter out — and
    /// that an export would carry for ever.
    pub fn is_empty(&self) -> bool {
        !self.loved && self.rating.is_none() && self.note.is_none() && self.tags.is_empty()
    }
}

/// One listening event.
///
/// An annotation is a statement; a play is an event, and events accumulate.
#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    /// Who listened.
    pub owner: UserRef,
    /// What was played.
    pub track: EntityRef,
    /// When it started (Unix epoch, seconds).
    pub at: u64,
    /// How much of it was actually heard.
    pub ms_played: u64,
    /// Whether it played to the end.
    ///
    /// A track skipped after eight seconds is evidence *against* it, and a
    /// history that cannot tell a skip from a listen measures the wrong thing.
    pub completed: bool,
}

/// How often one person played one track, for as long as the library lasts.
///
/// Deliberately not derived from the log: the log is bounded so that the file
/// stays small, and "what have I never heard" — the question M3's `discover`
/// shuffle asks — cannot be answered from a truncated one. Two structures,
/// because they answer two questions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayCount {
    /// Whose count it is.
    pub owner: UserRef,
    /// Which track.
    pub track: EntityRef,
    /// How many times it was played, all-time.
    pub count: u32,
    /// When it was last played.
    pub last_played: u64,
}

/// A query saved under a name: a smart collection.
///
/// It holds the expression rather than the result, so it answers with what the
/// library holds *now* — which is the whole difference between a smart
/// collection and a playlist. And since running one produces a selection, and a
/// selection is what `--csv`, `--m3u` and M3's queue consume, a saved query is
/// playable the day it is written, with nothing built for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// Whose collection it is.
    pub owner: UserRef,
    /// What it is called, as typed.
    pub name: String,
    /// The expression, kept as written rather than as parsed: a grammar that
    /// gains a field must not silently change what an old collection means.
    pub expression: String,
    /// When it was first saved.
    pub created_at: u64,
    /// When it was last changed.
    pub updated_at: u64,
}

/// Events kept in the log before the oldest are forgotten.
///
/// The log answers "what did I listen to last night"; the counters answer
/// everything that needs all of history. Bounding the first keeps the file
/// small without costing the second anything.
pub const HISTORY_LIMIT: usize = 500;

/// A record the user has taken off a report, and why that is allowed.
///
/// **The one thing in this file that is not keyed on an entity of the
/// catalog.** Everything else here describes something the library holds; this
/// describes something it deliberately does *not*. `aede missing` lists albums a
/// source credits to an artist and the shelf lacks, and sometimes the source is
/// simply wrong about what an album is — a demo, a compilation and a single all
/// arrive typed `Album` until somebody says otherwise on MusicBrainz.
///
/// Aède will not overrule a source: the whole attributed layer exists so that
/// what somebody else said stays what they said, correctable only by them. But
/// it will record that **you** disagree, which is what this file has always
/// been for, and stop putting the record in front of you.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetAside {
    /// Whose decision it is.
    pub owner: UserRef,
    /// The MusicBrainz release-group identifier, which is the key.
    ///
    /// Globally unique, so no artist is needed to tell two apart, and stable
    /// across a re-fetch and a rescan — where a title is neither.
    pub release_group: String,
    /// The title as it read when it was set aside.
    ///
    /// Kept so a listing can show something a person recognises. Without it the
    /// list is a column of identifiers, and a decision nobody can read is one
    /// nobody can undo.
    pub title: String,
    /// When it was set aside.
    pub created_at: u64,
}

/// Everything the user wrote, as it sits on disk.
#[derive(Debug, Clone, Default)]
pub struct UserData {
    /// One record per owner and target.
    pub annotations: Vec<Annotation>,
    /// The most recent plays, oldest first, bounded by [`HISTORY_LIMIT`].
    pub plays: Vec<Play>,
    /// All-time counts, which never forget.
    pub counts: Vec<PlayCount>,
    /// Saved queries, by name.
    pub collections: Vec<Collection>,
    /// Records taken off the `missing` report — see [`SetAside`].
    pub set_aside: Vec<SetAside>,
}

impl UserData {
    /// The record for one owner and target, if there is one.
    pub fn find(&self, owner: &str, target: &EntityRef) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|a| a.owner == owner && &a.target == target)
    }

    /// The record for one owner and target, opening one if needed.
    ///
    /// `now` stamps a record that did not exist; an existing one keeps its
    /// creation date and has its `updated_at` moved by the caller.
    pub fn entry(&mut self, owner: &str, target: &EntityRef, now: u64) -> &mut Annotation {
        let found = self
            .annotations
            .iter()
            .position(|a| a.owner == owner && &a.target == target);
        let at = match found {
            Some(at) => at,
            None => {
                self.annotations.push(Annotation {
                    owner: owner.to_string(),
                    target: target.clone(),
                    created_at: now,
                    updated_at: now,
                    ..Default::default()
                });
                self.annotations.len() - 1
            }
        };
        &mut self.annotations[at]
    }

    /// Drops the records that no longer say anything.
    ///
    /// Called after every change, so that un-loving the one album somebody ever
    /// loved leaves nothing behind.
    pub fn forget_empty(&mut self) {
        self.annotations.retain(|a| !a.is_empty());
    }

    /// Copies everything said about one target onto another.
    ///
    /// Returns `false` when there was nothing to copy — which is worth telling
    /// the user, since a silent success there means they typed the wrong name
    /// and will not find out until they look.
    pub fn copy(&mut self, owner: &str, from: &EntityRef, to: &EntityRef, now: u64) -> bool {
        let Some(source) = self.find(owner, from).cloned() else {
            return false;
        };
        let target = self.entry(owner, to, now);
        target.loved = source.loved;
        target.rating = source.rating;
        target.note = source.note.clone();
        target.tags = source.tags.clone();
        target.updated_at = now;
        true
    }

    /// Records a play: one event in the log, one more on the counter.
    /// Takes back the most recent play of a track, log and counter together.
    ///
    /// The counter is not a summary of the log — the log is bounded at
    /// [`HISTORY_LIMIT`] and the counter is not — so a removal that touched one
    /// and not the other would leave a track played "three times" with two
    /// plays behind it, and nothing on screen to say which was right. They are
    /// written together by [`UserData::record_play`] and they are taken back
    /// together here.
    ///
    /// Returns `false` when there was nothing to take back, which the caller
    /// says rather than reporting a removal that did not happen.
    pub fn forget_last_play(&mut self, owner: &str, track: &EntityRef) -> bool {
        let Some(index) = self
            .plays
            .iter()
            .rposition(|p| p.owner == owner && &p.track == track)
        else {
            // The log may have rolled the play off its front while the counter
            // still holds it. Decrementing on a play nobody can point at would
            // be guessing, so it is refused.
            return false;
        };
        self.plays.remove(index);
        if let Some(counter) = self
            .counts
            .iter_mut()
            .find(|c| c.owner == owner && &c.track == track)
        {
            counter.count = counter.count.saturating_sub(1);
            // The last-played date now belongs to whatever play is newest of
            // those left, and to nothing at all when none is.
            counter.last_played = self
                .plays
                .iter()
                .filter(|p| p.owner == owner && &p.track == track)
                .map(|p| p.at)
                .max()
                .unwrap_or(0);
        }
        self.counts.retain(|c| c.count > 0);
        true
    }

    /// Forgets everything an owner ever played.
    ///
    /// Both structures, for the reason above. Returns how many plays and how
    /// many counters went, because "your history is cleared" is a claim nobody
    /// can check and a number is.
    pub fn forget_history(&mut self, owner: &str) -> (usize, usize) {
        let plays = self.plays.iter().filter(|p| p.owner == owner).count();
        let counts = self.counts.iter().filter(|c| c.owner == owner).count();
        self.plays.retain(|p| p.owner != owner);
        self.counts.retain(|c| c.owner != owner);
        (plays, counts)
    }

    /// Records one listen: the bounded log and the all-time counter together.
    ///
    /// Both, always — see [`UserData::forget_last_play`] for why the two must
    /// never be written apart.
    pub fn record_play(&mut self, play: Play) {
        match self
            .counts
            .iter_mut()
            .find(|c| c.owner == play.owner && c.track == play.track)
        {
            Some(counter) => {
                counter.count = counter.count.saturating_add(1);
                counter.last_played = counter.last_played.max(play.at);
            }
            None => self.counts.push(PlayCount {
                owner: play.owner.clone(),
                track: play.track.clone(),
                count: 1,
                last_played: play.at,
            }),
        }
        self.plays.push(play);
        // Oldest first, so the excess falls off the front.
        if self.plays.len() > HISTORY_LIMIT {
            let excess = self.plays.len() - HISTORY_LIMIT;
            self.plays.drain(..excess);
        }
    }

    /// A saved query by name, compared without regard to case or accents:
    /// somebody typing `aede collection Metal` means the one they saved as
    /// "metal".
    pub fn collection(&self, owner: &str, name: &str) -> Option<&Collection> {
        let wanted = text::normalize(name);
        self.collections
            .iter()
            .find(|c| c.owner == owner && text::normalize(&c.name) == wanted)
    }

    /// Saves a query under a name, replacing one of the same name.
    ///
    /// Returns `true` when a collection was replaced, which the caller says out
    /// loud: overwriting somebody's saved query in silence is how they find out
    /// a week later.
    pub fn save_collection(&mut self, owner: &str, name: &str, expression: &str, now: u64) -> bool {
        let wanted = text::normalize(name);
        if let Some(existing) = self
            .collections
            .iter_mut()
            .find(|c| c.owner == owner && text::normalize(&c.name) == wanted)
        {
            existing.expression = expression.to_string();
            existing.updated_at = now;
            return true;
        }
        self.collections.push(Collection {
            owner: owner.to_string(),
            name: name.to_string(),
            expression: expression.to_string(),
            created_at: now,
            updated_at: now,
        });
        false
    }

    /// Drops a saved query; `false` when there was none by that name.
    pub fn forget_collection(&mut self, owner: &str, name: &str) -> bool {
        let wanted = text::normalize(name);
        let before = self.collections.len();
        self.collections
            .retain(|c| !(c.owner == owner && text::normalize(&c.name) == wanted));
        self.collections.len() != before
    }

    /// How many times one person played a track, all-time.
    pub fn play_count(&self, owner: &str, track: &EntityRef) -> u32 {
        self.counts
            .iter()
            .find(|c| c.owner == owner && &c.track == track)
            .map(|c| c.count)
            .unwrap_or(0)
    }
}

/// What a reconciliation found.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Attachment {
    /// Records whose target is in the catalog.
    pub attached: usize,
    /// Records whose target is not, and which are kept as they are.
    pub waiting: usize,
    /// Records whose key was rewritten because the file moved.
    pub moved: usize,
}

/// Reattaches what the user wrote to a catalog that has been rebuilt.
///
/// A scan renumbers everything and may have seen a file move. A reference that
/// no longer resolves is tried again by **file name and size**, and rewritten
/// when exactly one file matches — one, because two files of the same name and
/// size give no reason to prefer either, and guessing there would move somebody
/// else's note onto the wrong track.
///
/// What still does not resolve is left alone. An annotation is never dropped
/// for want of a target: the folder may be on a drive that is simply not
/// plugged in.
pub fn reconcile(data: &mut UserData, catalog: &Catalog) -> Attachment {
    let mut report = Attachment::default();
    let by_name_and_size = movable_files(catalog);

    for target in data
        .annotations
        .iter_mut()
        .map(|a| &mut a.target)
        .chain(data.plays.iter_mut().map(|p| &mut p.track))
        .chain(data.counts.iter_mut().map(|c| &mut c.track))
    {
        if target.resolve(catalog).is_some() {
            report.attached += 1;
            continue;
        }
        if target.kind == EntityKind::Track
            && let Some(path) = moved_to(&by_name_and_size, &target.key)
        {
            target.key = path;
            report.moved += 1;
            report.attached += 1;
            continue;
        }
        report.waiting += 1;
    }
    report
}

/// Files reachable by name and size, for the ones that appear exactly once.
///
/// A name and a size shared by two files identify neither, so those are left
/// out rather than picked between.
fn movable_files(catalog: &Catalog) -> BTreeMap<(String, u64), Option<String>> {
    let mut seen: BTreeMap<(String, u64), Option<String>> = BTreeMap::new();
    for file in &catalog.files {
        let key = (text::file_name(&file.path).to_string(), file.size);
        seen.entry(key)
            .and_modify(|slot| *slot = None)
            .or_insert(Some(file.path.clone()));
    }
    seen
}

/// The single file that carries the same name as a vanished path.
///
/// The size of the old file is unknown — it is gone — so only the name is
/// matched, and only when it is unique in the whole library.
fn moved_to(files: &BTreeMap<(String, u64), Option<String>>, old_path: &str) -> Option<String> {
    let name = text::file_name(old_path);
    let mut found: Option<&String> = None;
    for ((candidate, _), path) in files {
        if candidate != name {
            continue;
        }
        let Some(path) = path else { return None };
        if found.is_some() {
            return None;
        }
        found = Some(path);
    }
    found.cloned()
}

// --------------------------------------------------------------------------
// On disk
// --------------------------------------------------------------------------

/// Where the user file sits inside a data folder.
pub fn user_path(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join(USER_FILE)
}

/// Saves what the user wrote, atomically.
///
/// Written **pretty**, unlike the catalog. The catalog is machine output that
/// nobody reads; this file is the one a user may want to open, grep, or repair
/// by hand after a bad restore, and it is small enough for that to cost
/// nothing.
pub fn save(data: &UserData, path: &std::path::Path) -> Result<(), crate::store::StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, to_json(data).to_string_pretty())?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Loads what the user wrote; `Ok(None)` when nothing has been written yet.
pub fn load(path: &std::path::Path) -> Result<Option<UserData>, crate::store::StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value = crate::json::parse(&text).map_err(crate::store::StoreError::Parse)?;
    from_json(&value).map(Some)
}

/// The document, one array per table.
pub fn to_json(data: &UserData) -> crate::json::Json {
    use crate::json::Json;
    let mut root = Json::obj();
    root.set("format_version", USER_FORMAT_VERSION.into());

    let annotations: Vec<Json> = data
        .annotations
        .iter()
        .map(|a| {
            let mut o = Json::obj();
            o.set("owner", a.owner.as_str().into());
            o.set("target", a.target.to_token().into());
            if a.loved {
                o.set("loved", Json::Bool(true));
            }
            if let Some(rating) = a.rating {
                o.set("rating", u32::from(rating).into());
            }
            if let Some(note) = &a.note {
                o.set("note", note.as_str().into());
            }
            if !a.tags.is_empty() {
                o.set(
                    "tags",
                    Json::Arr(a.tags.iter().map(|t| t.as_str().into()).collect()),
                );
            }
            o.set("created_at", a.created_at.into());
            o.set("updated_at", a.updated_at.into());
            o
        })
        .collect();
    root.set("annotations", Json::Arr(annotations));

    let plays: Vec<Json> = data
        .plays
        .iter()
        .map(|p| {
            let mut o = Json::obj();
            o.set("owner", p.owner.as_str().into());
            o.set("track", p.track.to_token().into());
            o.set("at", p.at.into());
            o.set("ms_played", p.ms_played.into());
            o.set("completed", Json::Bool(p.completed));
            o
        })
        .collect();
    root.set("plays", Json::Arr(plays));

    let counts: Vec<Json> = data
        .counts
        .iter()
        .map(|c| {
            let mut o = Json::obj();
            o.set("owner", c.owner.as_str().into());
            o.set("track", c.track.to_token().into());
            o.set("count", c.count.into());
            o.set("last_played", c.last_played.into());
            o
        })
        .collect();
    root.set("counts", Json::Arr(counts));

    let collections: Vec<Json> = data
        .collections
        .iter()
        .map(|c| {
            let mut o = Json::obj();
            o.set("owner", c.owner.as_str().into());
            o.set("name", c.name.as_str().into());
            o.set("query", c.expression.as_str().into());
            o.set("created_at", c.created_at.into());
            o.set("updated_at", c.updated_at.into());
            o
        })
        .collect();
    root.set("collections", Json::Arr(collections));

    let set_aside: Vec<Json> = data
        .set_aside
        .iter()
        .map(|a| {
            let mut o = Json::obj();
            o.set("owner", a.owner.as_str().into());
            o.set("release_group", a.release_group.as_str().into());
            o.set("title", a.title.as_str().into());
            o.set("created_at", a.created_at.into());
            o
        })
        .collect();
    root.set("set_aside", Json::Arr(set_aside));
    root
}

/// Reads the document back.
///
/// A row whose target cannot be parsed is **skipped rather than fatal**: one
/// corrupted line must not cost the user every note they ever wrote, which is
/// the opposite of the catalog's rule, where a broken row means the graph does
/// not hold together and refusing is the safe answer. Here there is no graph —
/// only statements, each standing alone.
pub fn from_json(value: &crate::json::Json) -> Result<UserData, crate::store::StoreError> {
    use crate::store::StoreError;
    let found = value.field_u32("format_version").unwrap_or(0);
    if found != USER_FORMAT_VERSION {
        return Err(StoreError::Version {
            found,
            expected: USER_FORMAT_VERSION,
        });
    }
    let mut data = UserData::default();

    for row in value
        .get("annotations")
        .and_then(|v| v.as_arr())
        .unwrap_or(&[])
    {
        let Some(target) = row
            .field_str("target")
            .and_then(|t| EntityRef::parse_token(&t))
        else {
            continue;
        };
        data.annotations.push(Annotation {
            owner: row.field_str("owner").unwrap_or_else(|| LOCAL_USER.into()),
            target,
            loved: row.field_bool("loved"),
            rating: row.field_u32("rating").map(|r| r.clamp(1, 5) as u8),
            note: row.field_str("note"),
            tags: row
                .get("tags")
                .and_then(|v| v.as_arr())
                .map(|list| list.iter().filter_map(|t| t.as_string()).collect())
                .unwrap_or_default(),
            created_at: row.field_u64("created_at").unwrap_or(0),
            updated_at: row.field_u64("updated_at").unwrap_or(0),
        });
    }

    for row in value.get("plays").and_then(|v| v.as_arr()).unwrap_or(&[]) {
        let Some(track) = row
            .field_str("track")
            .and_then(|t| EntityRef::parse_token(&t))
        else {
            continue;
        };
        data.plays.push(Play {
            owner: row.field_str("owner").unwrap_or_else(|| LOCAL_USER.into()),
            track,
            at: row.field_u64("at").unwrap_or(0),
            ms_played: row.field_u64("ms_played").unwrap_or(0),
            completed: row.field_bool("completed"),
        });
    }

    for row in value.get("counts").and_then(|v| v.as_arr()).unwrap_or(&[]) {
        let Some(track) = row
            .field_str("track")
            .and_then(|t| EntityRef::parse_token(&t))
        else {
            continue;
        };
        data.counts.push(PlayCount {
            owner: row.field_str("owner").unwrap_or_else(|| LOCAL_USER.into()),
            track,
            count: row.field_u32("count").unwrap_or(0),
            last_played: row.field_u64("last_played").unwrap_or(0),
        });
    }
    for row in value
        .get("collections")
        .and_then(|v| v.as_arr())
        .unwrap_or(&[])
    {
        let (Some(name), Some(expression)) = (row.field_str("name"), row.field_str("query")) else {
            continue;
        };
        data.collections.push(Collection {
            owner: row.field_str("owner").unwrap_or_else(|| LOCAL_USER.into()),
            name,
            expression,
            created_at: row.field_u64("created_at").unwrap_or(0),
            updated_at: row.field_u64("updated_at").unwrap_or(0),
        });
    }
    for row in value
        .get("set_aside")
        .and_then(crate::json::Json::as_arr)
        .unwrap_or(&[])
    {
        // No identifier, no decision: the title alone cannot say which record
        // was meant, and a wish list quietly shortened by one is worse than
        // one item too long.
        let Some(release_group) = row.field_str("release_group") else {
            continue;
        };
        data.set_aside.push(SetAside {
            owner: row
                .field_str("owner")
                .unwrap_or_else(|| LOCAL_USER.to_string()),
            release_group,
            title: row.field_str("title").unwrap_or_default(),
            created_at: row.field_u64("created_at").unwrap_or(0),
        });
    }
    Ok(data)
}

/// What a merge did, so that an import can say it rather than be trusted.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Merge {
    /// Records that did not exist here.
    pub added: usize,
    /// Records replaced because the incoming one was written later.
    pub updated: usize,
    /// Records left alone because what is here is newer.
    pub kept: usize,
    /// Listening events that were not already in the log.
    pub plays: usize,
    /// Saved queries taken in.
    pub collections: usize,
}

/// Folds one set of user data into another.
///
/// **Merging, never replacing.** Someone restoring half a backup wants their
/// two halves, not the older one; and an import that emptied what was already
/// there would be the one operation in this program capable of losing
/// everything at once.
///
/// The rule is the same everywhere: the record written **last** wins, and the
/// one that lost is reported rather than dropped in silence. Play counters take
/// the larger of the two, since a count is a total and neither side ever
/// counted the other's listens.
pub fn merge(into: &mut UserData, incoming: UserData) -> Merge {
    let mut report = Merge::default();

    for annotation in incoming.annotations {
        match into
            .annotations
            .iter_mut()
            .find(|a| a.owner == annotation.owner && a.target == annotation.target)
        {
            Some(existing) if existing.updated_at >= annotation.updated_at => report.kept += 1,
            Some(existing) => {
                *existing = annotation;
                report.updated += 1;
            }
            None => {
                into.annotations.push(annotation);
                report.added += 1;
            }
        }
    }

    // An event is identified by who, what and when: importing the same backup
    // twice must not double anybody's history.
    for play in incoming.plays {
        let known = into
            .plays
            .iter()
            .any(|p| p.owner == play.owner && p.track == play.track && p.at == play.at);
        if !known {
            into.plays.push(play);
            report.plays += 1;
        }
    }
    into.plays.sort_by_key(|p| p.at);
    if into.plays.len() > HISTORY_LIMIT {
        let excess = into.plays.len() - HISTORY_LIMIT;
        into.plays.drain(..excess);
    }

    for count in incoming.counts {
        match into
            .counts
            .iter_mut()
            .find(|c| c.owner == count.owner && c.track == count.track)
        {
            Some(existing) => {
                existing.count = existing.count.max(count.count);
                existing.last_played = existing.last_played.max(count.last_played);
            }
            None => into.counts.push(count),
        }
    }

    for collection in incoming.collections {
        let owner = collection.owner.clone();
        match into.collections.iter_mut().find(|c| {
            c.owner == owner && text::normalize(&c.name) == text::normalize(&collection.name)
        }) {
            Some(existing) if existing.updated_at >= collection.updated_at => report.kept += 1,
            Some(existing) => {
                *existing = collection;
                report.collections += 1;
            }
            None => {
                into.collections.push(collection);
                report.collections += 1;
            }
        }
    }
    // A decision has no versions to arbitrate: it was taken or it was not, so
    // importing the same backup twice must not file it twice.
    for aside in incoming.set_aside {
        let known = into
            .set_aside
            .iter()
            .any(|a| a.owner == aside.owner && a.release_group == aside.release_group);
        if !known {
            into.set_aside.push(aside);
            report.added += 1;
        }
    }
    report
}

#[cfg(test)]
#[path = "user_tests.rs"]
mod tests;
