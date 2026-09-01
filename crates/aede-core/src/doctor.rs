//! Library diagnosis.
//!
//! A real library is always damaged: missing tags, duplicates, incomplete
//! albums, mixed editions. This module brings them to light without changing
//! anything — automatic correction will come later, and will have to be
//! reversible.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{self, Catalog, Id};
use crate::text;

/// How much a problem hurts, from the most serious to the least.
///
/// The declaration order is the order of the report: the derived comparison puts `Error` before
/// `Warning` before `Info`, and [`diagnose`] sorts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Prevents the rest from working (no title, no duration).
    Error,
    /// Degrades browsing or ranking.
    Warning,
    /// Plain completeness remark.
    Info,
}

impl Severity {
    /// Short lowercase name of the level, meant to prefix a report line or to filter on.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// Nature of a problem found in the library, independent of the file it was found on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueKind {
    /// No title in the tags: the one on display was inferred from the file name.
    MissingTitle,
    /// No artist in the tags, so the track hangs off no discography.
    MissingArtist,
    /// No album in the tags: the track will stay on its own, outside any release.
    MissingAlbum,
    /// No usable year, which leaves the track out of any chronological ordering.
    MissingDate,
    /// No track number, so playback order within the album cannot be guaranteed.
    MissingTrackNumber,
    /// Duration could not be read, which usually points at a truncated or corrupted file.
    MissingDuration,
    /// Same artist and title heard again at a near-identical duration: the copies waste space.
    DuplicateTrack,
    /// A whole album is present twice, in the same quality: one of the two copies is dead weight.
    DuplicateAlbum,
    /// The same album is kept in two encodings. Not a defect — worth knowing.
    OtherEdition,
    /// A decoder found the audio no longer matches the MD5 the file carries.
    Md5Mismatch,
    /// Tracks missing from a disc, the sign of a rip left unfinished — whether
    /// they are gaps in the numbering or a tail short of the announced total.
    IncompleteAlbum,
    /// One release mixes codecs or sample rates, which usually means it was assembled from
    /// different sources.
    MixedQuality,
    /// The release carries no cover art, neither embedded nor beside the files.
    MissingCover,
    /// The announced year falls outside the plausible range for a recording.
    SuspiciousYear,
    /// A checksum carried by the file does not match its contents: the audio
    /// has been damaged since it was written.
    DamagedAudio,
}

impl IssueKind {
    /// Weight given to this kind of problem, which fixes its rank in the report.
    pub fn severity(self) -> Severity {
        match self {
            IssueKind::MissingTitle
            | IssueKind::MissingArtist
            | IssueKind::MissingDuration
            | IssueKind::DamagedAudio
            | IssueKind::Md5Mismatch => Severity::Error,
            IssueKind::MissingAlbum
            | IssueKind::DuplicateTrack
            | IssueKind::DuplicateAlbum
            | IssueKind::IncompleteAlbum
            | IssueKind::MixedQuality
            | IssueKind::SuspiciousYear => Severity::Warning,
            IssueKind::MissingDate
            | IssueKind::MissingTrackNumber
            | IssueKind::MissingCover
            | IssueKind::OtherEdition => Severity::Info,
        }
    }

    /// Wording of the problem in one short phrase, ready to head a report line.
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::MissingTitle => "missing title",
            IssueKind::MissingArtist => "missing artist",
            IssueKind::MissingAlbum => "missing album",
            IssueKind::MissingDate => "missing year",
            IssueKind::MissingTrackNumber => "missing track number",
            IssueKind::MissingDuration => "unreadable duration",
            IssueKind::DuplicateTrack => "likely duplicate",
            IssueKind::DuplicateAlbum => "album present twice",
            IssueKind::OtherEdition => "album kept in two encodings",
            IssueKind::Md5Mismatch => "audio does not match its MD5",
            IssueKind::IncompleteAlbum => "incomplete album",
            IssueKind::MixedQuality => "mixed quality within an album",
            IssueKind::MissingCover => "missing cover art",
            IssueKind::SuspiciousYear => "implausible year",
            IssueKind::DamagedAudio => "damaged audio",
        }
    }
}

/// How many parts the file says the whole holds, if it says so at all.
///
/// One helper for two questions — how many tracks on this disc, how many discs
/// in this set — because they are the same question asked of two vocabularies,
/// and both are read the same way: a dedicated total field, or the trailing
/// half of a `9/12` written into the number itself, which many taggers do.
/// `field` names the total, `numbered` the field that may carry the pair.
///
/// Only the leading digits of a value are kept, and a value that does not parse
/// is no answer rather than a wrong one. The names given here are the canonical
/// ones: `tags::canonical_key` has already folded `totaltracks`, `disctotal`
/// and the rest into a single spelling, so this must not keep a second list of
/// aliases that would then have to be maintained beside it.
fn announced(file: &crate::model::AudioFile, field: &str, numbered: &str) -> Option<u32> {
    let leading = |value: &str| {
        let digits: String = value
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse::<u32>().ok()
    };
    let from_pair = file
        .first_tag(numbered)
        .and_then(|value| value.split_once('/').map(|(_, total)| total.to_string()))
        .and_then(|total| leading(&total));
    let from_field = file.first_tag(field).and_then(leading);
    from_pair.into_iter().chain(from_field).max()
}

/// How many tracks the file says its disc holds, if it says so at all.
fn announced_total(file: &crate::model::AudioFile) -> Option<u32> {
    announced(file, "tracktotal", "tracknumber")
}

/// How many discs the file says its set holds, if it says so at all.
fn announced_discs(file: &crate::model::AudioFile) -> Option<u32> {
    announced(file, "disctotal", "discnumber")
}

/// One observation made on the library, tied to the files that carry it.
#[derive(Debug, Clone)]
pub struct Issue {
    /// What was observed, which also decides how serious the observation is.
    pub kind: IssueKind,
    /// Plain description, ready to be displayed.
    pub detail: String,
    /// Files concerned (a single one, or several for duplicates).
    pub files: Vec<String>,
}

impl Issue {
    /// Weight of the observation, taken from its kind: no problem carries a level of its own.
    pub fn severity(&self) -> Severity {
        self.kind.severity()
    }
}

/// Full analysis. The result is sorted by severity then by kind, for a display
/// that is stable from one run to the next.
pub fn diagnose(catalog: &Catalog) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_tracks(catalog, &mut issues);
    check_integrity(catalog, &mut issues);
    check_imported_analyses(catalog, &mut issues);
    check_duplicate_albums(catalog, &mut issues);
    check_other_editions(catalog, &mut issues);
    check_duplicates(catalog, &mut issues);
    check_releases(catalog, &mut issues);

    issues.sort_by(|a, b| {
        a.severity()
            .cmp(&b.severity())
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.files.cmp(&b.files))
    });
    issues
}

/// Number of problems per severity.
pub fn summary(issues: &[Issue]) -> BTreeMap<Severity, usize> {
    let mut out = BTreeMap::new();
    for issue in issues {
        *out.entry(issue.severity()).or_insert(0) += 1;
    }
    out
}

fn check_tracks(catalog: &Catalog, issues: &mut Vec<Issue>) {
    for track in &catalog.tracks {
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        let path = file.path.clone();

        if file.first_tag("title").is_none() {
            issues.push(Issue {
                kind: IssueKind::MissingTitle,
                detail: format!("title inferred from the file name: \"{}\"", track.title),
                files: vec![path.clone()],
            });
        }
        if file.first_tag("artist").is_none() {
            issues.push(Issue {
                kind: IssueKind::MissingArtist,
                detail: "no artist in the tags".into(),
                files: vec![path.clone()],
            });
        }
        if file.first_tag("album").is_none() {
            issues.push(Issue {
                kind: IssueKind::MissingAlbum,
                detail: "track cannot be attached to an album".into(),
                files: vec![path.clone()],
            });
        }
        if track.duration_ms.is_none() || track.duration_ms == Some(0) {
            issues.push(Issue {
                kind: IssueKind::MissingDuration,
                detail: "duration not determined: file possibly corrupted".into(),
                files: vec![path.clone()],
            });
        }
        if track.track_no.is_none() {
            issues.push(Issue {
                kind: IssueKind::MissingTrackNumber,
                detail: "playback order not guaranteed within the album".into(),
                files: vec![path.clone()],
            });
        }

        let year = file.first_tag("date").and_then(text::extract_year);
        match year {
            None => issues.push(Issue {
                kind: IssueKind::MissingDate,
                detail: "no usable year".into(),
                files: vec![path],
            }),
            Some(year) if !(1860..=2100).contains(&year) => issues.push(Issue {
                kind: IssueKind::SuspiciousYear,
                detail: format!("year announced: {year}"),
                files: vec![path],
            }),
            Some(_) => {}
        }
    }
}

/// Reports the files whose stored checksums did not match.
///
/// Only what `aede check` has already established: this never reads a file, so
/// `doctor` stays instant. Files with no verdict are silently passed over here
/// and counted by the command itself, which can say how to obtain one.
fn check_integrity(catalog: &Catalog, issues: &mut Vec<Issue>) {
    for file in &catalog.files {
        if let Some(record) = &file.integrity
            && let crate::audit::integrity::Verdict::Damaged { detail } = &record.verdict
        {
            issues.push(Issue {
                kind: IssueKind::DamagedAudio,
                detail: format!("{detail} — restore from a backup or rip again"),
                files: vec![file.path.clone()],
            });
        }
    }
}

/// Reports what an imported analysis found and Aède cannot see.
///
/// An MD5 mismatch is an **error**, even when Aède's own check said the file
/// was intact: the two look at different things. The frame checksums prove the
/// container was not corrupted; the MD5 proves the decoded audio is the audio
/// that was encoded. A file can pass the first and fail the second — a stream
/// re-encoded by a non-conforming tool — and that case is exactly what Aède
/// cannot see before it decodes anything itself.
fn check_imported_analyses(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let files: BTreeMap<&str, &model::AudioFile> =
        catalog.files.iter().map(|f| (f.path.as_str(), f)).collect();
    for record in &catalog.analyses {
        // An analysis of a file the catalog does not hold is not a problem to
        // report: it is waiting for the folder it speaks of to be scanned.
        let Some(file) = files.get(record.path.as_str()) else {
            continue;
        };
        // An analysis of bytes that changed since says nothing about the file
        // that is there now.
        if !record.still_applies(file.size, file.mtime) {
            continue;
        }

        if record.md5_failed() {
            let contradiction = matches!(
                file.integrity.as_ref().map(|r| &r.verdict),
                Some(crate::audit::integrity::Verdict::Intact)
            );
            let detail = if contradiction {
                format!(
                    "{} decoded the audio and it does not match the file's own MD5, \
                     although the frame checksums are valid: the stream was re-encoded",
                    record.source
                )
            } else {
                format!(
                    "{} found the audio does not match the file's own MD5",
                    record.source
                )
            };
            issues.push(Issue {
                kind: IssueKind::Md5Mismatch,
                detail,
                files: vec![file.path.clone()],
            });
        }

        // The spectral verdicts — transcoding, upscaling, upsampling — are read
        // from the report, stored and kept up to date, and deliberately not
        // reported here. See `analysis::FileAnalysis::suspect_encoding`, which
        // nothing calls on purpose.
    }
}

/// Reports the albums the model linked to a copy of themselves.
///
/// Only the copies that are identical in quality: a hi-res edition beside the
/// CD rip is a deliberate second copy and not something to fix.
fn check_duplicate_albums(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let mut seen: std::collections::BTreeSet<(Id, Id)> = Default::default();
    for release in &catalog.releases {
        for other_id in catalog.related_releases(release.id, model::DUPLICATE) {
            let pair = (release.id.min(other_id), release.id.max(other_id));
            if !seen.insert(pair) {
                continue;
            }
            let Some(other) = catalog.release(other_id) else {
                continue;
            };
            // The smaller of the two is what deleting one would free; both are
            // named, since only the user knows which folder to keep.
            let wasted = release_size(catalog, release.id).min(release_size(catalog, other_id));
            issues.push(Issue {
                kind: IssueKind::DuplicateAlbum,
                detail: format!(
                    "\"{}\" is present twice in the same quality ({} recoverable)",
                    release.title,
                    text::format_size(wasted)
                ),
                files: vec![release.folder.clone(), other.folder.clone()],
            });
        }
    }
}

/// `true` when the tracks all come from releases the model already tied
/// together, whichever way.
///
/// A copied album would otherwise be reported once per track, and so would a
/// hi-res edition sitting beside its CD rip — which is not a defect at all. The
/// album-level lines say both things once, and name the folders.
fn covered_by_the_album(catalog: &Catalog, cluster: &[(Id, u64)]) -> bool {
    let releases: Vec<Id> = cluster
        .iter()
        .filter_map(|&(id, _)| catalog.track(id))
        .filter_map(|t| t.release_id)
        .collect();
    if releases.len() != cluster.len() || releases.len() < 2 {
        return false;
    }
    let first = releases[0];
    let linked: std::collections::BTreeSet<Id> = catalog
        .related_releases(first, model::DUPLICATE)
        .into_iter()
        .chain(catalog.related_releases(first, model::OTHER_EDITION))
        .collect();
    releases
        .iter()
        .skip(1)
        .all(|other| *other == first || linked.contains(other))
}

/// Notes the albums kept in two encodings. Not a defect: a hi-res copy beside
/// the CD rip is a choice, and saying nothing at all would look like an
/// oversight next to the duplicate lines.
fn check_other_editions(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let mut seen: std::collections::BTreeSet<(Id, Id)> = Default::default();
    for release in &catalog.releases {
        for other_id in catalog.related_releases(release.id, model::OTHER_EDITION) {
            let pair = (release.id.min(other_id), release.id.max(other_id));
            if !seen.insert(pair) {
                continue;
            }
            let Some(other) = catalog.release(other_id) else {
                continue;
            };
            issues.push(Issue {
                kind: IssueKind::OtherEdition,
                detail: format!(
                    "\"{}\" is kept in two encodings ({} and {})",
                    release.title,
                    quality_summary(catalog, release.id),
                    quality_summary(catalog, other_id)
                ),
                files: vec![release.folder.clone(), other.folder.clone()],
            });
        }
    }
}

/// The formats a release is made of, in one short phrase.
fn quality_summary(catalog: &Catalog, release_id: Id) -> String {
    let labels: std::collections::BTreeSet<String> = catalog
        .release(release_id)
        .map(|r| {
            r.track_ids
                .iter()
                .filter_map(|&id| catalog.track(id))
                .filter_map(|t| catalog.file(t.file_id))
                .map(|f| f.properties.quality_label())
                .collect()
        })
        .unwrap_or_default();
    labels.into_iter().collect::<Vec<_>>().join(", ")
}

fn release_size(catalog: &Catalog, release_id: Id) -> u64 {
    catalog
        .release(release_id)
        .map(|r| {
            r.track_ids
                .iter()
                .filter_map(|&id| catalog.track(id))
                .filter_map(|t| catalog.file(t.file_id))
                .map(|f| f.size)
                .sum()
        })
        .unwrap_or(0)
}

/// Two tracks are considered duplicates if the same artist and the same title
/// come back with a close duration (less than 3 seconds apart).
///
/// The duration is decisive: without it, a live rendition would be reported as
/// a duplicate of the studio version.
fn check_duplicates(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let mut groups: BTreeMap<(String, String), Vec<(Id, u64)>> = BTreeMap::new();

    for track in &catalog.tracks {
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
        let artist = file.first_tag("artist").unwrap_or("");
        let key = (text::normalize(artist), text::normalize(&track.title));
        if key.1.is_empty() {
            continue;
        }
        groups
            .entry(key)
            .or_default()
            .push((track.id, track.duration_ms.unwrap_or(0)));
    }

    for ((artist, title), mut entries) in groups {
        if entries.len() < 2 {
            continue;
        }
        entries.sort_by_key(|&(_, duration)| duration);

        // Tracks whose durations follow each other closely are grouped.
        let mut cluster: Vec<(Id, u64)> = vec![entries[0]];
        let mut clusters: Vec<Vec<(Id, u64)>> = Vec::new();
        for &entry in &entries[1..] {
            let last = cluster.last().map(|&(_, d)| d).unwrap_or(0);
            if entry.1.abs_diff(last) <= 3_000 {
                cluster.push(entry);
            } else {
                clusters.push(std::mem::take(&mut cluster));
                cluster = vec![entry];
            }
        }
        clusters.push(cluster);

        for cluster in clusters.into_iter().filter(|c| c.len() > 1) {
            // A copied album would otherwise be reported once per track: the
            // album-level issue already names the two folders, and thirteen
            // lines saying the same thing bury everything else.
            if covered_by_the_album(catalog, &cluster) {
                continue;
            }
            let files: Vec<String> = cluster
                .iter()
                .filter_map(|&(id, _)| catalog.track(id))
                .filter_map(|t| catalog.file(t.file_id))
                .map(|f| f.path.clone())
                .collect();
            let wasted: u64 = cluster
                .iter()
                .skip(1)
                .filter_map(|&(id, _)| catalog.track(id))
                .filter_map(|t| catalog.file(t.file_id))
                .map(|f| f.size)
                .sum();
            issues.push(Issue {
                kind: IssueKind::DuplicateTrack,
                detail: format!(
                    "\"{title}\" by \"{artist}\" present {} times ({} recoverable)",
                    cluster.len(),
                    text::format_size(wasted)
                ),
                files,
            });
        }
    }
}

/// What names an album across the folders it may be spread over.
///
/// Deliberately *not* the release identity, which includes the folder — that is
/// what tells a CD rip from a vinyl rip of the same record, and it is exactly
/// the part that has to be dropped here to see the two halves of one set.
fn album_key(release: &model::Release) -> (String, Option<Id>) {
    (release.key.clone(), release.album_artist_id)
}

/// Every disc the library holds of each album, wherever it sits on disk.
fn discs_by_album(catalog: &Catalog) -> BTreeMap<(String, Option<Id>), BTreeSet<u32>> {
    let mut out: BTreeMap<(String, Option<Id>), BTreeSet<u32>> = BTreeMap::new();
    for release in &catalog.releases {
        let discs = out.entry(album_key(release)).or_default();
        for track in release.track_ids.iter().filter_map(|&id| catalog.track(id)) {
            discs.insert(track.disc_no.unwrap_or(1));
        }
    }
    out
}

fn check_releases(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let discs_elsewhere = discs_by_album(catalog);
    for release in &catalog.releases {
        let tracks: Vec<_> = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .collect();
        if tracks.is_empty() {
            continue;
        }
        let paths: Vec<String> = tracks
            .iter()
            .filter_map(|t| catalog.file(t.file_id))
            .map(|f| f.path.clone())
            .collect();

        // Gaps in the numbering, disc by disc — and what the files themselves
        // say the disc should hold, which is the only way to see the tracks
        // missing from the *end*. On gaps alone, an album truncated after track
        // 9 of 12 looks perfectly whole: there is nothing between 1 and 9 left
        // to be missing. That is exactly the shape an interrupted rip has, so
        // the check was blind to its most common case.
        let mut by_disc: BTreeMap<u32, (Vec<u32>, u32)> = BTreeMap::new();
        for track in &tracks {
            if let Some(no) = track.track_no {
                let entry = by_disc.entry(track.disc_no.unwrap_or(1)).or_default();
                entry.0.push(no);
                if let Some(total) = catalog.file(track.file_id).and_then(announced_total) {
                    entry.1 = entry.1.max(total);
                }
            }
        }
        for (disc, (mut numbers, announced)) in by_disc {
            numbers.sort_unstable();
            numbers.dedup();
            let Some(&last) = numbers.last() else {
                continue;
            };
            // A total smaller than what is present is a wrong tag, not a
            // missing track, so the ceiling is whichever of the two is larger.
            let max = last.max(announced);
            let missing: Vec<u32> = (1..=max).filter(|n| !numbers.contains(n)).collect();
            if !missing.is_empty() {
                let list: Vec<String> = missing.iter().take(12).map(|n| n.to_string()).collect();
                issues.push(Issue {
                    kind: IssueKind::IncompleteAlbum,
                    detail: format!(
                        "\"{}\" disc {disc}: missing tracks {}{}",
                        release.title,
                        list.join(", "),
                        if missing.len() > 12 { "…" } else { "" }
                    ),
                    files: paths.clone(),
                });
            }
        }

        // A whole disc missing, which the check above cannot see: every disc
        // that is there is complete, and nothing in the numbering says how many
        // there should be. A four-disc soundtrack ripped as three looks perfect
        // until the day it is played. `disctotal` answers it, and so does a
        // hole in the disc numbers themselves.
        let present: BTreeSet<u32> = tracks.iter().map(|t| t.disc_no.unwrap_or(1)).collect();
        let announced = tracks
            .iter()
            .filter_map(|t| catalog.file(t.file_id))
            .filter_map(announced_discs)
            .max();
        // Same rule as the tracks: a total smaller than what is here is a wrong
        // tag rather than a missing disc.
        let ceiling = present
            .iter()
            .copied()
            .max()
            .unwrap_or(1)
            .max(announced.unwrap_or(0));
        let sibling = discs_elsewhere.get(&album_key(release));
        let missing: Vec<u32> = (1..=ceiling)
            .filter(|n| !present.contains(n))
            // Not missing, merely somewhere else: a set laid out as two folders
            // the disc-folder rule does not recognise — "Album CD1" beside
            // "Album CD2" rather than under a common parent — makes two
            // releases, each of which would otherwise report the other as
            // missing. Look for it in the library before calling it lost.
            .filter(|n| !sibling.is_some_and(|discs| discs.contains(n)))
            .collect();
        if !missing.is_empty() {
            let list: Vec<String> = missing.iter().take(12).map(|n| n.to_string()).collect();
            issues.push(Issue {
                kind: IssueKind::IncompleteAlbum,
                detail: format!(
                    "\"{}\": missing {} {}{}{}",
                    release.title,
                    if missing.len() > 1 { "discs" } else { "disc" },
                    list.join(", "),
                    if missing.len() > 12 { "…" } else { "" },
                    match announced {
                        Some(total) => format!(" of {total}"),
                        None => String::new(),
                    }
                ),
                files: paths.clone(),
            });
        }

        // A mix of codecs or sample rates within a single album: the sign of an
        // album pieced together from different sources.
        let files: Vec<_> = tracks
            .iter()
            .filter_map(|t| catalog.file(t.file_id))
            .collect();
        let codecs: std::collections::BTreeSet<&str> =
            files.iter().map(|f| f.properties.codec.as_str()).collect();
        let rates: std::collections::BTreeSet<u32> = files
            .iter()
            .filter_map(|f| f.properties.sample_rate)
            .collect();
        if codecs.len() > 1 || rates.len() > 1 {
            let mut details = Vec::new();
            if codecs.len() > 1 {
                details.push(format!(
                    "codecs: {}",
                    codecs.iter().copied().collect::<Vec<_>>().join(", ")
                ));
            }
            if rates.len() > 1 {
                details.push(format!(
                    "sample rates: {}",
                    rates
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            issues.push(Issue {
                kind: IssueKind::MixedQuality,
                detail: format!("\"{}\" — {}", release.title, details.join("; ")),
                files: paths.clone(),
            });
        }

        // Cover art: embedded, or dropped in the folder.
        let has_cover = release.cover_path.is_some() || files.iter().any(|f| f.has_embedded_art);
        if !has_cover {
            issues.push(Issue {
                kind: IssueKind::MissingCover,
                detail: format!("\"{}\" has no cover art", release.title),
                files: vec![release.folder.clone()],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{self, IntegrityRecord, ScannedFile};
    use crate::tags::RawTags;

    fn file(
        path: &str,
        fields: &[(&str, &str)],
        duration: Option<u64>,
        codec: &str,
    ) -> ScannedFile {
        let mut tags = RawTags::default();
        for (k, v) in fields {
            tags.insert(k, *v);
        }
        tags.properties.codec = codec.into();
        tags.properties.lossless = codec == "flac";
        tags.properties.sample_rate = Some(44_100);
        tags.properties.duration_ms = duration;
        ScannedFile {
            path: path.into(),
            size: 5_000_000,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }
    }

    fn count(issues: &[Issue], kind: IssueKind) -> usize {
        issues.iter().filter(|i| i.kind == kind).count()
    }

    #[test]
    fn detects_missing_tags() {
        let c = model::build(
            vec![file("/m/a/01 no tags.flac", &[], Some(1000), "flac")],
            vec!["/m".into()],
            0,
        );
        let issues = diagnose(&c);
        assert_eq!(count(&issues, IssueKind::MissingTitle), 1);
        assert_eq!(count(&issues, IssueKind::MissingArtist), 1);
        assert_eq!(count(&issues, IssueKind::MissingAlbum), 1);
        assert_eq!(count(&issues, IssueKind::MissingDate), 1);
    }

    #[test]
    fn detects_an_unreadable_duration() {
        let c = model::build(
            vec![file(
                "/m/a/01.flac",
                &[
                    ("title", "X"),
                    ("artist", "Y"),
                    ("album", "Z"),
                    ("date", "2000"),
                ],
                None,
                "flac",
            )],
            vec!["/m".into()],
            0,
        );
        let issues = diagnose(&c);
        assert_eq!(count(&issues, IssueKind::MissingDuration), 1);
        assert_eq!(
            issues
                .iter()
                .find(|i| i.kind == IssueKind::MissingDuration)
                .unwrap()
                .severity(),
            Severity::Error
        );
    }

    #[test]
    fn a_copied_album_is_reported_once_and_not_per_track() {
        // The same three tracks in two folders. Before, this produced three
        // "likely duplicate" lines saying the same thing.
        let common = |n: &'static str, title: &'static str| {
            vec![
                ("title", title),
                ("artist", "Danzig"),
                ("albumartist", "Danzig"),
                ("album", "Danzig 4"),
                ("date", "1994"),
                ("tracknumber", n),
            ]
        };
        let mut files = Vec::new();
        for folder in ["/m/A", "/m/B"] {
            for (n, title) in [
                ("1", "Brand New God"),
                ("2", "Little Whip"),
                ("3", "Cantspeak"),
            ] {
                files.push(file(
                    &format!("{folder}/{n}.flac"),
                    &common(n, title),
                    Some(100_000 + n.parse::<u64>().unwrap_or(0) * 1000),
                    "flac",
                ));
            }
        }
        let c = model::build(files, vec!["/m".into()], 0);
        let issues = diagnose(&c);
        assert_eq!(count(&issues, IssueKind::DuplicateAlbum), 1);
        assert_eq!(
            count(&issues, IssueKind::DuplicateTrack),
            0,
            "the album line already says it"
        );
        let album = issues
            .iter()
            .find(|i| i.kind == IssueKind::DuplicateAlbum)
            .unwrap();
        assert_eq!(album.files.len(), 2, "both folders are named");
        assert_eq!(album.severity(), Severity::Warning);
    }

    #[test]
    fn detects_a_duplicate_but_not_a_live_version() {
        let common = |album: &'static str| {
            vec![
                ("title", "So What"),
                ("artist", "Miles Davis"),
                ("album", album),
                ("date", "1959"),
            ]
        };
        let c = model::build(
            vec![
                file(
                    "/m/a/01.flac",
                    &common("Kind of Blue"),
                    Some(545_000),
                    "flac",
                ),
                // Near-identical copy: duplicate.
                file("/m/b/01.mp3", &common("Best of"), Some(546_000), "mp3"),
                // Live version, markedly longer: not a duplicate.
                file("/m/c/01.flac", &common("Live"), Some(900_000), "flac"),
            ],
            vec!["/m".into()],
            0,
        );
        let issues = diagnose(&c);
        let duplicates: Vec<&Issue> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::DuplicateTrack)
            .collect();
        assert_eq!(duplicates.len(), 1, "only one duplicate group expected");
        assert_eq!(duplicates[0].files.len(), 2);
    }

    #[test]
    fn an_album_cut_short_at_the_end_is_incomplete_too() {
        // Counting gaps alone, an album truncated after track 2 of 5 looks
        // whole: there is nothing between 1 and 2 left to be missing. That is
        // the shape an interrupted rip has, and the check was blind to it.
        let track = |n: &'static str| {
            vec![
                ("title", "T"),
                ("artist", "A"),
                ("album", "Album"),
                ("date", "1990"),
                ("tracknumber", n),
                ("tracktotal", "5"),
            ]
        };
        let c = model::build(
            vec![
                file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
                file("/m/a/02.flac", &track("2"), Some(2000), "flac"),
            ],
            vec!["/m".into()],
            0,
        );
        let gap = diagnose(&c)
            .into_iter()
            .find(|i| i.kind == IssueKind::IncompleteAlbum)
            .expect("the missing tail must be seen");
        for n in ["3", "4", "5"] {
            assert!(gap.detail.contains(n), "detail: {}", gap.detail);
        }
    }

    #[test]
    fn a_total_smaller_than_what_is_there_is_a_wrong_tag_not_a_gap() {
        // A disc announcing 2 while holding 3 has a bad tag; inventing missing
        // tracks from it would report a defect that is not one.
        let track = |n: &'static str| {
            vec![
                ("title", "T"),
                ("artist", "A"),
                ("album", "Album"),
                ("date", "1990"),
                ("tracknumber", n),
                ("tracktotal", "2"),
            ]
        };
        let c = model::build(
            vec![
                file("/m/b/01.flac", &track("1"), Some(1000), "flac"),
                file("/m/b/02.flac", &track("2"), Some(2000), "flac"),
                file("/m/b/03.flac", &track("3"), Some(3000), "flac"),
            ],
            vec!["/m".into()],
            0,
        );
        assert!(
            !diagnose(&c)
                .iter()
                .any(|i| i.kind == IssueKind::IncompleteAlbum),
            "nothing is missing here"
        );
    }

    /// One disc of a set, complete in itself.
    fn disc(path: &'static str, disc: &'static str, total: &'static str) -> ScannedFile {
        file(
            path,
            &[
                ("title", "T"),
                ("artist", "A"),
                ("album", "Box"),
                ("date", "1997"),
                ("tracknumber", "1"),
                ("tracktotal", "1"),
                ("discnumber", disc),
                ("disctotal", total),
            ],
            Some(1000),
            "flac",
        )
    }

    #[test]
    fn a_set_missing_a_whole_disc_is_incomplete() {
        // Every disc that is there is complete, so the gap check sees nothing:
        // a four-disc soundtrack ripped as three looks perfect until the day
        // it is played. Only the announced total can say otherwise.
        let c = model::build(
            vec![
                disc("/m/box/Disc 1/01.flac", "1", "4"),
                disc("/m/box/Disc 2/01.flac", "2", "4"),
                disc("/m/box/Disc 3/01.flac", "3", "4"),
            ],
            vec!["/m".into()],
            0,
        );
        let issue = diagnose(&c)
            .into_iter()
            .find(|i| i.kind == IssueKind::IncompleteAlbum)
            .expect("the missing disc must be seen");
        assert_eq!(issue.detail, "\"Box\": missing disc 4 of 4");
    }

    #[test]
    fn a_hole_in_the_disc_numbers_needs_no_total() {
        // Nothing announces four here; discs 1 and 3 alone are enough to say
        // that the second is not on the shelf.
        let c = model::build(
            vec![
                disc("/m/box/Disc 1/01.flac", "1", ""),
                disc("/m/box/Disc 3/01.flac", "3", ""),
            ],
            vec!["/m".into()],
            0,
        );
        let issue = diagnose(&c)
            .into_iter()
            .find(|i| i.kind == IssueKind::IncompleteAlbum)
            .expect("the hole must be seen");
        assert_eq!(issue.detail, "\"Box\": missing disc 2");
    }

    #[test]
    fn a_complete_set_and_a_plain_album_say_nothing() {
        // The check has to be silent on the ordinary case, or it becomes the
        // line everybody learns to skip.
        let c = model::build(
            vec![
                disc("/m/box/Disc 1/01.flac", "1", "2"),
                disc("/m/box/Disc 2/01.flac", "2", "2"),
                file(
                    "/m/plain/01.flac",
                    &[
                        ("title", "T"),
                        ("artist", "A"),
                        ("album", "Plain"),
                        ("date", "1990"),
                        ("tracknumber", "1"),
                        ("tracktotal", "1"),
                    ],
                    Some(1000),
                    "flac",
                ),
            ],
            vec!["/m".into()],
            0,
        );
        assert!(
            !diagnose(&c)
                .iter()
                .any(|i| i.kind == IssueKind::IncompleteAlbum),
            "nothing is missing in either"
        );
    }

    #[test]
    fn a_disc_in_a_folder_of_its_own_is_not_a_disc_that_is_missing() {
        // "Box CD1" beside "Box CD2" is a layout the disc-folder rule does not
        // recognise — the names are not bare disc folders — so the set arrives
        // as two releases. Each announces two discs and holds one, and each
        // would report the other as missing while it sits right there.
        let c = model::build(
            vec![
                disc("/m/Box CD1/01.flac", "1", "2"),
                disc("/m/Box CD2/01.flac", "2", "2"),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(c.releases.len(), 2, "two folders, two releases");
        assert!(
            !diagnose(&c)
                .iter()
                .any(|i| i.kind == IssueKind::IncompleteAlbum),
            "both discs are in the library: {:#?}",
            diagnose(&c)
                .iter()
                .filter(|i| i.kind == IssueKind::IncompleteAlbum)
                .map(|i| i.detail.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn detects_an_incomplete_album() {
        let track = |n: &'static str| {
            vec![
                ("title", "T"),
                ("artist", "A"),
                ("album", "Album"),
                ("date", "1990"),
                ("tracknumber", n),
            ]
        };
        let c = model::build(
            vec![
                file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
                file("/m/a/03.flac", &track("3"), Some(2000), "flac"),
            ],
            vec!["/m".into()],
            0,
        );
        let issues = diagnose(&c);
        let gap = issues
            .iter()
            .find(|i| i.kind == IssueKind::IncompleteAlbum)
            .unwrap();
        assert!(gap.detail.contains('2'), "detail: {}", gap.detail);
    }

    #[test]
    fn detects_mixed_quality() {
        let track = |n: &'static str| {
            vec![
                ("title", "T"),
                ("artist", "A"),
                ("album", "Album"),
                ("date", "1990"),
                ("tracknumber", n),
            ]
        };
        let c = model::build(
            vec![
                file("/m/a/01.flac", &track("1"), Some(1000), "flac"),
                file("/m/a/02.mp3", &track("2"), Some(2000), "mp3"),
            ],
            vec!["/m".into()],
            0,
        );
        assert_eq!(count(&diagnose(&c), IssueKind::MixedQuality), 1);
    }

    #[test]
    fn healthy_library_reports_nothing() {
        let track = |title: &'static str, n: &'static str| {
            vec![
                ("title", title),
                ("artist", "A"),
                ("album", "Album"),
                ("date", "1990"),
                ("tracknumber", n),
            ]
        };
        let mut a = file("/m/a/01.flac", &track("Overture", "1"), Some(1000), "flac");
        let mut b = file("/m/a/02.flac", &track("Finale", "2"), Some(2000), "flac");
        a.tags.has_embedded_art = true;
        b.tags.has_embedded_art = true;
        let c = model::build(vec![a, b], vec!["/m".into()], 0);
        assert!(diagnose(&c).is_empty(), "got: {:?}", diagnose(&c));
    }

    #[test]
    fn two_identical_tracks_are_not_silently_ignored() {
        // Two genuinely identical tracks must be reported — as tracks, or as
        // the album that holds them, but never passed over.
        let fields = vec![
            ("title", "Intro"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
        ];
        let c = model::build(
            vec![
                file("/m/a/01.flac", &fields, Some(30_000), "flac"),
                file("/m/b/01.flac", &fields, Some(30_500), "flac"),
            ],
            vec!["/m".into()],
            0,
        );
        let issues = diagnose(&c);
        let reported =
            count(&issues, IssueKind::DuplicateTrack) + count(&issues, IssueKind::DuplicateAlbum);
        assert_eq!(reported, 1, "reported once, one way or the other");
        assert!(
            issues
                .iter()
                .filter(|i| matches!(
                    i.kind,
                    IssueKind::DuplicateTrack | IssueKind::DuplicateAlbum
                ))
                .any(|i| i.files.len() == 2),
            "both copies are named"
        );
    }

    /// Builds a healthy one-file catalog to hang an imported analysis on.
    fn catalog_of_one() -> Catalog {
        let fields = vec![
            ("title", "Intro"),
            ("artist", "A"),
            ("album", "Album"),
            ("date", "1990"),
        ];
        let mut f = file("/m/a/01.flac", &fields, Some(30_000), "flac");
        f.tags.has_embedded_art = true;
        model::build(vec![f], vec!["/m".into()], 0)
    }

    fn analysis_of(c: &Catalog) -> crate::analysis::FileAnalysis {
        let f = &c.files[0];
        crate::analysis::FileAnalysis {
            path: f.path.clone(),
            source: "flaccompagnon".into(),
            source_version: 1,
            size_bytes: f.size,
            modified_unix: f.mtime,
            ..Default::default()
        }
    }

    #[test]
    fn a_failed_md5_is_an_error_even_when_the_checksums_passed() {
        // The two checks answer different questions: the frame CRCs prove the
        // container was not corrupted, the MD5 proves the audio is the audio
        // that was encoded. Passing the first and failing the second is not a
        // contradiction to arbitrate but a finding to report.
        let mut c = catalog_of_one();
        c.files[0].integrity = Some(IntegrityRecord {
            verdict: crate::audit::integrity::Verdict::Intact,
            method: crate::audit::integrity::FLAC_METHOD.into(),
            checked_at: 0,
        });
        c.analyses.push(crate::analysis::FileAnalysis {
            md5_state: Some("Mismatch".into()),
            ..analysis_of(&c)
        });

        let issues = diagnose(&c);
        assert_eq!(count(&issues, IssueKind::Md5Mismatch), 1);
        let issue = issues
            .iter()
            .find(|i| i.kind == IssueKind::Md5Mismatch)
            .unwrap();
        assert_eq!(issue.kind.severity(), Severity::Error);
        assert!(
            issue.detail.contains("re-encoded"),
            "the disagreement is explained, not hidden: {}",
            issue.detail
        );
        assert!(
            issue.detail.contains("flaccompagnon"),
            "and attributed: {}",
            issue.detail
        );
    }

    #[test]
    fn an_imported_verdict_dies_with_the_bytes_it_was_reached_on() {
        let mut c = catalog_of_one();
        let stale = crate::analysis::FileAnalysis {
            md5_state: Some("Mismatch".into()),
            transcoding: Some("detected".into()),
            size_bytes: c.files[0].size + 1,
            ..analysis_of(&c)
        };
        c.analyses.push(stale);
        let issues = diagnose(&c);
        assert_eq!(count(&issues, IssueKind::Md5Mismatch), 0);
    }

    #[test]
    fn a_spectral_verdict_is_stored_and_never_reported() {
        // The three detections another tool makes from the spectrum —
        // transcoding, upscaling, upsampling — are read, kept, and said
        // nowhere. They are heuristics whose own author hedges them ("could be
        // a naturally dark master"), and a report that restates a hedge as a
        // warning of its own has stopped describing the library and started
        // arguing about it. The measurements behind them stay on the file's
        // page, attributed, where the person who ran the tool can read them.
        let mut c = catalog_of_one();
        c.analyses.push(crate::analysis::FileAnalysis {
            transcoding: Some("detected".into()),
            upscaling: Some(true),
            upsampling: Some(true),
            detail: Some("cut at 16 kHz".into()),
            ..analysis_of(&c)
        });
        let issues = diagnose(&c);
        assert!(
            issues.iter().all(|i| !i.detail.contains("cut at 16 kHz")),
            "no line may carry the verdict: {issues:#?}"
        );
        for word in ["transcod", "upscal", "upsampl", "lossy"] {
            assert!(
                issues.iter().all(|i| !i.detail.contains(word)),
                "\"{word}\" must not appear in the report: {issues:#?}"
            );
        }
        // And the record itself is untouched, so the day the tool is trusted
        // the display is all there is to write back.
        assert!(c.analyses[0].suspect_encoding(), "still stored, still true");
    }

    #[test]
    fn an_analysis_of_a_file_the_catalog_does_not_hold_is_not_a_problem() {
        // It is waiting for its folder to be scanned, which is not a defect in
        // the library and must not be reported as one.
        let mut c = catalog_of_one();
        c.analyses.push(crate::analysis::FileAnalysis {
            path: "/elsewhere/never scanned.flac".into(),
            source: "flaccompagnon".into(),
            md5_state: Some("Mismatch".into()),
            transcoding: Some("detected".into()),
            ..Default::default()
        });
        assert!(diagnose(&c).is_empty(), "got: {:?}", diagnose(&c));
        assert_eq!(c.pending_analyses(), 1, "but it is counted as waiting");
    }

    #[test]
    fn a_clean_analysis_adds_nothing_to_report() {
        let mut c = catalog_of_one();
        c.analyses.push(crate::analysis::FileAnalysis {
            md5_state: Some("Match".into()),
            transcoding: Some("none".into()),
            upscaling: Some(false),
            upsampling: Some(false),
            ..analysis_of(&c)
        });
        assert!(diagnose(&c).is_empty(), "got: {:?}", diagnose(&c));
    }
}
