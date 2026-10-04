//! Owner-scoped translations over the existing locked personal store.

use std::collections::{BTreeMap, BTreeSet};

use aede_core::accounts::Role;
use aede_core::user::{self, Annotation, Playlist, Scrobble, UserData};

use super::authentication::Identity;
use super::catalog::{ArtistScope, Index};
use super::*;

fn failure(message: &str) -> ProtocolError {
    ProtocolError::new(0, message)
}
fn invalid(message: &str) -> ProtocolError {
    ProtocolError::new(10, message)
}
fn missing() -> ProtocolError {
    ProtocolError::new(70, "playlist not found")
}

fn mutates(method: &str) -> bool {
    !matches!(method, "getStarred2" | "getPlaylists" | "getPlaylist")
}

/// All personal selection and mutation uses one completed on-disk catalog
/// under the writer lock, with permissions rechecked before reading/saving.
pub(super) async fn execute(
    state: ApiState,
    identity: Identity,
    method: &str,
    parameters: Parameters,
) -> Result<Value, ProtocolError> {
    locked(state, identity, method, parameters, false).await
}

/// Projects catalog metadata with this owner's favourites, ratings and counts.
/// No private field is added to the shared catalog or its public native API.
pub(super) async fn catalog(
    state: ApiState,
    identity: Identity,
    method: &str,
    parameters: Parameters,
) -> Result<Value, ProtocolError> {
    super::catalog::catalog_parameters(method, &parameters)?;
    locked(state, identity, method, parameters, true).await
}

async fn locked(
    state: ApiState,
    identity: Identity,
    method: &str,
    parameters: Parameters,
    catalog_read: bool,
) -> Result<Value, ProtocolError> {
    let mutation = !catalog_read && mutates(method);
    if mutation && identity.role == Role::Auditor {
        return Err(ProtocolError::new(
            50,
            "auditor accounts cannot change personal data",
        ));
    }
    let permit = state
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| failure("personal work is busy; retry shortly"))?;
    let method = method.to_string();
    tokio::task::spawn_blocking(move || {
        // The permit remains owned by the worker after an HTTP disconnection.
        let _permit = permit;
        let _lock = StoreLock::try_acquire(&state.data_dir)
            .map_err(|_| failure("personal store is busy or unavailable"))?;
        authentication::recheck_blocking(&state, &identity)?;
        let catalog = store::load(&store::catalog_path(&state.data_dir))
            .map_err(|_| failure("current catalog could not be read"))?
            .ok_or_else(|| failure("catalog unavailable"))?;
        let mut data = user::load(&user::user_path(&state.data_dir))
            .map_err(|_| failure("personal data could not be read"))?
            .unwrap_or_default();
        user::reconcile(&mut data, &catalog);
        let index = Index::new(&catalog)?;
        let now = aede_core::clock::now_seconds();
        let result = if catalog_read {
            let mut result = super::catalog::dispatch_index(&method, &parameters, &index)?;
            PrivateView::new(&data, &identity, &parameters)
                .decorate_payload(&index, &mut result)?;
            result
        } else {
            dispatch(&method, &parameters, &index, &identity, &mut data, now)?
        };
        authentication::recheck_blocking(&state, &identity)?;
        if mutation {
            data.forget_empty();
            user::save(&data, &user::user_path(&state.data_dir))
                .map_err(|_| failure("personal data could not be saved"))?;
        }
        Ok(result)
    })
    .await
    .map_err(|_| failure("personal work failed"))?
}

fn folder(parameters: &Parameters) -> Result<(), ProtocolError> {
    if parameters
        .get("musicFolderId")
        .is_some_and(|value| value != "1")
    {
        return Err(ProtocolError::new(70, "music folder not found"));
    }
    Ok(())
}

struct PrivateView<'a> {
    annotations: BTreeMap<&'a EntityRef, &'a Annotation>,
    counts: BTreeMap<&'a EntityRef, u32>,
    artist_scope: ArtistScope,
}

impl<'a> PrivateView<'a> {
    fn new(data: &'a UserData, identity: &Identity, parameters: &Parameters) -> Self {
        let mut annotations = BTreeMap::new();
        let mut counts = BTreeMap::new();
        // Match the core's first-record lookup even for a legacy duplicate.
        for row in data
            .annotations
            .iter()
            .filter(|row| row.owner == identity.owner)
        {
            annotations.entry(&row.target).or_insert(row);
        }
        for row in data.counts.iter().filter(|row| row.owner == identity.owner) {
            counts.entry(&row.track).or_insert(row.count);
        }
        Self {
            annotations,
            counts,
            artist_scope: ArtistScope::for_client(parameters),
        }
    }

    fn render(&self, index: &Index<'_>, reference: &EntityRef) -> Result<Value, ProtocolError> {
        let mut value = index.render_reference_scoped(reference, self.artist_scope)?;
        self.decorate(reference, &mut value)?;
        Ok(value)
    }

    fn decorate(&self, reference: &EntityRef, value: &mut Value) -> Result<(), ProtocolError> {
        if let Some(annotation) = self.annotations.get(reference) {
            if annotation.loved {
                value["starred"] = utc_date(annotation.updated_at)?.into();
            }
            if let Some(rating) = annotation.rating {
                value["userRating"] = rating.into();
            }
        }
        if reference.kind == EntityKind::Track {
            value["playCount"] = self.counts.get(reference).copied().unwrap_or(0).into();
        }
        Ok(())
    }

    fn decorate_payload(&self, index: &Index<'_>, value: &mut Value) -> Result<(), ProtocolError> {
        if let Some(id) = value.get("id").and_then(Value::as_str).filter(|id| {
            ["track-", "release-", "artist-"]
                .iter()
                .any(|prefix| id.starts_with(prefix))
        }) {
            let reference = index.reference(
                id,
                &[EntityKind::Track, EntityKind::Release, EntityKind::Artist],
            )?;
            self.decorate(&reference, value)?;
        }
        // Only the shallow, server-built protocol tree is traversed here. Tag
        // text is always scalar, so user input cannot create recursion depth.
        match value {
            Value::Object(object) => {
                for value in object.values_mut() {
                    self.decorate_payload(index, value)?;
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.decorate_payload(index, value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn playlist_view(
    index: &Index<'_>,
    view: &PrivateView<'_>,
    identity: &Identity,
    playlist: &Playlist,
    entries: bool,
) -> Result<Value, ProtocolError> {
    let mut songs = Vec::with_capacity(if entries { playlist.tracks.len() } else { 0 });
    let mut duration = 0u64;
    for reference in &playlist.tracks {
        // A missing file keeps its reference in the store and fails explicitly.
        if entries {
            let song = view.render(index, reference)?;
            duration =
                duration.saturating_add(song.get("duration").and_then(Value::as_u64).unwrap_or(0));
            songs.push(song);
        } else {
            duration = duration.saturating_add(index.track_duration(reference)?.unwrap_or(0));
        }
    }
    let mut value = json!({"id": playlist.id, "name": playlist.name, "owner": identity.username,
        "public": false, "songCount": playlist.tracks.len(), "duration": duration,
        "created": utc_date(playlist.created_at)?, "changed": utc_date(playlist.updated_at)?});
    if let Some(comment) = &playlist.comment {
        value["comment"] = comment.clone().into();
    }
    if entries {
        value["entry"] = songs.into();
    }
    Ok(value)
}

fn tracks(index: &Index<'_>, values: &[String]) -> Result<Vec<EntityRef>, ProtocolError> {
    values
        .iter()
        .map(|id| index.reference(id, &[EntityKind::Track]))
        .collect()
}

fn unsigned(value: &str, name: &str) -> Result<u64, ProtocolError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(&format!("{name} must be a non-negative integer")));
    }
    value
        .parse()
        .map_err(|_| invalid(&format!("{name} is out of range")))
}

fn dispatch(
    method: &str,
    parameters: &Parameters,
    index: &Index<'_>,
    identity: &Identity,
    data: &mut UserData,
    now: u64,
) -> Result<Value, ProtocolError> {
    match method {
        "getStarred2" => {
            parameters.allowed(&["musicFolderId"])?;
            folder(parameters)?;
            let view = PrivateView::new(data, identity, parameters);
            let mut artists = Vec::new();
            let mut albums = Vec::new();
            let mut songs = Vec::new();
            for annotation in data
                .annotations
                .iter()
                .filter(|a| a.owner == identity.owner && a.loved)
            {
                if !matches!(
                    annotation.target.kind,
                    EntityKind::Artist | EntityKind::Release | EntityKind::Track
                ) {
                    continue;
                }
                let value = match view.render(index, &annotation.target) {
                    Ok(value) => value,
                    Err(error) if error.code == 70 => continue,
                    Err(error) => return Err(error),
                };
                match annotation.target.kind {
                    EntityKind::Artist => artists.push(value),
                    EntityKind::Release => albums.push(value),
                    _ => songs.push(value),
                }
            }
            Ok(json!({"starred2": {"artist": artists, "album": albums, "song": songs}}))
        }
        "star" | "unstar" => {
            parameters.allowed(&["id", "albumId", "artistId"])?;
            let mut targets = BTreeSet::new();
            for (name, kinds) in [
                (
                    "id",
                    &[EntityKind::Track, EntityKind::Release, EntityKind::Artist][..],
                ),
                ("albumId", &[EntityKind::Release][..]),
                ("artistId", &[EntityKind::Artist][..]),
            ] {
                for id in parameters.values(name) {
                    targets.insert(index.reference(id, kinds)?);
                }
            }
            if targets.is_empty() {
                return Err(invalid("at least one favourite target is required"));
            }
            for reference in targets {
                let annotation = data.entry(&identity.owner, &reference, now);
                annotation.loved = method == "star";
                annotation.updated_at = now;
            }
            Ok(json!({}))
        }
        "setRating" => {
            parameters.allowed(&["id", "rating"])?;
            let reference = index.reference(
                parameters.required("id")?,
                &[EntityKind::Track, EntityKind::Release, EntityKind::Artist],
            )?;
            let rating = parameters.number("rating", 0, 5)?;
            parameters.required("rating")?;
            let annotation = data.entry(&identity.owner, &reference, now);
            annotation.rating = (rating != 0).then_some(rating as u8);
            annotation.updated_at = now;
            Ok(json!({}))
        }
        "getPlaylists" => {
            parameters.allowed(&["username"])?;
            if parameters
                .get("username")
                .is_some_and(|username| !username.eq_ignore_ascii_case(&identity.username))
            {
                return Err(ProtocolError::new(
                    50,
                    "only your own playlists are available",
                ));
            }
            let view = PrivateView::new(data, identity, parameters);
            let playlists = data
                .playlists
                .iter()
                .filter(|p| p.owner == identity.owner)
                .map(|playlist| playlist_view(index, &view, identity, playlist, false))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"playlists": {"playlist": playlists}}))
        }
        "getPlaylist" => {
            parameters.allowed(&["id"])?;
            let playlist = data
                .playlist(&identity.owner, parameters.required("id")?)
                .ok_or_else(missing)?;
            Ok(
                json!({"playlist": playlist_view(index, &PrivateView::new(data, identity, parameters), identity, playlist, true)?}),
            )
        }
        "createPlaylist" => {
            parameters.allowed(&["playlistId", "name", "songId"])?;
            let tracks = tracks(index, parameters.values("songId"))?;
            let id = if let Some(id) = parameters.get("playlistId") {
                data.playlist(&identity.owner, id).ok_or_else(missing)?;
                data.update_playlist(
                    &identity.owner,
                    id,
                    parameters.get("name"),
                    None,
                    Some(tracks),
                    now,
                )
                .map_err(|error| invalid(&error))?;
                id.to_string()
            } else {
                data.create_playlist(&identity.owner, parameters.required("name")?, tracks, now)
                    .map_err(|error| invalid(&error))?
            };
            let playlist = data.playlist(&identity.owner, &id).ok_or_else(missing)?;
            Ok(
                json!({"playlist": playlist_view(index, &PrivateView::new(data, identity, parameters), identity, playlist, true)?}),
            )
        }
        "updatePlaylist" => {
            parameters.allowed(&[
                "playlistId",
                "name",
                "comment",
                "public",
                "songIdToAdd",
                "songIndexToRemove",
            ])?;
            if parameters
                .get("public")
                .is_some_and(|value| value != "false")
            {
                return Err(invalid("playlists are private; public must be false"));
            }
            let id = parameters.required("playlistId")?;
            let existing = data.playlist(&identity.owner, id).ok_or_else(missing)?;
            let mut remove = BTreeSet::new();
            for value in parameters.values("songIndexToRemove") {
                let position = usize::try_from(unsigned(value, "songIndexToRemove")?)
                    .map_err(|_| invalid("playlist index out of range"))?;
                if position >= existing.tracks.len() || !remove.insert(position) {
                    return Err(invalid("invalid or duplicate playlist removal index"));
                }
            }
            let mut replacement: Vec<_> = existing
                .tracks
                .iter()
                .enumerate()
                .filter(|(position, _)| !remove.contains(position))
                .map(|(_, track)| track.clone())
                .collect();
            replacement.extend(tracks(index, parameters.values("songIdToAdd"))?);
            data.update_playlist(
                &identity.owner,
                id,
                parameters.get("name"),
                parameters.get("comment"),
                Some(replacement),
                now,
            )
            .map_err(|error| invalid(&error))?;
            Ok(json!({}))
        }
        "deletePlaylist" => {
            parameters.allowed(&["id"])?;
            if !data.delete_playlist(&identity.owner, parameters.required("id")?) {
                return Err(missing());
            }
            Ok(json!({}))
        }
        "scrobble" => {
            parameters.allowed(&["id", "time", "submission"])?;
            if parameters
                .get("submission")
                .is_some_and(|value| value != "true")
            {
                return Err(invalid("submission must be true for a stored scrobble"));
            }
            let references = tracks(index, parameters.values("id"))?;
            if references.is_empty() {
                return Err(invalid("at least one scrobble id is required"));
            }
            let times = parameters.values("time");
            if !times.is_empty() && times.len() != references.len() {
                return Err(invalid("time and id must have the same number of values"));
            }
            let default_ms = now.saturating_mul(1000);
            let timestamps = if times.is_empty() {
                vec![default_ms; references.len()]
            } else {
                times
                    .iter()
                    .map(|value| unsigned(value, "time"))
                    .collect::<Result<Vec<_>, _>>()?
            };
            if timestamps
                .iter()
                .any(|time| *time > default_ms.saturating_add(86_400_000))
            {
                return Err(invalid(
                    "scrobble time must not be over one day in the future",
                ));
            }
            for (track, at_ms) in references.into_iter().zip(timestamps) {
                data.record_scrobble(Scrobble {
                    owner: identity.owner.clone(),
                    track,
                    at_ms,
                })
                .map_err(|error| invalid(&error))?;
            }
            Ok(json!({}))
        }
        _ => Err(failure("personal method is not implemented")),
    }
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
