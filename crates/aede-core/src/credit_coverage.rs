//! Read-only coverage of MusicBrainz recording and work credits.
//!
//! A completed relationship lookup with no credits is an answer, not a gap.
//! Count canonical recordings rather than file placements so reissues do not
//! inflate the library-wide figures.

use crate::model::{Catalog, EntityKind, Id};
use crate::sources::{Facts, MUSICBRAINZ, Sources};
use crate::user::EntityRef;

/// What is known about an identified recording's external credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditStatus {
    /// No local MusicBrainz recording identifier to ask about.
    Unidentified,
    /// No completed MusicBrainz relationship lookup is attached.
    Waiting,
    /// A completed answer exists, but its source identity is not trusted.
    Untrusted,
    /// A trusted lookup completed and returned no direct or work credits.
    Empty,
    /// A trusted lookup returned at least one direct or work credit.
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

/// Classify every canonical recording, including those without an MBID.
pub fn recordings(catalog: &Catalog, sources: &Sources) -> Vec<RecordingCreditCoverage> {
    catalog
        .recordings
        .iter()
        .map(|recording| {
            let mut coverage = RecordingCreditCoverage {
                recording_id: recording.id,
                status: if recording.mbid.is_some() {
                    CreditStatus::Waiting
                } else {
                    CreditStatus::Unidentified
                },
                recording_credits: 0,
                work_credits: 0,
            };
            if recording.mbid.is_none() {
                return coverage;
            }
            for &track_id in &recording.track_ids {
                let Some(entity) = EntityRef::of(catalog, EntityKind::Track, track_id) else {
                    continue;
                };
                let Some(record) = sources.get(&entity, MUSICBRAINZ) else {
                    continue;
                };
                let Facts::Track(facts) = &record.facts else {
                    continue;
                };
                if !facts.relationships_complete {
                    continue;
                }
                if !sources.is_trusted(catalog, record) {
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
