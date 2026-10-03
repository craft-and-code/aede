//! Source-backed artist identities for contributors absent from local tags.
//!
//! These are read-only views over trusted credit assertions, not new catalog
//! artists. Grouping by MusicBrainz ID prevents two namesakes from becoming
//! one person and prevents a reissue from inventing another identity.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Catalog, Id};
use crate::sources::Sources;
use crate::text;

/// An artist credited on local recordings but not identified in local tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedContributor {
    /// MusicBrainz artist identity from the credited relationship.
    pub mbid: String,
    /// Canonical name reported by the most recent trusted credit.
    pub name: String,
    /// Other canonical or credited-as spellings carried by those credits.
    pub names: Vec<String>,
    /// Local recordings on which this person has a trusted credit.
    pub recording_ids: Vec<Id>,
    /// Local editions on which the artist has a direct edition credit.
    pub release_ids: Vec<Id>,
    /// Explicit work identities on which this person has a trusted credit.
    pub work_mbids: Vec<String>,
}

struct Builder {
    name: String,
    fetched_at: u64,
    names: BTreeSet<String>,
    recording_ids: BTreeSet<Id>,
    release_ids: BTreeSet<Id>,
    work_mbids: BTreeSet<String>,
}

/// Group trusted source credits into stable, non-local artist identities.
pub fn sourced(catalog: &Catalog, sources: &Sources) -> Vec<SourcedContributor> {
    let local_mbids: BTreeSet<&str> = catalog
        .artists
        .iter()
        .filter_map(|artist| artist.mbid.as_deref())
        .collect();
    let mut grouped: BTreeMap<String, Builder> = BTreeMap::new();
    for link in sources
        .credit_links(catalog)
        .into_iter()
        .filter(|link| link.trusted)
    {
        let mbid = link.credit.artist_mbid.trim();
        if mbid.is_empty() || local_mbids.contains(mbid) {
            continue;
        }
        let name = link.credit.artist_name.trim();
        let builder = grouped.entry(mbid.to_string()).or_insert_with(|| Builder {
            name: name.to_string(),
            fetched_at: link.fetched_at,
            names: BTreeSet::new(),
            recording_ids: BTreeSet::new(),
            release_ids: BTreeSet::new(),
            work_mbids: BTreeSet::new(),
        });
        if !name.is_empty() {
            builder.names.insert(name.to_string());
            if builder.name.is_empty()
                || link.fetched_at > builder.fetched_at
                || (link.fetched_at == builder.fetched_at && name < builder.name.as_str())
            {
                builder.name = name.to_string();
                builder.fetched_at = link.fetched_at;
            }
        }
        if let Some(credited_as) = link.credit.credited_as.as_deref()
            && !credited_as.trim().is_empty()
        {
            builder.names.insert(credited_as.trim().to_string());
        }
        builder.recording_ids.insert(link.recording_id);
        if let Some(work) = &link.work {
            builder.work_mbids.insert(work.mbid.clone());
        }
    }
    for link in sources
        .edition_credit_links(catalog)
        .into_iter()
        .filter(|link| link.trusted)
    {
        let mbid = link.credit.artist_mbid.trim();
        if mbid.is_empty() || local_mbids.contains(mbid) {
            continue;
        }
        let name = link.credit.artist_name.trim();
        let builder = grouped.entry(mbid.to_string()).or_insert_with(|| Builder {
            name: name.to_string(),
            fetched_at: link.fetched_at,
            names: BTreeSet::new(),
            recording_ids: BTreeSet::new(),
            release_ids: BTreeSet::new(),
            work_mbids: BTreeSet::new(),
        });
        if !name.is_empty() {
            builder.names.insert(name.to_string());
            if builder.name.is_empty()
                || link.fetched_at > builder.fetched_at
                || (link.fetched_at == builder.fetched_at && name < builder.name.as_str())
            {
                builder.name = name.to_string();
                builder.fetched_at = link.fetched_at;
            }
        }
        if let Some(credited_as) = link.credit.credited_as.as_deref()
            && !credited_as.trim().is_empty()
        {
            builder.names.insert(credited_as.trim().to_string());
        }
        builder.release_ids.insert(link.release_id);
    }
    grouped
        .into_iter()
        .map(|(mbid, builder)| SourcedContributor {
            name: if builder.name.is_empty() {
                mbid.clone()
            } else {
                builder.name
            },
            mbid,
            names: builder.names.into_iter().collect(),
            recording_ids: builder.recording_ids.into_iter().collect(),
            release_ids: builder.release_ids.into_iter().collect(),
            work_mbids: builder.work_mbids.into_iter().collect(),
        })
        .collect()
}

/// Match a sourced contributor by canonical or credited-as spelling.
/// Callers that select by MusicBrainz identity compare [`SourcedContributor::mbid`] directly.
pub fn matches_name(contributor: &SourcedContributor, query: &str, exact: bool) -> bool {
    let wanted = text::normalize(query);
    !wanted.is_empty()
        && contributor.names.iter().any(|name| {
            let key = text::normalize(name);
            if exact {
                key == wanted
            } else {
                key.contains(&wanted)
            }
        })
}

#[cfg(test)]
#[path = "contributors_tests.rs"]
mod tests;
