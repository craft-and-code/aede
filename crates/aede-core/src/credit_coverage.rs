//! Read-only coverage of MusicBrainz recording, work and edition credits.
//!
//! A completed relationship lookup with no credits is an answer, not a gap.
//! Count canonical recordings rather than file placements so reissues do not
//! inflate the recording figures. Each local edition has its own lookup;
//! manual edition corrections are counted without pretending it was queried.

use std::collections::BTreeMap;

use crate::model::{Catalog, EntityKind, Id};
use crate::sources::{Facts, MUSICBRAINZ, ReviewDecision, Sources, trusted_with_identity};
use crate::user::EntityRef;

/// What is known about a recording or edition's external credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditStatus {
    /// No usable local MusicBrainz identifier to ask about.
    Unidentified,
    /// No completed MusicBrainz relationship lookup is attached.
    Waiting,
    /// A completed answer exists, but its source identity is not trusted.
    Untrusted,
    /// A trusted lookup completed and returned no credits in its scope.
    Empty,
    /// A trusted lookup returned at least one credit in its scope.
    Credited,
}

impl CreditStatus {
    /// Stable machine-readable spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unidentified => "unidentified",
            Self::Waiting => "waiting",
            Self::Untrusted => "untrusted",
            Self::Empty => "empty",
            Self::Credited => "credited",
        }
    }
}

/// One canonical recording's coverage and the scope of its sourced credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingCreditCoverage {
    /// Canonical recording in this catalog.
    pub recording_id: Id,
    /// Current external-credit state.
    pub status: CreditStatus,
    /// Direct recording relationships in the trusted answer.
    pub recording_credits: usize,
    /// Work-level relationships in the trusted answer.
    pub work_credits: usize,
}

/// One local edition's external lookup and separately supplied corrections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditionCreditCoverage {
    /// Local release edition in this catalog.
    pub release_id: Id,
    /// MusicBrainz lookup state; manual corrections do not complete a lookup.
    pub status: CreditStatus,
    /// Edition relationships in the trusted completed MusicBrainz answer.
    pub edition_credits: usize,
    /// Trusted manual edition corrections, independent of lookup completion.
    pub manual_credits: usize,
}

/// Classify local editions separately from their canonical recordings.
///
/// Only an exact, completed and trusted MusicBrainz answer supplies the lookup
/// status. A manual correction uses its explicit local scope, never completes
/// an external lookup, and cannot supply recording or work coverage. Counts
/// describe the retained answers; excluding an individual relationship does
/// not erase the evidence that an external lookup was completed.
pub fn editions(catalog: &Catalog, sources: &Sources) -> Vec<EditionCreditCoverage> {
    let records: BTreeMap<_, _> = sources
        .records
        .iter()
        .filter(|record| matches!(&record.facts, Facts::Release(_)))
        .map(|record| ((record.key.as_str(), record.source.as_str()), record))
        .collect();
    let reviews = reviews_by_claim(sources, EntityKind::Release);
    catalog
        .releases
        .iter()
        .map(|release| {
            let local_id = usable_id(release.mbid.as_deref());
            let mut coverage = EditionCreditCoverage {
                release_id: release.id,
                status: if local_id.is_some() {
                    CreditStatus::Waiting
                } else {
                    CreditStatus::Unidentified
                },
                edition_credits: 0,
                manual_credits: 0,
            };
            let Some(entity) = EntityRef::of(catalog, EntityKind::Release, release.id) else {
                return coverage;
            };
            let group_id = release
                .release_group_id
                .and_then(|id| catalog.release_group(id))
                .map(|group| group.mbid.as_str());
            for source in [MUSICBRAINZ, "manual"] {
                let Some(record) = records.get(&(entity.key.as_str(), source)) else {
                    continue;
                };
                let Facts::Release(facts) = &record.facts else {
                    continue;
                };
                let exact_edition =
                    usable_id(facts.edition_mbid.as_deref()).is_some_and(|id| Some(id) == local_id);
                let review = reviews
                    .get(&(
                        record.key.as_str(),
                        record.source.as_str(),
                        record.source_id.as_deref(),
                    ))
                    .copied();
                let trusted = trusted_with_identity(record, group_id, review);
                if source == "manual" {
                    if trusted && (facts.edition_mbid.is_none() || exact_edition) {
                        coverage.manual_credits = facts.credits.len();
                    }
                } else if exact_edition && facts.relationships_complete {
                    if !trusted {
                        coverage.status = CreditStatus::Untrusted;
                    } else {
                        coverage.edition_credits = facts.credits.len();
                        coverage.status = if facts.credits.is_empty() {
                            CreditStatus::Empty
                        } else {
                            CreditStatus::Credited
                        };
                    }
                }
            }
            coverage
        })
        .collect()
}

fn usable_id(id: Option<&str>) -> Option<&str> {
    id.filter(|id| !id.trim().is_empty())
}

fn reviews_by_claim(
    sources: &Sources,
    kind: EntityKind,
) -> BTreeMap<(&str, &str, Option<&str>), ReviewDecision> {
    let mut result = BTreeMap::new();
    for review in sources
        .reviews
        .iter()
        .filter(|review| review.entity.kind == kind)
    {
        // Keep the first matching decision, exactly as Sources::review_for.
        result
            .entry((
                review.entity.key.as_str(),
                review.source.as_str(),
                review.source_id.as_deref(),
            ))
            .or_insert(review.decision);
    }
    result
}

/// Classify every canonical recording, including those without an MBID.
pub fn recordings(catalog: &Catalog, sources: &Sources) -> Vec<RecordingCreditCoverage> {
    let records_by_path: BTreeMap<&str, &crate::sources::SourceRecord> = sources
        .records
        .iter()
        .filter(|record| record.source == MUSICBRAINZ && matches!(&record.facts, Facts::Track(_)))
        .map(|record| (record.key.as_str(), record))
        .collect();
    let reviews = reviews_by_claim(sources, EntityKind::Track);
    catalog
        .recordings
        .iter()
        .map(|recording| {
            let local_id = usable_id(recording.mbid.as_deref());
            let mut coverage = RecordingCreditCoverage {
                recording_id: recording.id,
                status: if local_id.is_some() {
                    CreditStatus::Waiting
                } else {
                    CreditStatus::Unidentified
                },
                recording_credits: 0,
                work_credits: 0,
            };
            if local_id.is_none() {
                return coverage;
            }
            for &track_id in &recording.track_ids {
                let Some(entity) = EntityRef::of(catalog, EntityKind::Track, track_id) else {
                    continue;
                };
                let Some(record) = records_by_path.get(entity.key.as_str()) else {
                    continue;
                };
                let Facts::Track(facts) = &record.facts else {
                    continue;
                };
                if !facts.relationships_complete {
                    continue;
                }
                let review = reviews
                    .get(&(
                        record.key.as_str(),
                        record.source.as_str(),
                        record.source_id.as_deref(),
                    ))
                    .copied();
                if !trusted_with_identity(record, local_id, review) {
                    if coverage.status == CreditStatus::Waiting {
                        coverage.status = CreditStatus::Untrusted;
                    }
                    continue;
                }
                let work_credits = facts.works.iter().map(|work| work.credits.len()).sum();
                let direct_credits = facts.credits.len();
                // Several local placements can carry the same recording. A
                // source answer is a snapshot, not an additive list of rows.
                // Prefer the most informative trusted completed snapshot.
                if coverage.status != CreditStatus::Credited
                    || direct_credits + work_credits
                        > coverage.recording_credits + coverage.work_credits
                {
                    coverage.recording_credits = direct_credits;
                    coverage.work_credits = work_credits;
                    coverage.status = if direct_credits + work_credits == 0 {
                        CreditStatus::Empty
                    } else {
                        CreditStatus::Credited
                    };
                }
            }
            coverage
        })
        .collect()
}

#[cfg(test)]
#[path = "credit_coverage_tests.rs"]
mod tests;
