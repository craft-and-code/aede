//! Catalog persistence.
//!
//! The format is a JSON document whose every key is a "table": it is the
//! mirror image of the relational schema described in `schema.sql`. At
//! milestone M1, replacing this module with a SQLite implementation will
//! require touching nothing else.
//!
//! Writing is atomic (temporary file then rename): an interruption during
//! saving cannot leave a half-written catalog behind.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::analysis::FileAnalysis;
use crate::audit;
use crate::json::{self, Json};
use crate::model::{
    self, Artist, AudioFile, Catalog, Credit, EntityKind, Genre, GenreLink, Id, IntegrityRecord,
    Label, Relation, Release, Track,
};
use crate::tags::AudioProperties;

/// On-disk format version. Every incompatible change increments it.
pub const FORMAT_VERSION: u32 = 1;

/// Name of the catalog file inside the data folder.
pub const CATALOG_FILE: &str = "catalog.json";

/// What can go wrong when saving or loading a catalog.
///
/// A missing file is not an error: [`load`] reports it as `Ok(None)`.
#[derive(Debug)]
pub enum StoreError {
    /// The catalog file, its parent folder or the temporary file used for the
    /// atomic rename could not be accessed.
    Io(std::io::Error),
    /// The file exists but is not valid JSON, typically after being edited by
    /// hand or truncated by a full disk.
    Parse(json::ParseError),
    /// The file was written by an incompatible version.
    Version {
        /// Version stamped in the file; `0` when the field is missing
        /// altogether, which is how a pre-versioning catalog shows up.
        found: u32,
        /// Version this build understands, that is [`FORMAT_VERSION`].
        expected: u32,
    },
    /// The JSON parsed but the catalog does not hold together: a row without
    /// an identifier, a track pointing at no file, non-contiguous identifiers.
    /// The payload names the specific breach.
    Invalid(&'static str),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "input/output error: {e}"),
            StoreError::Parse(e) => write!(f, "unreadable catalog: {e}"),
            StoreError::Version { found, expected } => write!(
                f,
                "catalog in version {found}, expected {expected} — run a full scan again"
            ),
            StoreError::Invalid(what) => write!(f, "inconsistent catalog: {what}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

/// Default data location: `~/.local/share/aede`, or `$AEDE_HOME` if the
/// variable is set.
pub fn default_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("AEDE_HOME") {
        return PathBuf::from(custom);
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("aede");
    }
    match std::env::var("HOME") {
        Ok(home) => PathBuf::from(home).join(".local/share/aede"),
        Err(_) => PathBuf::from(".aede"),
    }
}

/// Builds the catalog path inside a data folder.
///
/// Nothing is created or checked here; the folder is only made to exist when
/// [`save`] runs.
pub fn catalog_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CATALOG_FILE)
}

/// Saves the catalog atomically.
pub fn save(catalog: &Catalog, path: &Path) -> Result<(), StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, to_json(catalog).to_string_compact())?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Loads a catalog. Returns `Ok(None)` if the file does not exist yet.
pub fn load(path: &Path) -> Result<Option<Catalog>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let value = json::parse(&text).map_err(StoreError::Parse)?;
    from_json(&value).map(Some)
}

// --------------------------------------------------------------------------
// Serialisation
// --------------------------------------------------------------------------

/// Turns a catalog into its JSON document, one array per table.
///
/// Exposed apart from [`save`] so callers can inspect or divert the document
/// without writing it. The result carries the current [`FORMAT_VERSION`], so a
/// round trip through [`from_json`] is always accepted.
pub fn to_json(catalog: &Catalog) -> Json {
    let mut root = Json::obj();
    root.set("format_version", FORMAT_VERSION.into());
    root.set("relation_rules", model::RELATION_RULES.into());
    root.set("scanned_at", catalog.scanned_at.into());
    root.set(
        "excluded",
        Json::Arr(
            catalog
                .excluded
                .iter()
                .map(|f| Json::from(f.as_str()))
                .collect(),
        ),
    );
    root.set(
        "roots",
        Json::Arr(catalog.roots.iter().map(|r| Json::Str(r.clone())).collect()),
    );

    root.set("file", array(&catalog.files, file_to_json));
    root.set("artist", array(&catalog.artists, artist_to_json));
    root.set("release", array(&catalog.releases, release_to_json));
    root.set("track", array(&catalog.tracks, track_to_json));
    root.set("label", array(&catalog.labels, label_to_json));
    root.set("genre", array(&catalog.genres, genre_to_json));
    root.set("credit", array(&catalog.credits, credit_to_json));
    root.set("relation", array(&catalog.relations, relation_to_json));
    root.set(
        "genre_link",
        array(&catalog.genre_links, genre_link_to_json),
    );
    root.set("analysis", array(&catalog.analyses, analysis_to_json));
    root
}

fn array<T>(items: &[T], f: impl Fn(&T) -> Json) -> Json {
    Json::Arr(items.iter().map(f).collect())
}

fn opt_str(value: &Option<String>) -> Json {
    match value {
        Some(s) => Json::Str(s.clone()),
        None => Json::Null,
    }
}

fn opt_num<T: Into<Json> + Copy>(value: &Option<T>) -> Json {
    match value {
        Some(v) => (*v).into(),
        None => Json::Null,
    }
}

fn file_to_json(file: &AudioFile) -> Json {
    let mut o = Json::obj();
    o.set("id", file.id.into());
    o.set("path", file.path.clone().into());
    o.set("size", file.size.into());
    o.set("mtime", file.mtime.into());
    o.set("codec", file.properties.codec.clone().into());
    o.set("container", file.properties.container.clone().into());
    o.set("sample_rate", opt_num(&file.properties.sample_rate));
    o.set(
        "bit_depth",
        opt_num(&file.properties.bit_depth.map(u32::from)),
    );
    o.set(
        "channels",
        opt_num(&file.properties.channels.map(u32::from)),
    );
    o.set("duration_ms", opt_num(&file.properties.duration_ms));
    o.set("bitrate_kbps", opt_num(&file.properties.bitrate_kbps));
    o.set("lossless", file.properties.lossless.into());
    o.set("has_embedded_art", file.has_embedded_art.into());

    let mut tags = Json::obj();
    for (key, values) in &file.tags {
        tags.set(
            key,
            Json::Arr(values.iter().map(|v| Json::Str(v.clone())).collect()),
        );
    }
    o.set("tags", tags);

    // Absent when the file was never checked. The three verdicts are stored as
    // keys rather than as a boolean, because "nothing to check" is an answer of
    // its own and not a missing one.
    if let Some(record) = &file.integrity {
        let mut integrity = Json::obj();
        integrity.set("state", record.verdict.key().into());
        integrity.set("method", record.method.clone().into());
        integrity.set("checked_at", record.checked_at.into());
        if let audit::integrity::Verdict::Damaged { detail } = &record.verdict {
            integrity.set("detail", detail.clone().into());
        }
        o.set("integrity", integrity);
    }
    o
}

/// An imported analysis, field for field.
///
/// Absent measurements are written as `null` rather than left out: the reader
/// then distinguishes "not measured" from "measured as zero", which for a peak
/// or a dynamic range is the whole difference.
fn analysis_to_json(a: &FileAnalysis) -> Json {
    let mut o = Json::obj();
    o.set("path", a.path.clone().into());
    o.set("source", a.source.clone().into());
    o.set("source_version", a.source_version.into());
    o.set("imported_at", a.imported_at.into());
    o.set("size_bytes", a.size_bytes.into());
    o.set("modified_unix", a.modified_unix.into());
    o.set("md5_state", opt_str(&a.md5_state));
    o.set("md5_detail", opt_str(&a.md5_detail));
    o.set("real_bit_depth", opt_num(&a.real_bit_depth.map(u32::from)));
    o.set("requant_rate", opt_float(&a.requant_rate));
    o.set("fake_stereo", opt_bool(&a.fake_stereo));
    o.set("ext_mismatch", opt_bool(&a.ext_mismatch));
    o.set("transcoding", opt_str(&a.transcoding));
    o.set("upscaling", opt_bool(&a.upscaling));
    o.set("upsampling", opt_bool(&a.upsampling));
    o.set("summary", opt_str(&a.summary));
    o.set("detail", opt_str(&a.detail));
    o.set("cutoff_hz", opt_float(&a.cutoff_hz));
    o.set("cutoff_ratio", opt_float(&a.cutoff_ratio));
    o.set("dr_db", opt_float(&a.dr_db));
    o.set("peak_dbfs", opt_float(&a.peak_dbfs));
    o.set("true_peak_dbtp", opt_float(&a.true_peak_dbtp));
    o.set("clipped_samples", opt_num(&a.clipped_samples));
    o.set("clip_events", opt_num(&a.clip_events));
    o.set("clipped", opt_bool(&a.clipped));
    o.set("error", opt_str(&a.error));
    o
}

fn analysis_from_json(item: &Json) -> FileAnalysis {
    FileAnalysis {
        path: item.field_str("path").unwrap_or_default(),
        source: item.field_str("source").unwrap_or_default(),
        source_version: item.field_u32("source_version").unwrap_or(0),
        imported_at: item.field_u64("imported_at").unwrap_or(0),
        size_bytes: item.field_u64("size_bytes").unwrap_or(0),
        modified_unix: item.field_u64("modified_unix").unwrap_or(0),
        md5_state: item.field_str("md5_state"),
        md5_detail: item.field_str("md5_detail"),
        real_bit_depth: item.field_u32("real_bit_depth").map(|v| v as u16),
        requant_rate: item.field_f64("requant_rate"),
        fake_stereo: item.field_optional_bool("fake_stereo"),
        ext_mismatch: item.field_optional_bool("ext_mismatch"),
        transcoding: item.field_str("transcoding"),
        upscaling: item.field_optional_bool("upscaling"),
        upsampling: item.field_optional_bool("upsampling"),
        summary: item.field_str("summary"),
        detail: item.field_str("detail"),
        cutoff_hz: item.field_f64("cutoff_hz"),
        cutoff_ratio: item.field_f64("cutoff_ratio"),
        dr_db: item.field_f64("dr_db"),
        peak_dbfs: item.field_f64("peak_dbfs"),
        true_peak_dbtp: item.field_f64("true_peak_dbtp"),
        clipped_samples: item.field_u64("clipped_samples"),
        clip_events: item.field_u64("clip_events"),
        clipped: item.field_optional_bool("clipped"),
        error: item.field_str("error"),
    }
}

fn opt_bool(value: &Option<bool>) -> Json {
    match value {
        Some(v) => (*v).into(),
        None => Json::Null,
    }
}

fn opt_float(value: &Option<f64>) -> Json {
    match value {
        Some(v) => (*v).into(),
        None => Json::Null,
    }
}

fn artist_to_json(a: &Artist) -> Json {
    let mut o = Json::obj();
    o.set("id", a.id.into());
    o.set("name", a.name.clone().into());
    o.set("sort_name", a.sort_name.clone().into());
    o.set("key", a.key.clone().into());
    o.set("mbid", opt_str(&a.mbid));
    o
}

fn release_to_json(r: &Release) -> Json {
    let mut o = Json::obj();
    o.set("id", r.id.into());
    o.set("title", r.title.clone().into());
    o.set("key", r.key.clone().into());
    o.set("album_artist_id", opt_num(&r.album_artist_id));
    o.set("date", opt_str(&r.date));
    o.set("year", opt_num(&r.year));
    o.set(
        "label_ids",
        Json::Arr(r.label_ids.iter().map(|&id| Json::Num(id as f64)).collect()),
    );
    o.set("catalog_number", opt_str(&r.catalog_number));
    o.set("barcode", opt_str(&r.barcode));
    o.set("media", opt_str(&r.media));
    o.set("mbid", opt_str(&r.mbid));
    o.set("release_group_mbid", opt_str(&r.release_group_mbid));
    o.set("is_compilation", r.is_compilation.into());
    o.set("folder", r.folder.clone().into());
    o.set("cover_path", opt_str(&r.cover_path));
    o.set(
        "track_ids",
        Json::Arr(r.track_ids.iter().map(|&id| Json::Num(id as f64)).collect()),
    );
    o
}

fn track_to_json(t: &Track) -> Json {
    let mut o = Json::obj();
    o.set("id", t.id.into());
    o.set("file_id", t.file_id.into());
    o.set("release_id", opt_num(&t.release_id));
    o.set("title", t.title.clone().into());
    o.set("disc_no", opt_num(&t.disc_no));
    o.set("track_no", opt_num(&t.track_no));
    o.set("duration_ms", opt_num(&t.duration_ms));
    o.set("isrc", opt_str(&t.isrc));
    o.set("mbid", opt_str(&t.mbid));
    o
}

fn label_to_json(l: &Label) -> Json {
    let mut o = Json::obj();
    o.set("id", l.id.into());
    o.set("name", l.name.clone().into());
    o.set("key", l.key.clone().into());
    o
}

fn genre_to_json(g: &Genre) -> Json {
    let mut o = Json::obj();
    o.set("id", g.id.into());
    o.set("name", g.name.clone().into());
    o.set("key", g.key.clone().into());
    o
}

fn credit_to_json(c: &Credit) -> Json {
    let mut o = Json::obj();
    o.set("artist_id", c.artist_id.into());
    o.set("entity_kind", c.entity_kind.as_str().into());
    o.set("entity_id", c.entity_id.into());
    o.set("role", c.role.clone().into());
    o
}

fn relation_to_json(r: &Relation) -> Json {
    let mut o = Json::obj();
    o.set("source_kind", r.source_kind.as_str().into());
    o.set("source_id", r.source_id.into());
    o.set("target_kind", r.target_kind.as_str().into());
    o.set("target_id", r.target_id.into());
    o.set("kind", r.kind.clone().into());
    o.set("weight", r.weight.into());
    o.set("source", r.source.clone().into());
    o
}

fn genre_link_to_json(g: &GenreLink) -> Json {
    let mut o = Json::obj();
    o.set("genre_id", g.genre_id.into());
    o.set("entity_kind", g.entity_kind.as_str().into());
    o.set("entity_id", g.entity_id.into());
    o
}

// --------------------------------------------------------------------------
// Deserialisation
// --------------------------------------------------------------------------

/// Rebuilds a catalog from its JSON document.
///
/// The version is checked before anything else, so an old file is rejected
/// rather than half-decoded. Identifiers are then verified to be contiguous:
/// the model indexes entities by position, and a gap would silently shift
/// every relation.
pub fn from_json(value: &Json) -> Result<Catalog, StoreError> {
    let version = value.field_u32("format_version").unwrap_or(0);
    if version != FORMAT_VERSION {
        return Err(StoreError::Version {
            found: version,
            expected: FORMAT_VERSION,
        });
    }

    let mut catalog = Catalog {
        scanned_at: value.field_u64("scanned_at").unwrap_or(0),
        roots: string_list(value.get("roots")),
        // Absent from a file written before exclusions existed, which reads as
        // "nothing excluded" — the behaviour that file was saved with.
        excluded: string_list(value.get("excluded")),
        ..Default::default()
    };
    // Relations inferred under older rules are stale, not wrong: they are
    // recomputed below rather than refused, so that upgrading Aède never costs
    // a rescan — nor the integrity verdicts a rescan would keep but a rebuilt
    // catalog would not.
    let stale_relations = value.field_u32("relation_rules").unwrap_or(0) != model::RELATION_RULES;

    for item in rows(value, "file") {
        catalog.files.push(file_from_json(item)?);
    }
    for item in rows(value, "artist") {
        catalog.artists.push(Artist {
            id: item
                .field_u32("id")
                .ok_or(StoreError::Invalid("artist without identifier"))?,
            name: item.field_str("name").unwrap_or_default(),
            sort_name: item.field_str("sort_name").unwrap_or_default(),
            key: item.field_str("key").unwrap_or_default(),
            mbid: item.field_str("mbid"),
        });
    }
    for item in rows(value, "release") {
        catalog.releases.push(Release {
            id: item
                .field_u32("id")
                .ok_or(StoreError::Invalid("release without identifier"))?,
            title: item.field_str("title").unwrap_or_default(),
            key: item.field_str("key").unwrap_or_default(),
            album_artist_id: item.field_u32("album_artist_id"),
            date: item.field_str("date"),
            year: item.field_u32("year"),
            label_ids: id_list(item.get("label_ids")),
            catalog_number: item.field_str("catalog_number"),
            barcode: item.field_str("barcode"),
            media: item.field_str("media"),
            mbid: item.field_str("mbid"),
            release_group_mbid: item.field_str("release_group_mbid"),
            is_compilation: item.field_bool("is_compilation"),
            folder: item.field_str("folder").unwrap_or_default(),
            cover_path: item.field_str("cover_path"),
            track_ids: id_list(item.get("track_ids")),
        });
    }
    for item in rows(value, "track") {
        catalog.tracks.push(Track {
            id: item
                .field_u32("id")
                .ok_or(StoreError::Invalid("track without identifier"))?,
            file_id: item
                .field_u32("file_id")
                .ok_or(StoreError::Invalid("track without file"))?,
            release_id: item.field_u32("release_id"),
            title: item.field_str("title").unwrap_or_default(),
            disc_no: item.field_u32("disc_no"),
            track_no: item.field_u32("track_no"),
            duration_ms: item.field_u64("duration_ms"),
            isrc: item.field_str("isrc"),
            mbid: item.field_str("mbid"),
        });
    }
    for item in rows(value, "label") {
        catalog.labels.push(Label {
            id: item
                .field_u32("id")
                .ok_or(StoreError::Invalid("label without identifier"))?,
            name: item.field_str("name").unwrap_or_default(),
            key: item.field_str("key").unwrap_or_default(),
        });
    }
    for item in rows(value, "genre") {
        catalog.genres.push(Genre {
            id: item
                .field_u32("id")
                .ok_or(StoreError::Invalid("genre without identifier"))?,
            name: item.field_str("name").unwrap_or_default(),
            key: item.field_str("key").unwrap_or_default(),
        });
    }
    for item in rows(value, "credit") {
        let Some(kind) = item
            .field_str("entity_kind")
            .and_then(|s| EntityKind::parse_kind(&s))
        else {
            continue;
        };
        catalog.credits.push(Credit {
            artist_id: item.field_u32("artist_id").unwrap_or(0),
            entity_kind: kind,
            entity_id: item.field_u32("entity_id").unwrap_or(0),
            role: item.field_str("role").unwrap_or_default(),
        });
    }
    for item in rows(value, "relation") {
        let (Some(source_kind), Some(target_kind)) = (
            item.field_str("source_kind")
                .and_then(|s| EntityKind::parse_kind(&s)),
            item.field_str("target_kind")
                .and_then(|s| EntityKind::parse_kind(&s)),
        ) else {
            continue;
        };
        catalog.relations.push(Relation {
            source_kind,
            source_id: item.field_u32("source_id").unwrap_or(0),
            target_kind,
            target_id: item.field_u32("target_id").unwrap_or(0),
            kind: item.field_str("kind").unwrap_or_default(),
            weight: item.field_u32("weight").unwrap_or(1),
            source: item.field_str("source").unwrap_or_default(),
        });
    }
    for item in rows(value, "genre_link") {
        let Some(kind) = item
            .field_str("entity_kind")
            .and_then(|s| EntityKind::parse_kind(&s))
        else {
            continue;
        };
        catalog.genre_links.push(GenreLink {
            genre_id: item.field_u32("genre_id").unwrap_or(0),
            entity_kind: kind,
            entity_id: item.field_u32("entity_id").unwrap_or(0),
        });
    }

    for item in rows(value, "analysis") {
        catalog.analyses.push(analysis_from_json(item));
    }

    verify_integrity(&catalog)?;
    if stale_relations {
        model::rebuild_relations(&mut catalog);
    }
    Ok(catalog)
}

/// The `Vec`s are indexed by identifier: we check that the file read back
/// respects that invariant before handing it to the rest of the program.
fn verify_integrity(catalog: &Catalog) -> Result<(), StoreError> {
    for (index, file) in catalog.files.iter().enumerate() {
        if file.id as usize != index {
            return Err(StoreError::Invalid("non-contiguous file identifiers"));
        }
    }
    for (index, artist) in catalog.artists.iter().enumerate() {
        if artist.id as usize != index {
            return Err(StoreError::Invalid("non-contiguous artist identifiers"));
        }
    }
    for (index, release) in catalog.releases.iter().enumerate() {
        if release.id as usize != index {
            return Err(StoreError::Invalid("non-contiguous release identifiers"));
        }
    }
    for (index, track) in catalog.tracks.iter().enumerate() {
        if track.id as usize != index {
            return Err(StoreError::Invalid("non-contiguous track identifiers"));
        }
        if track.file_id as usize >= catalog.files.len() {
            return Err(StoreError::Invalid("track attached to a missing file"));
        }
    }
    Ok(())
}

fn file_from_json(item: &Json) -> Result<AudioFile, StoreError> {
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(Json::Obj(map)) = item.get("tags") {
        for (key, values) in map {
            tags.insert(key.clone(), string_list(Some(values)));
        }
    }
    Ok(AudioFile {
        id: item
            .field_u32("id")
            .ok_or(StoreError::Invalid("file without identifier"))?,
        path: item
            .field_str("path")
            .ok_or(StoreError::Invalid("file without path"))?,
        size: item.field_u64("size").unwrap_or(0),
        mtime: item.field_u64("mtime").unwrap_or(0),
        properties: AudioProperties {
            codec: item.field_str("codec").unwrap_or_default(),
            container: item.field_str("container").unwrap_or_default(),
            sample_rate: item.field_u32("sample_rate"),
            bit_depth: item.field_u32("bit_depth").map(|v| v as u16),
            channels: item.field_u32("channels").map(|v| v as u16),
            duration_ms: item.field_u64("duration_ms"),
            bitrate_kbps: item.field_u32("bitrate_kbps"),
            lossless: item.field_bool("lossless"),
        },
        has_embedded_art: item.field_bool("has_embedded_art"),
        tags,
        integrity: integrity_from_json(item.get("integrity")),
    })
}

/// Reads back a stored verdict; an unknown state is treated as no verdict at
/// all, so a catalog written by a later version degrades instead of failing.
fn integrity_from_json(value: Option<&Json>) -> Option<IntegrityRecord> {
    let value = value?;
    let verdict = match value.field_str("state")?.as_str() {
        "intact" => audit::integrity::Verdict::Intact,
        "nothing_to_check" => audit::integrity::Verdict::NothingToCheck,
        "damaged" => audit::integrity::Verdict::Damaged {
            detail: value.field_str("detail").unwrap_or_default(),
        },
        _ => return None,
    };
    Some(IntegrityRecord {
        verdict,
        method: value.field_str("method").unwrap_or_default(),
        checked_at: value.field_u64("checked_at").unwrap_or(0),
    })
}

fn rows<'a>(value: &'a Json, key: &str) -> &'a [Json] {
    value.get(key).and_then(|v| v.as_arr()).unwrap_or(&[])
}

fn string_list(value: Option<&Json>) -> Vec<String> {
    value
        .and_then(|v| v.as_arr())
        .map(|items| items.iter().filter_map(|i| i.as_string()).collect())
        .unwrap_or_default()
}

fn id_list(value: Option<&Json>) -> Vec<Id> {
    value
        .and_then(|v| v.as_arr())
        .map(|items| items.iter().filter_map(|i| i.as_u32()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{self, ScannedFile};
    use crate::tags::RawTags;

    fn example_catalog() -> Catalog {
        let mut tags = RawTags::default();
        tags.insert("title", "So What");
        tags.insert("artist", "Miles Davis");
        tags.insert("album", "Kind of Blue");
        tags.insert("date", "1959");
        tags.insert("genre", "Jazz");
        tags.insert("label", "Columbia");
        tags.properties.codec = "flac".into();
        tags.properties.container = "flac".into();
        tags.properties.sample_rate = Some(44_100);
        tags.properties.bit_depth = Some(16);
        tags.properties.channels = Some(2);
        tags.properties.duration_ms = Some(545_000);
        tags.properties.lossless = true;

        model::build(
            vec![ScannedFile {
                path: "/music/Miles Davis/Kind of Blue/01 So What.flac".into(),
                size: 42_000_000,
                mtime: 1_700_000_000,
                tags,
                folder_cover: Some("/music/Miles Davis/Kind of Blue/cover.jpg".into()),
                integrity: None,
            }],
            vec!["/music".into()],
            1_700_000_100,
        )
    }

    #[test]
    fn full_round_trip() {
        let original = example_catalog();
        let encoded = to_json(&original);
        let decoded = from_json(&encoded).expect("read back");

        assert_eq!(decoded.roots, original.roots);
        assert_eq!(decoded.scanned_at, original.scanned_at);
        assert_eq!(decoded.files.len(), original.files.len());
        assert_eq!(decoded.artists.len(), original.artists.len());
        assert_eq!(decoded.tracks.len(), original.tracks.len());
        assert_eq!(decoded.credits.len(), original.credits.len());

        let f = &decoded.files[0];
        assert_eq!(f.path, original.files[0].path);
        assert_eq!(f.properties.sample_rate, Some(44_100));
        assert_eq!(f.properties.bit_depth, Some(16));
        assert!(f.properties.lossless);
        assert_eq!(f.first_tag("artist"), Some("Miles Davis"));

        let (albums, _) = decoded.find_releases("Kind of Blue");
        let album = albums.first().expect("album");
        assert_eq!(album.year, Some(1959));
        assert!(album.cover_path.is_some());
    }

    #[test]
    fn round_trip_through_json_text() {
        let original = example_catalog();
        let text = to_json(&original).to_string_compact();
        let read_back = from_json(&json::parse(&text).unwrap()).unwrap();
        assert_eq!(read_back.tracks[0].title, "So What");
    }

    #[test]
    fn an_integrity_verdict_survives_the_round_trip() {
        let mut original = example_catalog();
        original.files[0].integrity = Some(IntegrityRecord {
            verdict: audit::integrity::Verdict::Damaged {
                detail: "frame 12: audio checksum mismatch".into(),
            },
            method: audit::integrity::FLAC_METHOD.into(),
            checked_at: 1_700_000_500,
        });
        let text = to_json(&original).to_string_compact();
        let read_back = from_json(&json::parse(&text).unwrap()).unwrap();
        let record = read_back.files[0]
            .integrity
            .as_ref()
            .expect("the verdict is kept");
        assert_eq!(record.method, audit::integrity::FLAC_METHOD);
        assert_eq!(record.checked_at, 1_700_000_500);
        match &record.verdict {
            audit::integrity::Verdict::Damaged { detail } => {
                assert!(detail.contains("frame 12"), "detail kept: {detail}")
            }
            other => panic!("wrong verdict: {other:?}"),
        }
    }

    #[test]
    fn a_file_never_checked_stores_nothing() {
        // Absent, not "false": a file that was never looked at must not be
        // confused with one that carries no checksum.
        let text = to_json(&example_catalog()).to_string_compact();
        assert!(!text.contains("integrity"), "nothing is written");
        let read_back = from_json(&json::parse(&text).unwrap()).unwrap();
        assert!(read_back.files[0].integrity.is_none());
    }

    #[test]
    fn relations_inferred_under_older_rules_are_rebuilt_on_load() {
        // Upgrading Aède must not require a rescan to get the benefit of a new
        // inference, and must not cost the integrity verdicts a rebuilt
        // catalog would lose.
        // Two performers on one track, which is what a relation is made of.
        let mut tags = RawTags::default();
        tags.insert("title", "Sous le vent");
        tags.insert("artist", "Garou feat. Céline Dion");
        tags.insert("album", "Duos");
        tags.properties.duration_ms = Some(200_000);
        let original = model::build(
            vec![ScannedFile {
                path: "/music/Duos/01.flac".into(),
                size: 1_000,
                mtime: 0,
                tags,
                folder_cover: None,
                integrity: None,
            }],
            vec!["/music".into()],
            0,
        );
        assert!(!original.relations.is_empty(), "the fixture has relations");

        let mut encoded = to_json(&original);
        encoded.set("relation_rules", 0u32.into());
        encoded.set("relation", Json::Arr(Vec::new()));

        let read_back = from_json(&encoded).expect("an older catalog still loads");
        assert_eq!(
            read_back.relations.len(),
            original.relations.len(),
            "the relations were recomputed from the credits"
        );

        // A catalog written by this version is left exactly as it is.
        let current = to_json(&read_back);
        assert_eq!(
            current.field_u32("relation_rules"),
            Some(model::RELATION_RULES)
        );
    }

    #[test]
    fn incompatible_version_is_refused() {
        let mut encoded = to_json(&example_catalog());
        encoded.set("format_version", 999u32.into());
        assert!(matches!(
            from_json(&encoded),
            Err(StoreError::Version { .. })
        ));
    }

    #[test]
    fn inconsistent_identifiers_are_refused() {
        let mut encoded = to_json(&example_catalog());
        if let Some(Json::Arr(files)) = encoded.get("file").cloned().as_mut() {
            files[0].set("id", 7u32.into());
            encoded.set("file", Json::Arr(files.clone()));
        }
        assert!(matches!(from_json(&encoded), Err(StoreError::Invalid(_))));
    }

    #[test]
    fn atomic_save() {
        let folder = std::env::temp_dir().join("aede_test_store");
        let _ = std::fs::remove_dir_all(&folder);
        let path = catalog_path(&folder);

        assert!(load(&path).unwrap().is_none(), "no catalog to begin with");
        save(&example_catalog(), &path).expect("write");
        let read_back = load(&path).expect("read").expect("present");
        assert_eq!(read_back.tracks.len(), 1);
        // No temporary file must remain.
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
