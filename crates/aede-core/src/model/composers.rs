//! Exact local composer credits grouped by edition, without artist-name inference.
use super::{Catalog, EntityKind, Id};
use std::collections::{BTreeMap, BTreeSet};
impl Catalog {
    /// Index credited composers once per projection/evaluation. Recording and work credits
    /// reach every local edition containing that recording; duplicate credits stay one identity.
    /// This reads the local graph only, never unaccepted external claims or audio files.
    pub fn composers_by_release(&self) -> BTreeMap<Id, BTreeSet<Id>> {
        // Index only actual composer credits, rather than allocating an inverse map for
        // every track/recording/work placement in a potentially large library.
        let mut credits: BTreeMap<(EntityKind, Id), BTreeSet<Id>> = BTreeMap::new();
        for credit in self
            .credits
            .iter()
            .filter(|credit| credit.role == "composer")
        {
            if self.artist(credit.artist_id).is_some() {
                credits
                    .entry((credit.entity_kind, credit.entity_id))
                    .or_default()
                    .insert(credit.artist_id);
            }
        }
        let mut result = BTreeMap::new();
        if credits.is_empty() {
            return result;
        }
        for release in &self.releases {
            let mut composers = credits
                .get(&(EntityKind::Release, release.id))
                .cloned()
                .unwrap_or_default();
            for id in &release.track_ids {
                let Some(track) = self.track(*id) else {
                    continue;
                };
                for target in [
                    (EntityKind::Track, track.id),
                    (EntityKind::Recording, track.recording_id),
                ] {
                    if let Some(artists) = credits.get(&target) {
                        composers.extend(artists);
                    }
                }
                if let Some(recording) = self.recording(track.recording_id) {
                    for work in &recording.work_ids {
                        if let Some(artists) = credits.get(&(EntityKind::Work, *work)) {
                            composers.extend(artists);
                        }
                    }
                }
            }
            if !composers.is_empty() {
                result.insert(release.id, composers);
            }
        }
        result
    }
}
