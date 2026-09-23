//! Statistics on the catalog.
//!
//! Everything is computed on demand from the in-memory catalog: no aggregate
//! value is stored, so none can drift out of sync.

use std::collections::BTreeMap;

use crate::model::{self, Catalog, EntityKind, Id};

/// Named breakdown entry: `(label, count, size in bytes)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    /// Name of the slice as it is shown, already formatted (`FLAC`, `44.1 kHz`, `1970`).
    pub label: String,
    /// How many items fall into this slice.
    pub count: usize,
    /// Disk space these items occupy, so a breakdown can be read by weight and not only by
    /// number.
    pub bytes: u64,
}

/// Snapshot of what the catalog contains, recomputed on each call.
#[derive(Debug, Clone, Default)]
pub struct Stats {
    /// Audio files seen on disk, whether or not they could be attached to anything.
    pub files: usize,
    /// Recordings held by the catalog, one per file.
    pub tracks: usize,
    /// Albums, EPs and singles reconstructed from the tags.
    pub releases: usize,
    /// Releases gathering several artists rather than one, and so excluded from a discography.
    pub compilations: usize,
    /// Distinct artists, all roles taken together.
    pub artists: usize,
    /// Artists carrying at least one album.
    pub album_artists: usize,
    /// Distinct record labels credited across the releases.
    pub labels: usize,
    /// Distinct genres in use, after the tags have been normalized.
    pub genres: usize,
    /// Cumulative playing time of the library, in milliseconds.
    pub total_duration_ms: u64,
    /// Space the audio files take up on disk, in bytes; cover art is not counted.
    pub total_bytes: u64,
    /// Tracks with no identified album.
    pub orphan_tracks: usize,

    /// Files grouped by container format, the untagged ones landing under `unknown`.
    pub by_codec: Vec<Bucket>,
    /// Files grouped by perceived quality, following [`QualityTier`].
    pub by_quality: Vec<Bucket>,
    /// Files grouped by sampling frequency; files that declare none are left out.
    pub by_sample_rate: Vec<Bucket>,
    /// Releases grouped by decade of publication, undated ones gathered under a separate label.
    pub by_decade: Vec<Bucket>,

    /// Share of releases having cover art (embedded or in the folder).
    ///
    /// This field and the three that follow are proportions between 0 and 1, not percentages;
    /// the display layer decides how to present them.
    pub cover_ratio: f64,
    /// Share of tracks carrying a year.
    pub year_ratio: f64,
    /// Share of tracks carrying at least one genre.
    pub genre_ratio: f64,
    /// Share of tracks already identified by a MusicBrainz MBID.
    pub mbid_ratio: f64,
}

/// Quality class, as a listener perceives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityTier {
    /// Lossless beyond CD: more than 48 kHz or more than 16 bits.
    HiRes,
    /// Lossless, CD quality or below.
    Lossless,
    /// Lossy, 256 kbps or more.
    LossyHigh,
    /// Lossy, below 256 kbps.
    LossyLow,
    /// Nothing reliable to go on: unreadable file, or a bitrate the decoder never reported.
    Unknown,
}

impl QualityTier {
    /// Wording of the tier for a listing or a breakdown, in a form the reader can act on.
    pub fn label(self) -> &'static str {
        match self {
            QualityTier::HiRes => "Hi-res",
            QualityTier::Lossless => "Lossless (CD)",
            QualityTier::LossyHigh => "Lossy (>= 256 kbps)",
            QualityTier::LossyLow => "Lossy (< 256 kbps)",
            QualityTier::Unknown => "Unknown",
        }
    }
}

/// Ranks a file from what its audio properties actually establish.
///
/// A file that names no codec, or a lossy one that declares no bitrate, stays `Unknown`: better
/// an admitted gap than a tier guessed from the extension alone.
pub fn quality_tier(props: &crate::tags::AudioProperties) -> QualityTier {
    if props.codec.is_empty() {
        return QualityTier::Unknown;
    }
    if props.lossless {
        if props.is_hi_res() {
            QualityTier::HiRes
        } else {
            QualityTier::Lossless
        }
    } else {
        match props.bitrate_kbps {
            Some(kbps) if kbps >= 256 => QualityTier::LossyHigh,
            Some(_) => QualityTier::LossyLow,
            None => QualityTier::Unknown,
        }
    }
}

/// Runs through the whole catalog and produces the counts, breakdowns and completeness ratios.
///
/// The cost grows with the size of the library; call it when the catalog has settled rather than
/// after each change.
pub fn compute(catalog: &Catalog) -> Stats {
    let mut stats = Stats {
        files: catalog.files.len(),
        tracks: catalog.tracks.len(),
        releases: catalog.releases.len(),
        compilations: catalog.releases.iter().filter(|r| r.is_compilation).count(),
        artists: catalog.artists.len(),
        labels: catalog.labels.len(),
        genres: catalog.genres.len(),
        total_duration_ms: catalog.total_duration_ms(),
        total_bytes: catalog.total_size(),
        orphan_tracks: catalog
            .tracks
            .iter()
            .filter(|t| t.release_id.is_none())
            .count(),
        ..Default::default()
    };

    stats.album_artists = catalog
        .releases
        .iter()
        .filter_map(|r| r.album_artist_id)
        .collect::<std::collections::BTreeSet<_>>()
        .len();

    // --- Per-file breakdowns ----------------------------------------------
    let mut codecs: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut qualities: BTreeMap<&'static str, (usize, u64)> = BTreeMap::new();
    let mut rates: BTreeMap<u32, (usize, u64)> = BTreeMap::new();

    for file in &catalog.files {
        let codec = if file.properties.codec.is_empty() {
            "unknown".to_string()
        } else {
            file.properties.codec.to_uppercase()
        };
        let entry = codecs.entry(codec).or_default();
        entry.0 += 1;
        entry.1 += file.size;

        let tier = quality_tier(&file.properties).label();
        let entry = qualities.entry(tier).or_default();
        entry.0 += 1;
        entry.1 += file.size;

        if let Some(rate) = file.properties.sample_rate {
            let entry = rates.entry(rate).or_default();
            entry.0 += 1;
            entry.1 += file.size;
        }
    }

    stats.by_codec = sorted_buckets(codecs.into_iter());
    stats.by_quality = sorted_buckets(qualities.into_iter().map(|(k, v)| (k.to_string(), v)));
    stats.by_sample_rate = rates
        .into_iter()
        .map(|(rate, (count, bytes))| Bucket {
            label: format_rate(rate),
            count,
            bytes,
        })
        .collect();

    // --- Breakdown by decade ----------------------------------------------
    let mut decades: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for release in &catalog.releases {
        let label = match release.year {
            Some(year) => format!("{}", (year / 10) * 10),
            None => "No year".to_string(),
        };
        let bytes: u64 = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .filter_map(|t| catalog.file(t.file_id))
            .map(|f| f.size)
            .sum();
        let entry = decades.entry(label).or_default();
        entry.0 += 1;
        entry.1 += bytes;
    }
    stats.by_decade = decades
        .into_iter()
        .map(|(label, (count, bytes))| Bucket {
            label,
            count,
            bytes,
        })
        .collect();

    // --- Completeness rates -----------------------------------------------
    let with_cover = catalog
        .releases
        .iter()
        .filter(|r| {
            r.cover_path.is_some()
                || r.track_ids
                    .iter()
                    .filter_map(|&id| catalog.track(id))
                    .filter_map(|t| catalog.file(t.file_id))
                    .any(|f| f.has_embedded_art)
        })
        .count();
    stats.cover_ratio = ratio(with_cover, catalog.releases.len());

    let with_year = catalog
        .tracks
        .iter()
        .filter(|t| {
            t.release_id
                .and_then(|id| catalog.release(id))
                .map(|r| r.year.is_some())
                .unwrap_or(false)
        })
        .count();
    stats.year_ratio = ratio(with_year, catalog.tracks.len());

    let with_genre = catalog
        .genre_links
        .iter()
        .filter(|g| g.entity_kind == EntityKind::Track)
        .map(|g| g.entity_id)
        .collect::<std::collections::BTreeSet<Id>>()
        .len();
    stats.genre_ratio = ratio(with_genre, catalog.tracks.len());

    let with_mbid = catalog.tracks.iter().filter(|t| t.mbid.is_some()).count();
    stats.mbid_ratio = ratio(with_mbid, catalog.tracks.len());

    stats
}

/// The most present artists, by number of distinct tracks they are audible on.
///
/// Writing and production credits are excluded on purpose: a songwriter who
/// never performs would otherwise outrank the singers of their own catalog.
/// Those credits remain visible on the artist page and in [`top_writers`].
///
/// We count tracks, not credits: an artist who is both performer and composer
/// of a piece must not count it twice.
pub fn top_artists(catalog: &Catalog, limit: usize) -> Vec<(Id, usize)> {
    top_by_role(catalog, limit, true)
}

/// The most credited writers and producers, by number of distinct tracks.
pub fn top_writers(catalog: &Catalog, limit: usize) -> Vec<(Id, usize)> {
    top_by_role(catalog, limit, false)
}

fn top_by_role(catalog: &Catalog, limit: usize, performing: bool) -> Vec<(Id, usize)> {
    let mut sets: BTreeMap<Id, std::collections::BTreeSet<Id>> = BTreeMap::new();
    for credit in &catalog.credits {
        if credit.entity_kind == EntityKind::Track
            && model::is_performing_role(&credit.role) == performing
        {
            sets.entry(credit.artist_id)
                .or_default()
                .insert(credit.entity_id);
        }
    }
    let counts = sets.into_iter().map(|(id, set)| (id, set.len())).collect();
    top(counts, limit)
}

/// The most represented genres, by number of tracks.
pub fn top_genres(catalog: &Catalog, limit: usize) -> Vec<(Id, usize)> {
    let mut sets: BTreeMap<Id, std::collections::BTreeSet<Id>> = BTreeMap::new();
    for link in &catalog.genre_links {
        if link.entity_kind == EntityKind::Track {
            sets.entry(link.genre_id)
                .or_default()
                .insert(link.entity_id);
        }
    }
    top(
        sets.into_iter().map(|(id, set)| (id, set.len())).collect(),
        limit,
    )
}

/// The most present labels, by number of releases.
pub fn top_labels(catalog: &Catalog, limit: usize) -> Vec<(Id, usize)> {
    let mut counts: BTreeMap<Id, usize> = BTreeMap::new();
    for release in &catalog.releases {
        for &label_id in &release.label_ids {
            *counts.entry(label_id).or_insert(0) += 1;
        }
    }
    top(counts, limit)
}

fn top(counts: BTreeMap<Id, usize>, limit: usize) -> Vec<(Id, usize)> {
    let mut list: Vec<(Id, usize)> = counts.into_iter().collect();
    // On a tie, the smallest identifier comes first: stable result.
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(limit);
    list
}

fn sorted_buckets(items: impl Iterator<Item = (String, (usize, u64))>) -> Vec<Bucket> {
    let mut list: Vec<Bucket> = items
        .map(|(label, (count, bytes))| Bucket {
            label,
            count,
            bytes,
        })
        .collect();
    list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    list
}

fn ratio(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

fn format_rate(rate: u32) -> String {
    let khz = rate as f64 / 1000.0;
    if khz.fract() == 0.0 {
        format!("{} kHz", khz as u32)
    } else {
        format!("{khz:.1} kHz")
    }
}

#[cfg(test)]
#[path = "stats_tests.rs"]
mod tests;
