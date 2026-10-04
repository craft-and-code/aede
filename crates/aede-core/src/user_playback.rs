//! Owner-private native-player checkpoints in the optional personal-store table.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::Metadata;

use super::{EntityRef, Merge, UserData, UserRef};
use crate::json::Json;
use crate::model::{AudioFile, Catalog, EntityKind};
use crate::playback::normalization::Mode;
use crate::store::StoreError;

/// Maximum native-player checkpoints in one personal store.
pub const PLAYBACK_STATE_LIMIT: usize = 512;
/// Maximum named player profiles belonging to one immutable owner.
pub const PLAYBACK_PROFILE_LIMIT: usize = 16;
/// Maximum ordered occurrences retained by one player profile.
pub const PLAYBACK_ENTRY_LIMIT: usize = 64;

const MAX_EXACT_INTEGER: u64 = (1u64 << 53) - 1;
const MAX_DATE: u64 = 253_402_300_799;

/// File evidence captured when selecting a resumable source.
///
/// This is a size/timestamp identity guard, not a checksum. No relocation is
/// inferred: the accompanying track reference must still name the same path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackSource {
    /// Original encoded-file size in bytes.
    pub size: u64,
    /// Modification timestamp, whole Unix seconds.
    pub mtime: u64,
    /// Nanosecond fraction, when the source/catalog supplies precise evidence.
    pub mtime_ns: Option<u32>,
}

impl PlaybackSource {
    /// Capture catalog evidence without reading or changing an audio file.
    pub fn from_catalog(catalog: &Catalog, file: &AudioFile) -> Self {
        Self {
            size: file.size,
            mtime: file.mtime,
            mtime_ns: catalog.file_mtime_subseconds.get(&file.path).copied(),
        }
    }

    /// Capture precise evidence from an already opened source descriptor.
    ///
    /// Unavailable or pre-epoch timestamps are refused rather than turned into
    /// an ambiguous zero identity. The caller must open/validate the source.
    pub fn from_metadata(metadata: &Metadata) -> Result<Self, StoreError> {
        if !metadata.is_file() {
            return Err(StoreError::Invalid(
                "playback source must be a regular file",
            ));
        }
        let modified = metadata
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| StoreError::Invalid("playback source timestamp predates the epoch"))?;
        let source = Self {
            size: metadata.len(),
            mtime: modified.as_secs(),
            mtime_ns: Some(modified.subsec_nanos()),
        };
        if !source.valid() {
            return Err(StoreError::Invalid("invalid playback source identity"));
        }
        Ok(source)
    }

    /// Whether the same current catalog row still carries matching evidence.
    pub fn matches_catalog(&self, catalog: &Catalog, file: &AudioFile) -> bool {
        self.valid()
            && self.size == file.size
            && self.mtime == file.mtime
            && self.mtime_ns.is_none_or(|fraction| {
                catalog.file_mtime_subseconds.get(&file.path) == Some(&fraction)
            })
    }

    /// Recheck the already opened file before starting a resumed decode.
    pub fn matches_metadata(&self, metadata: &Metadata) -> bool {
        Self::from_metadata(metadata).is_ok_and(|current| {
            self.valid()
                && self.size == current.size
                && self.mtime == current.mtime
                && self
                    .mtime_ns
                    .is_none_or(|fraction| current.mtime_ns == Some(fraction))
        })
    }

    fn valid(&self) -> bool {
        self.size <= MAX_EXACT_INTEGER
            && self.mtime <= MAX_DATE
            && self
                .mtime_ns
                .is_none_or(|fraction| fraction < 1_000_000_000)
    }
}

/// One stable queue occurrence; repeating a track uses another occurrence ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackEntry {
    /// Positive, exact-JSON integer identifying this occurrence within its profile.
    pub occurrence: u64,
    /// Path-based track reference, independent of renumbered catalog IDs.
    pub track: EntityRef,
    /// Evidence that prevents resuming a changed source as the old occurrence.
    pub source: PlaybackSource,
}

/// Processing settings restored explicitly with a native-player profile.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSettings {
    /// Loudness scope; these settings never modify encoded source files.
    pub normalize: Mode,
    /// Requested PCM output rate, or the source-selected default.
    pub sample_rate: Option<u32>,
    /// Broad bass shelf in decibels, -12 through +12.
    pub bass_db: f32,
    /// Broad treble shelf in decibels, -12 through +12.
    pub treble_db: f32,
}

impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            normalize: Mode::Track,
            sample_rate: None,
            bass_db: 0.0,
            treble_db: 0.0,
        }
    }
}

/// One private named checkpoint, resumed only by an explicit client request.
///
/// Persistence does not create a listen or restart playback. The server owns
/// authorization, writer locking and compare-before-write of session/revision;
/// a stale socket must not overwrite a newer session with the same profile.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackState {
    /// Immutable account owner; mutable account login names are never used.
    pub owner: UserRef,
    /// Case-sensitive 1–64 ASCII letters, digits, underscores or hyphens.
    pub profile: String,
    /// Opaque server-created session identity, independent of queue contents.
    pub session_id: String,
    /// Positive exact-JSON revision used to reject stale updates.
    pub revision: u64,
    /// Finite ordered selection, including intentional repeated tracks.
    pub entries: Vec<PlaybackEntry>,
    /// Selected occurrence, or no selection after natural queue completion.
    pub current_occurrence: Option<u64>,
    /// Acknowledged source position for the selected occurrence, in milliseconds.
    pub position_ms: u64,
    /// Processing settings for a deliberate resume.
    pub settings: PlaybackSettings,
    /// Last successful checkpoint publication, Unix seconds.
    pub updated_at: u64,
}

fn identifier_valid(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Whether a native-player profile uses the bounded case-sensitive spelling.
pub fn valid_playback_profile(profile: &str) -> bool {
    identifier_valid(profile, 64)
}

fn state_valid(state: &PlaybackState) -> bool {
    let mut occurrences = BTreeSet::new();
    !state.owner.trim().is_empty()
        && state.owner.len() <= 512
        && !state.owner.contains('\0')
        && valid_playback_profile(&state.profile)
        && identifier_valid(&state.session_id, 128)
        && (1..=MAX_EXACT_INTEGER).contains(&state.revision)
        && state.entries.len() <= PLAYBACK_ENTRY_LIMIT
        && state.entries.iter().all(|entry| {
            (1..=MAX_EXACT_INTEGER).contains(&entry.occurrence)
                && occurrences.insert(entry.occurrence)
                && entry.track.kind == EntityKind::Track
                && !entry.track.key.trim().is_empty()
                && entry.track.key.len() <= 16384
                && !entry.track.key.contains('\0')
                && entry.source.valid()
        })
        && state
            .current_occurrence
            .is_none_or(|occurrence| occurrences.contains(&occurrence))
        && (state.current_occurrence.is_some() || state.position_ms == 0)
        && state.position_ms <= MAX_EXACT_INTEGER
        && state.updated_at <= MAX_DATE
        && state
            .settings
            .sample_rate
            .is_none_or(|rate| (8000..=192_000).contains(&rate))
        && state.settings.bass_db.is_finite()
        && (-12.0..=12.0).contains(&state.settings.bass_db)
        && state.settings.treble_db.is_finite()
        && (-12.0..=12.0).contains(&state.settings.treble_db)
}

pub(super) fn validate(data: &UserData) -> Result<(), StoreError> {
    if data.playback_states.len() > PLAYBACK_STATE_LIMIT {
        return Err(StoreError::Invalid(
            "personal playback state limit exceeded",
        ));
    }
    let mut owners = BTreeMap::new();
    let mut profiles = BTreeSet::new();
    for state in &data.playback_states {
        let count = owners.entry(&state.owner).or_insert(0usize);
        *count += 1;
        if *count > PLAYBACK_PROFILE_LIMIT
            || !profiles.insert((&state.owner, &state.profile))
            || !state_valid(state)
        {
            return Err(StoreError::Invalid(
                "invalid, duplicate or oversized personal playback state",
            ));
        }
    }
    Ok(())
}

impl UserData {
    /// Read only one owner's named checkpoint; another owner cannot select it.
    pub fn playback_state(&self, owner: &str, profile: &str) -> Option<&PlaybackState> {
        self.playback_states
            .iter()
            .find(|state| state.owner == owner && state.profile == profile)
    }

    /// Add or replace exactly one owner's named checkpoint, validating before mutation.
    ///
    /// Callers must authorize the supplied owner and compare the existing
    /// session/revision while holding the installation's writer lock. This
    /// domain operation does not implicitly acquire a second lock or save.
    pub fn set_playback_state(&mut self, state: PlaybackState) -> Result<(), StoreError> {
        validate(self)?;
        if !state_valid(&state) {
            return Err(StoreError::Invalid("invalid personal playback state"));
        }
        if let Some(existing) = self
            .playback_states
            .iter_mut()
            .find(|existing| existing.owner == state.owner && existing.profile == state.profile)
        {
            *existing = state;
        } else {
            if self.playback_states.len() >= PLAYBACK_STATE_LIMIT
                || self
                    .playback_states
                    .iter()
                    .filter(|existing| existing.owner == state.owner)
                    .count()
                    >= PLAYBACK_PROFILE_LIMIT
            {
                return Err(StoreError::Invalid(
                    "personal playback state limit exceeded",
                ));
            }
            self.playback_states.push(state);
        }
        Ok(())
    }

    /// Forget one profile for this owner, leaving other owners/profiles untouched.
    pub fn clear_playback_state(&mut self, owner: &str, profile: &str) -> bool {
        let previous = self.playback_states.len();
        self.playback_states
            .retain(|state| state.owner != owner || state.profile != profile);
        self.playback_states.len() != previous
    }
}

pub(super) fn write_table(data: &UserData, root: &mut Json) {
    if data.playback_states.is_empty() {
        return;
    }
    root.set(
        "playback_states",
        Json::Arr(data.playback_states.iter().map(to_json).collect()),
    );
}

fn to_json(state: &PlaybackState) -> Json {
    let mut row = Json::obj();
    row.set("owner", state.owner.as_str().into());
    row.set("profile", state.profile.as_str().into());
    row.set("session_id", state.session_id.as_str().into());
    row.set("revision", state.revision.into());
    row.set("updated_at", state.updated_at.into());
    row.set("position_ms", state.position_ms.into());
    if let Some(occurrence) = state.current_occurrence {
        row.set("current_occurrence", occurrence.into());
    }
    let mut settings = Json::obj();
    settings.set(
        "normalize",
        match state.settings.normalize {
            Mode::Off => "off",
            Mode::Track => "track",
            Mode::Album => "album",
        }
        .into(),
    );
    if let Some(rate) = state.settings.sample_rate {
        settings.set("sample_rate", rate.into());
    }
    settings.set("bass_db", f64::from(state.settings.bass_db).into());
    settings.set("treble_db", f64::from(state.settings.treble_db).into());
    row.set("settings", settings);
    row.set(
        "entries",
        Json::Arr(
            state
                .entries
                .iter()
                .map(|entry| {
                    let mut item = Json::obj();
                    item.set("occurrence", entry.occurrence.into());
                    item.set("track", entry.track.to_token().into());
                    item.set("size", entry.source.size.into());
                    item.set("mtime", entry.source.mtime.into());
                    if let Some(fraction) = entry.source.mtime_ns {
                        item.set("mtime_ns", fraction.into());
                    }
                    item
                })
                .collect(),
        ),
    );
    row
}

fn required_text(row: &Json, name: &str) -> Result<String, StoreError> {
    row.field_str(name).ok_or(StoreError::Invalid(
        "missing or malformed playback state field",
    ))
}

fn required_number(row: &Json, name: &str) -> Result<u64, StoreError> {
    row.field_u64(name).ok_or(StoreError::Invalid(
        "missing or malformed playback state number",
    ))
}

fn optional_number(row: &Json, name: &str) -> Result<Option<u64>, StoreError> {
    match row.get(name) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or(StoreError::Invalid(
            "malformed optional playback state number",
        )),
    }
}

fn optional_u32(row: &Json, name: &str) -> Result<Option<u32>, StoreError> {
    optional_number(row, name)?
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| StoreError::Invalid("playback state number exceeds u32"))
        })
        .transpose()
}

pub(super) fn read_states(root: &Json) -> Result<Vec<PlaybackState>, StoreError> {
    let rows = root
        .get("playback_states")
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    if rows.len() > PLAYBACK_STATE_LIMIT {
        return Err(StoreError::Invalid(
            "personal playback state limit exceeded",
        ));
    }
    let mut data = UserData::default();
    for row in rows {
        let settings = row
            .get("settings")
            .ok_or(StoreError::Invalid("missing playback settings"))?;
        let normalize = match settings.get("normalize").and_then(Json::as_str) {
            Some("off") => Mode::Off,
            Some("track") => Mode::Track,
            Some("album") => Mode::Album,
            _ => return Err(StoreError::Invalid("invalid playback normalization scope")),
        };
        let tone = |name| {
            settings
                .field_f64(name)
                .filter(|value| value.is_finite() && (-12.0..=12.0).contains(value))
                .map(|value| value as f32)
                .ok_or(StoreError::Invalid("invalid playback tone setting"))
        };
        let entries = row
            .get("entries")
            .and_then(Json::as_arr)
            .filter(|entries| entries.len() <= PLAYBACK_ENTRY_LIMIT)
            .ok_or(StoreError::Invalid("invalid personal playback entries"))?
            .iter()
            .map(|entry| {
                Ok(PlaybackEntry {
                    occurrence: required_number(entry, "occurrence")?,
                    track: EntityRef::parse_token(&required_text(entry, "track")?)
                        .ok_or(StoreError::Invalid("invalid playback track reference"))?,
                    source: PlaybackSource {
                        size: required_number(entry, "size")?,
                        mtime: required_number(entry, "mtime")?,
                        mtime_ns: optional_u32(entry, "mtime_ns")?,
                    },
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        data.playback_states.push(PlaybackState {
            owner: required_text(row, "owner")?,
            profile: required_text(row, "profile")?,
            session_id: required_text(row, "session_id")?,
            revision: required_number(row, "revision")?,
            entries,
            current_occurrence: optional_number(row, "current_occurrence")?,
            position_ms: required_number(row, "position_ms")?,
            settings: PlaybackSettings {
                normalize,
                sample_rate: optional_u32(settings, "sample_rate")?,
                bass_db: tone("bass_db")?,
                treble_db: tone("treble_db")?,
            },
            updated_at: required_number(row, "updated_at")?,
        });
    }
    validate(&data)?;
    Ok(data.playback_states)
}

pub(super) fn merge(into: &mut UserData, incoming: Vec<PlaybackState>, report: &mut Merge) {
    // An import must not rewind a current device or resurrect an older queue.
    // Full backup restoration still replaces this nested store explicitly.
    for state in incoming {
        if into.playback_state(&state.owner, &state.profile).is_some() {
            report.kept += 1;
        } else {
            into.playback_states.push(state);
            report.added += 1;
        }
    }
}

#[cfg(test)]
#[path = "user_playback_tests.rs"]
mod tests;
