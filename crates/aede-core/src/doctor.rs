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
    /// The same audio, proved by fingerprint rather than guessed from tags.
    ///
    /// Stronger than [`IssueKind::DuplicateTrack`] and it catches what that
    /// cannot: two files whose tags say quite different things, or nothing at
    /// all, and whose sound is the same recording. A tag-based guess has to
    /// call itself "likely"; this one does not.
    SameAudio,
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
    /// A source and the tags say different things about the same release.
    ///
    /// Reported, never resolved. Which of the two is right is not something
    /// this program can know — a tag may be wrong, and so may MusicBrainz —
    /// and the whole reason a fetched value is kept beside the tag rather than
    /// on top of it is that the disagreement stays visible and the user
    /// decides.
    SourceDisagrees,
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
            | IssueKind::SameAudio
            | IssueKind::DuplicateAlbum
            | IssueKind::IncompleteAlbum
            | IssueKind::MixedQuality
            | IssueKind::SuspiciousYear => Severity::Warning,
            IssueKind::MissingDate
            | IssueKind::MissingTrackNumber
            | IssueKind::MissingCover
            | IssueKind::SourceDisagrees
            | IssueKind::OtherEdition => Severity::Info,
        }
    }

    /// Wording of the problem in one short phrase, ready to head a report line.
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::SourceDisagrees => "source disagrees",
            IssueKind::MissingTitle => "missing title",
            IssueKind::MissingArtist => "missing artist",
            IssueKind::MissingAlbum => "missing album",
            IssueKind::MissingDate => "missing year",
            IssueKind::MissingTrackNumber => "missing track number",
            IssueKind::MissingDuration => "unreadable duration",
            IssueKind::DuplicateTrack => "likely duplicate",
            // Not "likely": the audio is the same audio. The word is what
            // separates this from the guess above it, and a reader deciding
            // whether to delete a file needs to know which of the two they
            // are looking at.
            IssueKind::SameAudio => "the same audio",
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
pub fn diagnose(catalog: &Catalog, sources: &crate::sources::Sources) -> Vec<Issue> {
    let mut issues = Vec::new();
    check_tracks(catalog, &mut issues);
    check_integrity(catalog, &mut issues);
    check_imported_analyses(catalog, &mut issues);
    check_sources(catalog, sources, &mut issues);
    check_duplicate_albums(catalog, &mut issues);
    check_other_editions(catalog, &mut issues);
    check_duplicates(catalog, &mut issues);
    check_same_audio(catalog, &mut issues);
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
/// Where a source and the tags say different things about one release.
///
/// Only releases for now, and on purpose: an artist's country has no tag to
/// disagree with, so a fetched artist fact is an addition rather than a second
/// opinion. A release is where the two actually meet — Picard writes
/// `RELEASETYPE`, `DATE` and `LABEL` — and it is therefore the only place this
/// report has anything to say.
///
/// A record the catalog cannot place is skipped rather than reported, exactly
/// as a waiting analysis is: it is not a defect that MusicBrainz holds
/// something about an album nobody has scanned yet.
fn check_sources(catalog: &Catalog, sources: &crate::sources::Sources, issues: &mut Vec<Issue>) {
    use crate::sources::{Facts, Verdict};
    use crate::user::EntityRef;

    for record in &sources.records {
        let Facts::Release(facts) = &record.facts else {
            continue;
        };
        let entity: EntityRef = record.entity();
        let Some(release) = entity.resolve(catalog).and_then(|id| catalog.release(id)) else {
            continue;
        };

        // The first file of the release answers for its tags: `RELEASETYPE`
        // and `LABEL` belong to the edition, not to one track of it.
        let tag = |name: &str| -> Option<String> {
            release
                .track_ids
                .first()
                .and_then(|&t| catalog.track(t))
                .and_then(|t| catalog.file(t.file_id))
                .and_then(|f| f.first_tag(name).map(str::to_string))
        };

        let label_tag = release
            .label_ids
            .first()
            .and_then(|&id| catalog.label(id))
            .map(|l| l.name.clone());

        let comparisons = [
            (
                "release type",
                facts.primary_type.as_deref(),
                tag("releasetype"),
                false,
            ),
            (
                "date",
                facts.first_released.as_deref(),
                release.date.clone(),
                true,
            ),
            ("label", facts.label.as_deref(), label_tag, false),
        ];

        for (what, theirs, yours, is_date) in comparisons {
            let Some(theirs) = theirs else { continue };
            let verdict = match is_date {
                true => crate::sources::verdict_date(theirs, yours.as_deref()),
                false => crate::sources::verdict(theirs, yours.as_deref()),
            };
            let Verdict::Differs { theirs, yours } = verdict else {
                continue;
            };
            issues.push(Issue {
                kind: IssueKind::SourceDisagrees,
                detail: format!(
                    "\"{}\": {} says the {what} is \"{theirs}\", the tags say \"{yours}\"",
                    release.title, record.source
                ),
                files: release
                    .track_ids
                    .iter()
                    .filter_map(|&t| catalog.track(t))
                    .filter_map(|t| catalog.file(t.file_id))
                    .map(|f| f.path.clone())
                    .take(1)
                    .collect(),
            });
        }
    }
}

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
/// Files whose fingerprints are identical: the same audio, whatever the tags
/// say about it.
///
/// This is what a fingerprint buys beyond identifying a file, and it is worth
/// more than the identification for a library somebody has been keeping for
/// years: [`IssueKind::DuplicateTrack`] compares artist, title and duration,
/// so it can only find copies whose *tags* already agree. Two rips of one
/// track filed under different titles — or under none — are invisible to it
/// and obvious here.
///
/// Only files that have been fingerprinted take part, which is what makes the
/// report honest: a library where nothing has been fingerprinted reports no
/// identical audio, and that is a silence rather than a clean bill of health.
/// `aede fingerprint --full` is what fills it in.
fn check_same_audio(catalog: &Catalog, issues: &mut Vec<Issue>) {
    let mut groups: BTreeMap<&str, Vec<&crate::model::AudioFile>> = BTreeMap::new();
    for file in &catalog.files {
        if let Some(print) = &file.fingerprint {
            groups.entry(print.data.as_str()).or_default().push(file);
        }
    }
    for (_, files) in groups {
        if files.len() < 2 {
            continue;
        }
        // Everything after the first is what could go, which is the number a
        // reader is actually deciding about.
        let wasted: u64 = files.iter().skip(1).map(|f| f.size).sum();
        issues.push(Issue {
            kind: IssueKind::SameAudio,
            detail: format!(
                "{} files are the same recording ({} recoverable)",
                files.len(),
                text::format_size(wasted)
            ),
            files: files.iter().map(|f| f.path.clone()).collect(),
        });
    }
}

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
#[path = "doctor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "doctor_source_tests.rs"]
mod source_tests;
