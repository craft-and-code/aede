//! ID3 browsing projection over the catalog graph.
//!
//! Releases remain separate editions and tracks remain local placements. The
//! album/artist shape exists only in this compatibility response, never in the
//! core model. IDs hash stable entity references and no response exposes paths.

use super::{Parameters, ProtocolError, opaque_id, utc_date};
use aede_core::model::{Artist, Catalog, EntityKind, Id, Release, Track};
use aede_core::text;
use aede_core::user::EntityRef;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

const PAGE_LIMIT: usize = 500;
const SEARCH_LIMIT: usize = 1000;
const QUERY_LIMIT: usize = 1024;
const MUSIC_FOLDER_ID: &str = "1";

struct ArtistRow<'a> {
    entity: &'a Artist,
    id: String,
    sort: String,
    albums: Vec<Id>,
}

struct AlbumRow<'a> {
    entity: &'a Release,
    id: String,
    sort: String,
    genres: Vec<Id>,
    duration_ms: u64,
    cover: Option<String>,
}

struct SongRow<'a> {
    entity: &'a Track,
    id: String,
    sort: String,
    artists: Vec<Id>,
    genres: Vec<Id>,
}

/// One borrowed projection per catalog operation. Resolve opaque IDs and
/// credits once so listing a page does not repeatedly scan the whole graph.
pub(super) struct Index<'a> {
    catalog: &'a Catalog,
    artists: Vec<ArtistRow<'a>>,
    albums: Vec<AlbumRow<'a>>,
    songs: Vec<SongRow<'a>>,
    ids: HashMap<String, (EntityKind, Id)>,
    references: BTreeMap<EntityRef, Id>,
    created: String,
}

impl<'a> Index<'a> {
    pub(super) fn new(catalog: &'a Catalog) -> Result<Self, ProtocolError> {
        if catalog
            .artists
            .iter()
            .enumerate()
            .any(|(at, row)| row.id as usize != at)
            || catalog
                .releases
                .iter()
                .enumerate()
                .any(|(at, row)| row.id as usize != at)
            || catalog
                .tracks
                .iter()
                .enumerate()
                .any(|(at, row)| row.id as usize != at)
        {
            return Err(ProtocolError::new(0, "catalog identities are invalid"));
        }
        let mut index = Self {
            catalog,
            artists: catalog
                .artists
                .iter()
                .map(|artist| {
                    Ok(ArtistRow {
                        entity: artist,
                        id: opaque_id(catalog, EntityKind::Artist, artist.id)?,
                        sort: text::normalize(&artist.sort_name),
                        albums: Vec::new(),
                    })
                })
                .collect::<Result<_, ProtocolError>>()?,
            albums: catalog
                .releases
                .iter()
                .map(|release| {
                    Ok(AlbumRow {
                        entity: release,
                        id: opaque_id(catalog, EntityKind::Release, release.id)?,
                        sort: text::normalize(&release.title),
                        genres: Vec::new(),
                        cover: super::artwork::cover_id(catalog, release)?,
                        duration_ms: release
                            .track_ids
                            .iter()
                            .filter_map(|id| catalog.track(*id)?.duration_ms)
                            .fold(0, u64::saturating_add),
                    })
                })
                .collect::<Result<_, ProtocolError>>()?,
            songs: catalog
                .tracks
                .iter()
                .map(|track| {
                    Ok(SongRow {
                        entity: track,
                        id: opaque_id(catalog, EntityKind::Track, track.id)?,
                        sort: text::normalize(&track.title),
                        artists: Vec::new(),
                        genres: Vec::new(),
                    })
                })
                .collect::<Result<_, ProtocolError>>()?,
            ids: HashMap::new(),
            references: BTreeMap::new(),
            // The core stores the last scan time, not a per-album addition
            // date. Use that explicit proxy for the required legacy field.
            created: utc_date(catalog.scanned_at)?,
        };
        for (id, kind, entity) in index
            .artists
            .iter()
            .map(|row| (&row.id, EntityKind::Artist, row.entity.id))
            .chain(
                index
                    .albums
                    .iter()
                    .map(|row| (&row.id, EntityKind::Release, row.entity.id)),
            )
            .chain(
                index
                    .songs
                    .iter()
                    .map(|row| (&row.id, EntityKind::Track, row.entity.id)),
            )
        {
            if index.ids.insert(id.clone(), (kind, entity)).is_some() {
                return Err(ProtocolError::new(0, "catalog identities are ambiguous"));
            }
            let reference = EntityRef::of(catalog, kind, entity)
                .ok_or_else(|| ProtocolError::new(0, "catalog identity is invalid"))?;
            index.references.insert(reference, entity);
        }
        // Core personal references may name an artist by its unique MBID.
        // Native keys win, and ambiguous MBIDs never invent an attachment.
        let mut mbids = HashMap::new();
        for artist in &catalog.artists {
            if let Some(mbid) = artist.mbid.as_deref() {
                mbids
                    .entry(mbid)
                    .and_modify(|id| *id = None)
                    .or_insert(Some(artist.id));
            }
        }
        for (mbid, id) in mbids {
            if let Some(id) = id {
                index
                    .references
                    .entry(EntityRef::new(EntityKind::Artist, format!("mbid:{mbid}")))
                    .or_insert(id);
            }
        }
        let mut album_credits = HashSet::new();
        for album in &index.albums {
            if let Some(artist) = album
                .entity
                .album_artist_id
                .and_then(|id| index.artists.get_mut(id as usize))
            {
                album_credits.insert((artist.entity.id, album.entity.id));
                artist.albums.push(album.entity.id);
            }
        }
        for credit in &catalog.credits {
            if credit.entity_kind == EntityKind::Release
                && credit.role == "album"
                && index.albums.get(credit.entity_id as usize).is_some()
                && album_credits.insert((credit.artist_id, credit.entity_id))
                && let Some(artist) = index.artists.get_mut(credit.artist_id as usize)
            {
                artist.albums.push(credit.entity_id);
            }
        }
        let mut credits: Vec<_> = catalog
            .credits
            .iter()
            .filter(|credit| {
                credit.entity_kind == EntityKind::Track
                    && matches!(credit.role.as_str(), "main" | "featured")
            })
            .collect();
        credits.sort_by(|a, b| {
            let artist_key = |id: Id| {
                index
                    .artists
                    .get(id as usize)
                    .map(|row| (row.sort.as_str(), row.id.as_str()))
            };
            (a.role != "main", a.order, artist_key(a.artist_id)).cmp(&(
                b.role != "main",
                b.order,
                artist_key(b.artist_id),
            ))
        });
        let mut assigned = HashSet::new();
        for credit in credits {
            if let Some(song) = index.songs.get_mut(credit.entity_id as usize)
                && assigned.insert((credit.entity_id, credit.artist_id))
            {
                song.artists.push(credit.artist_id);
            }
        }
        for song in &mut index.songs {
            if song.artists.is_empty()
                && let Some(artist) = song
                    .entity
                    .release_id
                    .and_then(|id| catalog.release(id))
                    .and_then(|album| album.album_artist_id)
            {
                song.artists.push(artist);
            }
        }
        let mut attached = HashSet::new();
        for link in &catalog.genre_links {
            if !attached.insert((link.entity_kind, link.entity_id, link.genre_id)) {
                continue;
            }
            match link.entity_kind {
                EntityKind::Release => {
                    if let Some(album) = index.albums.get_mut(link.entity_id as usize) {
                        album.genres.push(link.genre_id);
                    }
                }
                EntityKind::Track => {
                    if let Some(song) = index.songs.get_mut(link.entity_id as usize) {
                        song.genres.push(link.genre_id);
                    }
                }
                _ => {}
            }
        }
        for song in &mut index.songs {
            if song.genres.is_empty()
                && let Some(album) = song
                    .entity
                    .release_id
                    .and_then(|id| index.albums.get(id as usize))
            {
                song.genres.clone_from(&album.genres);
            }
        }
        Ok(index)
    }

    fn resolve(&self, id: &str, kind: EntityKind) -> Result<Id, ProtocolError> {
        self.ids
            .get(id)
            .filter(|(found, _)| *found == kind)
            .map(|(_, entity)| *entity)
            .ok_or_else(|| ProtocolError::new(70, "requested media was not found"))
    }

    /// Resolve a public ID only for the entity kinds accepted by its operation.
    /// Stable private references stay within the server and its personal store.
    pub(super) fn reference(
        &self,
        wanted: &str,
        allowed_kinds: &[EntityKind],
    ) -> Result<EntityRef, ProtocolError> {
        let &(kind, id) = self
            .ids
            .get(wanted)
            .filter(|(kind, _)| allowed_kinds.contains(kind))
            .ok_or_else(|| ProtocolError::new(70, "requested media was not found"))?;
        EntityRef::of(self.catalog, kind, id)
            .ok_or_else(|| ProtocolError::new(0, "catalog identity is invalid"))
    }

    /// Render a native artist, release or track as one compatibility list item.
    pub(super) fn render(&self, kind: EntityKind, id: Id) -> Result<Value, ProtocolError> {
        let value = match kind {
            EntityKind::Artist => self.artists.get(id as usize).map(|row| self.artist(row)),
            EntityKind::Release => self.albums.get(id as usize).map(|row| self.album(row)),
            EntityKind::Track => self.songs.get(id as usize).map(|row| self.song(row)),
            _ => None,
        };
        value.ok_or_else(|| ProtocolError::new(70, "requested media was not found"))
    }

    /// Render a saved stable reference without rescanning the catalog per row.
    pub(super) fn render_reference(&self, reference: &EntityRef) -> Result<Value, ProtocolError> {
        let legacy = (reference.kind == EntityKind::Release)
            .then(|| {
                let (prefix, folder) = reference
                    .key
                    .rsplit_once(aede_core::user::RELEASE_KEY_SEPARATOR)?;
                text::disc_folder(text::file_name(folder))?;
                Some(EntityRef::new(
                    EntityKind::Release,
                    format!("{prefix}|{}", text::folder(folder)),
                ))
            })
            .flatten();
        let &id = self
            .references
            .get(reference)
            .or_else(|| {
                legacy
                    .as_ref()
                    .and_then(|reference| self.references.get(reference))
            })
            .ok_or_else(|| ProtocolError::new(70, "requested media was not found"))?;
        self.render(reference.kind, id)
    }

    /// Known song duration in whole seconds without constructing a JSON item.
    /// A missing song is an error; an unmeasured duration stays unknown.
    pub(super) fn track_duration(
        &self,
        reference: &EntityRef,
    ) -> Result<Option<u64>, ProtocolError> {
        let track = (reference.kind == EntityKind::Track)
            .then(|| self.references.get(reference))
            .flatten()
            .and_then(|id| self.catalog.track(*id))
            .ok_or_else(|| ProtocolError::new(70, "requested song was not found"))?;
        Ok(track.duration_ms.map(|duration| duration / 1000))
    }

    fn artist(&self, row: &ArtistRow<'_>) -> Value {
        json!({
            "id": row.id,
            "name": row.entity.name,
            "albumCount": row.albums.len(),
            "musicBrainzId": row.entity.mbid.as_deref().unwrap_or_default(),
            "sortName": row.entity.sort_name,
        })
    }

    fn genre_name(&self, ids: &[Id]) -> Option<&str> {
        ids.iter()
            .filter_map(|id| self.catalog.genre(*id))
            .map(|genre| genre.name.as_str())
            .next()
    }

    fn album(&self, row: &AlbumRow<'_>) -> Value {
        let mut value = json!({
            "id": row.id,
            "name": row.entity.title,
            "songCount": row.entity.track_ids.len(),
            "duration": row.duration_ms / 1000,
            "created": self.created,
        });
        if let Some(artist) = row
            .entity
            .album_artist_id
            .and_then(|id| self.artists.get(id as usize))
        {
            value["artist"] = json!(artist.entity.name);
            value["artistId"] = json!(artist.id);
        }
        if let Some(year) = row.entity.year {
            value["year"] = json!(year);
        }
        if let Some(genre) = self.genre_name(&row.genres) {
            value["genre"] = json!(genre);
        }
        if let Some(cover) = &row.cover {
            value["coverArt"] = json!(cover);
        }
        value
    }

    fn song(&self, row: &SongRow<'_>) -> Value {
        let mut value = json!({
            "id": row.id,
            "title": row.entity.title,
            "isDir": false,
            "isVideo": false,
            "type": "music",
        });
        if let Some(album) = row
            .entity
            .release_id
            .and_then(|id| self.albums.get(id as usize))
        {
            value["album"] = json!(album.entity.title);
            value["albumId"] = json!(album.id);
            value["parent"] = json!(album.id);
            if let Some(cover) = &album.cover {
                value["coverArt"] = json!(cover);
            }
            if let Some(year) = album.entity.year {
                value["year"] = json!(year);
            }
        }
        if let Some(artist) = row
            .artists
            .first()
            .and_then(|id| self.artists.get(*id as usize))
        {
            value["artistId"] = json!(artist.id);
            value["artist"] = json!(
                row.artists
                    .iter()
                    .filter_map(|id| {
                        self.artists
                            .get(*id as usize)
                            .map(|artist| artist.entity.name.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
        }
        if let Some(position) = row.entity.track_no {
            value["track"] = json!(position);
        }
        if let Some(disc) = row.entity.disc_no {
            value["discNumber"] = json!(disc);
        }
        if let Some(duration) = row.entity.duration_ms {
            value["duration"] = json!(duration / 1000);
        }
        if let Some(genre) = self.genre_name(&row.genres) {
            value["genre"] = json!(genre);
        }
        if let Some(file) = self.catalog.file(row.entity.file_id) {
            value["size"] = json!(file.size);
            value["contentType"] = json!(super::media::content_type(&file.properties));
            value["suffix"] = json!(super::media::suffix(&file.properties));
            if let Some(bitrate) = file.properties.bitrate_kbps {
                value["bitRate"] = json!(bitrate);
            }
        }
        value
    }

    fn sorted_albums(&self) -> Vec<&AlbumRow<'_>> {
        let mut rows: Vec<_> = self.albums.iter().collect();
        rows.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
        rows
    }
}

fn folder(params: &Parameters) -> Result<(), ProtocolError> {
    if params
        .get("musicFolderId")
        .is_some_and(|id| id != MUSIC_FOLDER_ID)
    {
        return Err(ProtocolError::new(70, "music folder was not found"));
    }
    Ok(())
}

/// Resolve a song's public ID to the native reference consumed by playback.
/// The reference stays within the server; responses never serialize its key.
pub(super) fn track_reference(catalog: &Catalog, wanted: &str) -> Result<EntityRef, ProtocolError> {
    for track in &catalog.tracks {
        if opaque_id(catalog, EntityKind::Track, track.id)? == wanted {
            return EntityRef::of(catalog, EntityKind::Track, track.id)
                .ok_or_else(|| ProtocolError::new(0, "catalog track identity is invalid"));
        }
    }
    Err(ProtocolError::new(70, "requested media was not found"))
}

/// Translate one supported ID3 browsing method into its payload fields.
///
/// The caller runs this CPU work under the server's catalog worker limit and
/// wraps the result in the common JSON/XML response envelope.
#[cfg(test)]
pub(super) fn dispatch(
    method: &str,
    params: &Parameters,
    catalog: &Catalog,
) -> Result<Value, ProtocolError> {
    catalog_parameters(method, params)?;
    if method == "getMusicFolders" {
        return Ok(music_folders());
    }
    dispatch_validated(method, params, &Index::new(catalog)?)
}

/// Reuse a single projection while adding the caller's private annotations.
/// Endpoint parameter checks are identical to ordinary catalog dispatch.
pub(super) fn dispatch_index(
    method: &str,
    params: &Parameters,
    index: &Index<'_>,
) -> Result<Value, ProtocolError> {
    catalog_parameters(method, params)?;
    dispatch_validated(method, params, index)
}

fn music_folders() -> Value {
    json!({"musicFolders": {"musicFolder": [{"id": 1, "name": "Aède"}]}})
}

pub(super) fn catalog_parameters(method: &str, params: &Parameters) -> Result<(), ProtocolError> {
    match method {
        "getMusicFolders" => params.allowed(&[])?,
        "getGenres" => params.allowed(&[])?,
        "getArtists" => params.allowed(&["musicFolderId"])?,
        "getArtist" | "getAlbum" | "getSong" => params.allowed(&["id"])?,
        "getAlbumList2" => params.allowed(&[
            "type",
            "size",
            "limit",
            "offset",
            "fromYear",
            "toYear",
            "genre",
            "musicFolderId",
        ])?,
        "getRandomSongs" => {
            params.allowed(&["size", "genre", "fromYear", "toYear", "musicFolderId"])?
        }
        "getSongsByGenre" => params.allowed(&["genre", "count", "offset", "musicFolderId"])?,
        "search3" => params.allowed(&[
            "query",
            "artistCount",
            "artistOffset",
            "albumCount",
            "albumOffset",
            "songCount",
            "songOffset",
            "musicFolderId",
        ])?,
        _ => return Err(ProtocolError::new(0, "catalog method is not supported")),
    }
    folder(params)
}

fn dispatch_validated(
    method: &str,
    params: &Parameters,
    index: &Index<'_>,
) -> Result<Value, ProtocolError> {
    match method {
        "getMusicFolders" => Ok(music_folders()),
        "getArtists" => {
            let mut rows: Vec<_> = index.artists.iter().collect();
            rows.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
            let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
            for row in rows {
                let letter = row
                    .sort
                    .chars()
                    .next()
                    .filter(|letter| letter.is_alphabetic())
                    .map(|letter| letter.to_uppercase().collect::<String>())
                    .unwrap_or_else(|| "#".into());
                groups.entry(letter).or_default().push(index.artist(row));
            }
            Ok(
                json!({"artists": {"ignoredArticles": "", "index": groups.into_iter()
                .map(|(name, artist)| json!({"name": name, "artist": artist})).collect::<Vec<_>>()}}),
            )
        }
        "getArtist" => {
            let id = index.resolve(params.required("id")?, EntityKind::Artist)?;
            let row = &index.artists[id as usize];
            let mut value = index.artist(row);
            let mut albums: Vec<_> = row
                .albums
                .iter()
                .filter_map(|id| index.albums.get(*id as usize))
                .collect();
            albums.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
            value["album"] = json!(
                albums
                    .into_iter()
                    .map(|album| index.album(album))
                    .collect::<Vec<_>>()
            );
            Ok(json!({"artist": value}))
        }
        "getAlbum" => {
            let id = index.resolve(params.required("id")?, EntityKind::Release)?;
            let row = &index.albums[id as usize];
            let mut value = index.album(row);
            value["song"] = json!(
                row.entity
                    .track_ids
                    .iter()
                    .filter_map(|id| index.songs.get(*id as usize))
                    .map(|song| index.song(song))
                    .collect::<Vec<_>>()
            );
            Ok(json!({"album": value}))
        }
        "getSong" => {
            let id = index.resolve(params.required("id")?, EntityKind::Track)?;
            Ok(json!({"song": index.song(&index.songs[id as usize])}))
        }
        "getAlbumList2" => album_list(params, index),
        "getRandomSongs" => random_songs(params, index),
        "getSongsByGenre" => songs_by_genre(params, index),
        "search3" => search(params, index),
        "getGenres" => Ok(genres(index)),
        _ => Err(ProtocolError::new(0, "catalog method is not supported")),
    }
}

fn album_list(params: &Parameters, index: &Index<'_>) -> Result<Value, ProtocolError> {
    if params.get("size").is_some() && params.get("limit").is_some() {
        return Err(ProtocolError::new(10, "specify only one of size or limit"));
    }
    // Supersonic sends limit in its year/genre iterators. Honour that explicit
    // page-size alias with the same bounds rather than ignoring the argument.
    let size = params.number(
        if params.get("limit").is_some() {
            "limit"
        } else {
            "size"
        },
        10,
        PAGE_LIMIT,
    )?;
    let offset = params.number("offset", 0, usize::MAX)?;
    let kind = params.required("type")?;
    if kind != "byYear" && (params.get("fromYear").is_some() || params.get("toYear").is_some()) {
        return Err(ProtocolError::new(0, "year bounds require type=byYear"));
    }
    if kind != "byGenre" && params.get("genre").is_some() {
        return Err(ProtocolError::new(0, "genre requires type=byGenre"));
    }
    let mut rows = index.sorted_albums();
    match kind {
        "alphabeticalByName" | "alphabeticalByArtist" => {
            if kind == "alphabeticalByArtist" {
                let artist_key = |row: &AlbumRow<'_>| {
                    row.entity
                        .album_artist_id
                        .and_then(|id| index.artists.get(id as usize))
                        .map(|artist| artist.sort.as_str())
                        .unwrap_or_default()
                };
                rows.sort_by(|a, b| {
                    (artist_key(a), a.sort.as_str(), &a.id).cmp(&(
                        artist_key(b),
                        b.sort.as_str(),
                        &b.id,
                    ))
                });
            }
        }
        "byYear" => {
            params.required("fromYear")?;
            params.required("toYear")?;
            let from = params.number("fromYear", 0, u32::MAX as usize)? as u32;
            let to = params.number("toYear", 0, u32::MAX as usize)? as u32;
            rows.retain(|row| {
                row.entity
                    .year
                    .is_some_and(|year| (from.min(to)..=from.max(to)).contains(&year))
            });
            rows.sort_by(|a, b| {
                let years = a.entity.year.cmp(&b.entity.year);
                let years = if from > to { years.reverse() } else { years };
                years.then_with(|| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)))
            });
        }
        "byGenre" => {
            let genres = genre_ids(index, params.required("genre")?);
            rows.retain(|row| row.genres.iter().any(|id| genres.contains(id)));
        }
        "random" => {
            let order = random_order(rows.iter().map(|row| row.entity.id).collect())?;
            rows = order
                .ordered_tracks()
                .map(|id| &index.albums[id as usize])
                .collect();
        }
        _ => return Err(ProtocolError::new(0, "album list type is not supported")),
    }
    Ok(
        json!({"albumList2": {"album": rows.into_iter().skip(offset).take(size)
        .map(|row| index.album(row)).collect::<Vec<_>>()}}),
    )
}

/// The queue already owns the platform-independent, unbiased shuffle. Draw
/// one secure seed per request rather than duplicating its permutation logic.
fn random_order(ids: Vec<Id>) -> Result<aede_core::playback::Queue, ProtocolError> {
    let mut seed = [0; 8];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut seed)
        .map_err(|_| ProtocolError::new(0, "random selection is unavailable"))?;
    Ok(aede_core::playback::Queue::new(
        ids,
        Some(u64::from_le_bytes(seed)),
    ))
}

fn genre_ids(index: &Index<'_>, wanted: &str) -> HashSet<Id> {
    let wanted = text::normalize(wanted);
    index
        .catalog
        .genres
        .iter()
        .filter(|genre| text::normalize(&genre.name) == wanted)
        .map(|genre| genre.id)
        .collect()
}

fn songs_by_genre(params: &Parameters, index: &Index<'_>) -> Result<Value, ProtocolError> {
    let genre = genre_ids(index, params.required("genre")?);
    let count = params.number("count", 10, PAGE_LIMIT)?;
    let offset = params.number("offset", 0, usize::MAX)?;
    let mut rows: Vec<_> = index
        .songs
        .iter()
        .filter(|row| row.genres.iter().any(|id| genre.contains(id)))
        .collect();
    rows.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
    Ok(
        json!({"songsByGenre": {"song": rows.into_iter().skip(offset).take(count)
        .map(|row| index.song(row)).collect::<Vec<_>>()}}),
    )
}

fn random_songs(params: &Parameters, index: &Index<'_>) -> Result<Value, ProtocolError> {
    let size = params.number("size", 10, PAGE_LIMIT)?;
    let genres = params
        .get("genre")
        .map(|_| {
            params
                .required("genre")
                .map(|genre| genre_ids(index, genre))
        })
        .transpose()?;
    let year = |name| {
        params
            .get(name)
            .map(|_| {
                params.required(name)?;
                params
                    .number(name, 0, u32::MAX as usize)
                    .map(|year| year as u32)
            })
            .transpose()
    };
    let from = year("fromYear")?;
    let to = year("toYear")?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err(ProtocolError::new(0, "fromYear must not exceed toYear"));
    }
    let ids = index
        .songs
        .iter()
        .filter(|row| {
            let genre_matches = genres
                .as_ref()
                .is_none_or(|genres| row.genres.iter().any(|id| genres.contains(id)));
            let year = row
                .entity
                .release_id
                .and_then(|id| index.catalog.release(id))
                .and_then(|album| album.year);
            genre_matches
                && (from.is_none() && to.is_none()
                    || year.is_some_and(|year| {
                        from.is_none_or(|from| year >= from) && to.is_none_or(|to| year <= to)
                    }))
        })
        .map(|row| row.entity.id)
        .collect();
    let order = random_order(ids)?;
    Ok(
        json!({"randomSongs": {"song": order.ordered_tracks().take(size)
        .map(|id| index.song(&index.songs[id as usize])).collect::<Vec<_>>()}}),
    )
}

/// Status combines actual running scan workers with the last published catalog.
/// The count is a snapshot size, not an invented in-progress item total.
pub(super) fn scan_status(catalog: &Catalog, scanning: bool) -> Result<Value, ProtocolError> {
    Ok(json!({"scanStatus": {"scanning": scanning, "count": catalog.tracks.len()}}))
}

fn search(params: &Parameters, index: &Index<'_>) -> Result<Value, ProtocolError> {
    let query = params
        .get("query")
        .ok_or_else(|| ProtocolError::new(10, "query is required"))?;
    if query.len() > QUERY_LIMIT {
        return Err(ProtocolError::new(0, "query exceeds 1024 bytes"));
    }
    let wanted = text::normalize(query);
    let artist_count = params.number("artistCount", 20, SEARCH_LIMIT)?;
    let artist_offset = params.number("artistOffset", 0, usize::MAX)?;
    let album_count = params.number("albumCount", 20, SEARCH_LIMIT)?;
    let album_offset = params.number("albumOffset", 0, usize::MAX)?;
    let song_count = params.number("songCount", 20, SEARCH_LIMIT)?;
    let song_offset = params.number("songOffset", 0, usize::MAX)?;
    let matching_artists: Vec<_> = index
        .artists
        .iter()
        .map(|row| {
            wanted.is_empty()
                || text::normalize(&row.entity.name).contains(&wanted)
                || row
                    .entity
                    .aliases
                    .iter()
                    .any(|alias| text::normalize(alias).contains(&wanted))
        })
        .collect();
    let artist_matches = |id: Id| matching_artists.get(id as usize).copied().unwrap_or(false);
    let mut matching_albums = vec![false; index.albums.len()];
    for artist in &index.artists {
        if artist_matches(artist.entity.id) {
            for album in &artist.albums {
                if let Some(matched) = matching_albums.get_mut(*album as usize) {
                    *matched = true;
                }
            }
        }
    }
    let mut artists: Vec<_> = index
        .artists
        .iter()
        .filter(|row| artist_matches(row.entity.id))
        .collect();
    artists.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
    let albums = index.sorted_albums().into_iter().filter(|row| {
        row.sort.contains(&wanted)
            || matching_albums
                .get(row.entity.id as usize)
                .copied()
                .unwrap_or(false)
    });
    let mut songs: Vec<_> = index
        .songs
        .iter()
        .filter(|row| {
            row.sort.contains(&wanted)
                || row.artists.iter().copied().any(artist_matches)
                || row
                    .entity
                    .release_id
                    .and_then(|id| index.albums.get(id as usize))
                    .is_some_and(|album| album.sort.contains(&wanted))
        })
        .collect();
    songs.sort_by(|a, b| (a.sort.as_str(), &a.id).cmp(&(b.sort.as_str(), &b.id)));
    Ok(json!({"searchResult3": {
        "artist": artists.into_iter().skip(artist_offset).take(artist_count)
            .map(|row| index.artist(row)).collect::<Vec<_>>(),
        "album": albums.skip(album_offset).take(album_count)
            .map(|row| index.album(row)).collect::<Vec<_>>(),
        "song": songs.into_iter().skip(song_offset).take(song_count)
            .map(|row| index.song(row)).collect::<Vec<_>>(),
    }}))
}

fn genres(index: &Index<'_>) -> Value {
    let mut song_counts = vec![0usize; index.catalog.genres.len()];
    let mut album_counts = vec![0usize; index.catalog.genres.len()];
    for song in &index.songs {
        for id in &song.genres {
            if let Some(count) = song_counts.get_mut(*id as usize) {
                *count += 1;
            }
        }
    }
    for album in &index.albums {
        for id in &album.genres {
            if let Some(count) = album_counts.get_mut(*id as usize) {
                *count += 1;
            }
        }
    }
    let mut rows: Vec<_> = index.catalog.genres.iter().collect();
    rows.sort_by_cached_key(|genre| (text::normalize(&genre.name), genre.key.as_str()));
    json!({"genres": {"genre": rows.into_iter().map(|genre| json!({
        "value": genre.name,
        "songCount": song_counts.get(genre.id as usize).copied().unwrap_or_default(),
        "albumCount": album_counts.get(genre.id as usize).copied().unwrap_or_default(),
    })).collect::<Vec<_>>()}})
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
