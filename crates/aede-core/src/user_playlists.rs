//! Ordered private playlists and client-declared listens in the shared user store.

use super::*;
use crate::json::Json;
use crate::store::StoreError;

/// Maximum static playlists in one personal store.
pub const PLAYLIST_LIMIT: usize = 512;
/// Maximum static playlists belonging to one owner.
pub const PLAYLIST_OWNER_LIMIT: usize = 128;
/// Maximum ordered entries in one static playlist, including repetitions.
pub const PLAYLIST_TRACK_LIMIT: usize = 2000;

/// One owner's ordered, private selection, independent of catalog identifiers.
///
/// Repeated tracks and references to temporarily unavailable files are retained.
/// There is no sharing flag: selecting another owner never grants access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    /// Opaque persistent identifier, generated independently of its contents.
    pub id: String,
    /// Immutable personal owner, not a mutable login name.
    pub owner: UserRef,
    /// Human-readable name, 1–256 UTF-8 bytes containing non-whitespace.
    pub name: String,
    /// Optional description, at most 2048 UTF-8 bytes.
    pub comment: Option<String>,
    /// Stable track references in playback order; repetitions are meaningful.
    pub tracks: Vec<EntityRef>,
    /// Unix seconds when the playlist was created.
    pub created_at: u64,
    /// Unix seconds when its content or description last changed.
    pub updated_at: u64,
}

/// A client declares that a track was listened to, without playback evidence.
///
/// Unlike [`Play`], this does not claim a duration, completion, decoded audio,
/// or acknowledgement. Subsonic scrobble submissions supply only the track and
/// a timestamp. Each accepted submission increments the all-time counter;
/// repeating a request can therefore double-count a listen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrobble {
    /// Immutable owner whose client submitted the declaration.
    pub owner: UserRef,
    /// Stable reference to the declared track.
    pub track: EntityRef,
    /// Unix milliseconds supplied by the client, or receipt time when omitted.
    pub at_ms: u64,
}

const MAX_DATE: u64 = 253_402_300_799;

fn text_valid(value: &str, limit: usize, nonblank: bool) -> bool {
    value.len() <= limit && !value.contains('\0') && (!nonblank || !value.trim().is_empty())
}

fn track_valid(track: &EntityRef) -> bool {
    track.kind == EntityKind::Track && text_valid(&track.key, 16384, true)
}

fn playlist_valid(playlist: &Playlist) -> bool {
    playlist.id.strip_prefix("playlist-").is_some_and(|id| {
        id.len() == 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) && text_valid(&playlist.owner, 512, true)
        && text_valid(&playlist.name, 256, true)
        && playlist
            .comment
            .as_deref()
            .is_none_or(|text| text_valid(text, 2048, false))
        && playlist.tracks.len() <= PLAYLIST_TRACK_LIMIT
        && playlist.tracks.iter().all(track_valid)
        && playlist.created_at <= playlist.updated_at
        && playlist.updated_at <= MAX_DATE
}

pub(super) fn validate(data: &UserData) -> Result<(), StoreError> {
    let mut owners = BTreeMap::new();
    let mut ids = BTreeSet::new();
    if data.playlists.len() > PLAYLIST_LIMIT || data.scrobbles.len() > HISTORY_LIMIT {
        return Err(StoreError::Invalid(
            "personal playlist or scrobble limit exceeded",
        ));
    }
    for playlist in &data.playlists {
        let count = owners.entry(&playlist.owner).or_insert(0usize);
        *count += 1;
        if *count > PLAYLIST_OWNER_LIMIT || !ids.insert(&playlist.id) || !playlist_valid(playlist) {
            return Err(StoreError::Invalid(
                "invalid, duplicate or oversized personal playlist",
            ));
        }
    }
    if data.scrobbles.iter().any(|event| {
        !text_valid(&event.owner, 512, true)
            || !track_valid(&event.track)
            || event.at_ms > MAX_DATE * 1000 + 999
    }) {
        return Err(StoreError::Invalid("invalid personal scrobble"));
    }
    Ok(())
}

impl UserData {
    /// Finds a playlist only within the supplied owner's private namespace.
    pub fn playlist(&self, owner: &str, id: &str) -> Option<&Playlist> {
        self.playlists
            .iter()
            .find(|playlist| playlist.owner == owner && playlist.id == id)
    }

    /// Creates a bounded static playlist and returns its persistent random ID.
    ///
    /// Invalid data or a reached store/owner limit fails before any mutation.
    pub fn create_playlist(
        &mut self,
        owner: &str,
        name: &str,
        tracks: Vec<EntityRef>,
        now: u64,
    ) -> Result<String, String> {
        if self.playlists.len() >= PLAYLIST_LIMIT
            || self
                .playlists
                .iter()
                .filter(|playlist| playlist.owner == owner)
                .count()
                >= PLAYLIST_OWNER_LIMIT
        {
            return Err("playlist limit reached".into());
        }
        let id = format!("playlist-{}", crate::accounts::random_token()?);
        let playlist = Playlist {
            id: id.clone(),
            owner: owner.into(),
            name: name.into(),
            comment: None,
            tracks,
            created_at: now,
            updated_at: now,
        };
        if !playlist_valid(&playlist) || self.playlists.iter().any(|known| known.id == id) {
            return Err("invalid playlist or identifier collision".into());
        }
        self.playlists.push(playlist);
        Ok(id)
    }

    /// Updates only the supplied fields of an owner's playlist atomically.
    ///
    /// `Some("")` clears the comment; `None` preserves it. A missing owner/ID
    /// pair and invalid replacement data fail without changing the playlist.
    pub fn update_playlist(
        &mut self,
        owner: &str,
        id: &str,
        name: Option<&str>,
        comment: Option<&str>,
        tracks: Option<Vec<EntityRef>>,
        now: u64,
    ) -> Result<(), String> {
        let existing = self
            .playlists
            .iter_mut()
            .find(|playlist| playlist.owner == owner && playlist.id == id)
            .ok_or_else(|| "playlist not found".to_string())?;
        let mut replacement = existing.clone();
        if let Some(name) = name {
            replacement.name = name.into();
        }
        if let Some(comment) = comment {
            replacement.comment = (!comment.is_empty()).then(|| comment.into());
        }
        if let Some(tracks) = tracks {
            replacement.tracks = tracks;
        }
        replacement.updated_at = now;
        if !playlist_valid(&replacement) {
            return Err("invalid playlist replacement".into());
        }
        *existing = replacement;
        Ok(())
    }

    /// Deliberately removes one owner's playlist, leaving all music intact.
    pub fn delete_playlist(&mut self, owner: &str, id: &str) -> bool {
        let before = self.playlists.len();
        self.playlists
            .retain(|playlist| playlist.owner != owner || playlist.id != id);
        self.playlists.len() != before
    }

    /// Records a declared listen and updates its count without fabricating a [`Play`].
    ///
    /// Recent declarations retain their multiplicity, ordered by client time,
    /// and are bounded by [`HISTORY_LIMIT`]. Counts outlive that recent log.
    pub fn record_scrobble(&mut self, event: Scrobble) -> Result<(), String> {
        if !text_valid(&event.owner, 512, true)
            || !track_valid(&event.track)
            || event.at_ms > MAX_DATE * 1000 + 999
        {
            return Err("invalid scrobble declaration".into());
        }
        let at = event.at_ms / 1000;
        match self
            .counts
            .iter_mut()
            .find(|count| count.owner == event.owner && count.track == event.track)
        {
            Some(count) => {
                count.count = count.count.saturating_add(1);
                count.last_played = count.last_played.max(at);
            }
            None => self.counts.push(PlayCount {
                owner: event.owner.clone(),
                track: event.track.clone(),
                count: 1,
                last_played: at,
            }),
        }
        self.scrobbles.push(event);
        self.scrobbles.sort_by_key(|event| event.at_ms);
        if self.scrobbles.len() > HISTORY_LIMIT {
            self.scrobbles.drain(..self.scrobbles.len() - HISTORY_LIMIT);
        }
        Ok(())
    }
}

pub(super) fn write_tables(data: &UserData, root: &mut Json) {
    if !data.playlists.is_empty() {
        root.set(
            "playlists",
            Json::Arr(
                data.playlists
                    .iter()
                    .map(|playlist| {
                        let mut row = Json::obj();
                        row.set("id", playlist.id.as_str().into());
                        row.set("owner", playlist.owner.as_str().into());
                        row.set("name", playlist.name.as_str().into());
                        if let Some(comment) = &playlist.comment {
                            row.set("comment", comment.as_str().into());
                        }
                        row.set(
                            "tracks",
                            Json::Arr(
                                playlist
                                    .tracks
                                    .iter()
                                    .map(|track| track.to_token().into())
                                    .collect(),
                            ),
                        );
                        row.set("created_at", playlist.created_at.into());
                        row.set("updated_at", playlist.updated_at.into());
                        row
                    })
                    .collect(),
            ),
        );
    }
    if !data.scrobbles.is_empty() {
        root.set(
            "scrobbles",
            Json::Arr(
                data.scrobbles
                    .iter()
                    .map(|event| {
                        let mut row = Json::obj();
                        row.set("owner", event.owner.as_str().into());
                        row.set("track", event.track.to_token().into());
                        row.set("at_ms", event.at_ms.into());
                        row
                    })
                    .collect(),
            ),
        );
    }
}

fn required_string(row: &Json, name: &str) -> Result<String, StoreError> {
    row.field_str(name).ok_or(StoreError::Invalid(
        "missing or malformed private playlist/scrobble field",
    ))
}

pub(super) fn read_playlists(root: &Json) -> Result<Vec<Playlist>, StoreError> {
    let rows = root.get("playlists").and_then(Json::as_arr).unwrap_or(&[]);
    if rows.len() > PLAYLIST_LIMIT {
        return Err(StoreError::Invalid("personal playlist limit exceeded"));
    }
    let mut data = UserData::default();
    for row in rows {
        let tracks = row
            .get("tracks")
            .and_then(Json::as_arr)
            .filter(|tracks| tracks.len() <= PLAYLIST_TRACK_LIMIT)
            .ok_or(StoreError::Invalid("invalid personal playlist entries"))?
            .iter()
            .map(|track| {
                track
                    .as_str()
                    .and_then(EntityRef::parse_token)
                    .ok_or(StoreError::Invalid("invalid personal playlist track"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let comment = match row.get("comment") {
            None => None,
            Some(Json::Str(text)) => Some(text.clone()),
            _ => return Err(StoreError::Invalid("invalid personal playlist comment")),
        };
        data.playlists.push(Playlist {
            id: required_string(row, "id")?,
            owner: required_string(row, "owner")?,
            name: required_string(row, "name")?,
            comment,
            tracks,
            created_at: row
                .field_u64("created_at")
                .ok_or(StoreError::Invalid("invalid playlist creation time"))?,
            updated_at: row
                .field_u64("updated_at")
                .ok_or(StoreError::Invalid("invalid playlist update time"))?,
        });
    }
    validate(&data)?;
    Ok(data.playlists)
}

pub(super) fn read_scrobbles(root: &Json) -> Result<Vec<Scrobble>, StoreError> {
    let rows = root.get("scrobbles").and_then(Json::as_arr).unwrap_or(&[]);
    if rows.len() > HISTORY_LIMIT {
        return Err(StoreError::Invalid("personal scrobble limit exceeded"));
    }
    let mut data = UserData::default();
    for row in rows {
        data.scrobbles.push(Scrobble {
            owner: required_string(row, "owner")?,
            track: EntityRef::parse_token(&required_string(row, "track")?)
                .ok_or(StoreError::Invalid("invalid scrobble track"))?,
            at_ms: row
                .field_u64("at_ms")
                .ok_or(StoreError::Invalid("invalid scrobble timestamp"))?,
        });
    }
    validate(&data)?;
    data.scrobbles.sort_by_key(|event| event.at_ms);
    Ok(data.scrobbles)
}

pub(super) fn merge(
    into: &mut UserData,
    playlists: Vec<Playlist>,
    scrobbles: Vec<Scrobble>,
    report: &mut Merge,
) {
    for playlist in playlists {
        match into
            .playlists
            .iter_mut()
            .find(|known| known.owner == playlist.owner && known.id == playlist.id)
        {
            Some(known) if known.updated_at >= playlist.updated_at => report.kept += 1,
            Some(known) => {
                *known = playlist;
                report.updated += 1;
            }
            None => {
                into.playlists.push(playlist);
                report.added += 1;
            }
        }
    }
    let mut available = BTreeMap::new();
    for event in &into.scrobbles {
        *available
            .entry((&event.owner, &event.track, event.at_ms))
            .or_insert(0usize) += 1;
    }
    // Own the keys before appending, since each submitted copy may be distinct.
    let mut available: BTreeMap<_, _> = available
        .into_iter()
        .map(|((owner, track, at), count)| ((owner.clone(), track.clone(), at), count))
        .collect();
    for event in scrobbles {
        let copies = available
            .entry((event.owner.clone(), event.track.clone(), event.at_ms))
            .or_default();
        if *copies > 0 {
            *copies -= 1;
        } else {
            into.scrobbles.push(event);
            report.plays += 1;
        }
    }
    into.scrobbles.sort_by_key(|event| event.at_ms);
    if into.scrobbles.len() > HISTORY_LIMIT {
        into.scrobbles.drain(..into.scrobbles.len() - HISTORY_LIMIT);
    }
}

#[cfg(test)]
#[path = "user_playlists_tests.rs"]
mod tests;
