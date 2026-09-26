//! Typed representations for the frozen catalog API.

use super::*;

#[derive(Serialize)]
pub(super) struct Status {
    pub(super) status: &'static str,
    pub(super) api_version: u32,
    pub(super) catalog_loaded: bool,
}

#[derive(Serialize)]
pub(super) struct ScanResult {
    pub(super) status: &'static str,
    pub(super) scanned_at: u64,
    pub(super) files: usize,
}

#[derive(Serialize)]
pub(super) struct Library {
    pub(super) scanned_at: u64,
    pub(super) files: usize,
    pub(super) artists: usize,
    pub(super) releases: usize,
    pub(super) recordings: usize,
    pub(super) tracks: usize,
}

#[derive(Serialize)]
pub(super) struct ArtistItem {
    pub(super) reference: String,
    pub(super) name: String,
    pub(super) sort_name: String,
    pub(super) mbid: Option<String>,
    pub(super) aliases: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct ReleaseItem {
    pub(super) reference: String,
    pub(super) title: String,
    pub(super) year: Option<u32>,
    pub(super) album_artist: Option<String>,
    pub(super) track_count: usize,
    pub(super) cover_path: Option<String>,
}

#[derive(Serialize)]
pub(super) struct TrackItem {
    pub(super) reference: String,
    pub(super) title: String,
    pub(super) release: Option<String>,
    pub(super) recording: Option<String>,
    pub(super) duration_ms: Option<u64>,
}

#[derive(Serialize)]
pub(super) struct RecordingItem {
    pub(super) reference: String,
    pub(super) title: String,
    pub(super) mbid: Option<String>,
    pub(super) track_count: usize,
    pub(super) work_count: usize,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum EntityDetail {
    Artist {
        reference: String,
        name: String,
        sort_name: String,
        mbid: Option<String>,
        aliases: Vec<String>,
        releases: Vec<String>,
    },
    Release {
        reference: String,
        title: String,
        year: Option<u32>,
        album_artist: Option<String>,
        tracks: Vec<String>,
        release_group: Option<String>,
        labels: Vec<String>,
        cover_path: Option<String>,
    },
    Track {
        reference: String,
        title: String,
        release: Option<String>,
        recording: Option<String>,
        duration_ms: Option<u64>,
        path: String,
        size: u64,
    },
    Recording {
        reference: String,
        title: String,
        isrc: Option<String>,
        mbid: Option<String>,
        tracks: Vec<String>,
        works: Vec<String>,
    },
    Work {
        reference: String,
        title: String,
        mbid: String,
        recordings: Vec<String>,
    },
    ReleaseGroup {
        reference: String,
        title: String,
        mbid: String,
        releases: Vec<String>,
    },
    Label {
        reference: String,
        name: String,
        mbid: Option<String>,
        releases: Vec<String>,
    },
    Genre {
        reference: String,
        name: String,
        releases: Vec<String>,
        tracks: Vec<String>,
    },
}

#[derive(Serialize)]
pub(super) struct Page<T> {
    pub(super) items: Vec<T>,
    pub(super) total: usize,
    pub(super) offset: usize,
    pub(super) limit: usize,
    pub(super) scanned_at: u64,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListQuery {
    pub(super) offset: Option<String>,
    pub(super) limit: Option<String>,
    pub(super) q: Option<String>,
    pub(super) sort: Option<String>,
    pub(super) order: Option<String>,
    pub(super) mbid: Option<String>,
    pub(super) year: Option<String>,
    pub(super) artist: Option<String>,
    pub(super) release: Option<String>,
    pub(super) work: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ListKind {
    Artist,
    Release,
    Track,
    Recording,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SortMode {
    Catalog,
    Name,
    Title,
    Year,
}

pub(super) struct ListOptions {
    pub(super) offset: usize,
    pub(super) limit: usize,
    pub(super) q: Option<String>,
    pub(super) sort: SortMode,
    pub(super) descending: bool,
    pub(super) mbid: Option<String>,
    pub(super) year: Option<u32>,
    pub(super) artist: Option<String>,
    pub(super) release: Option<String>,
    pub(super) work: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EntityQuery {
    #[serde(rename = "ref")]
    pub(super) reference: String,
}
