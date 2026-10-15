//! Owner-private completion evidence, independent of the bounded recent history.
use crate::{
    model::{Catalog, EntityKind, Id},
    user::{EntityRef, UserData},
};
use std::collections::BTreeSet;
/// Confirmed complete track identities for exactly one owner. Legacy recent visits remain usable.
pub fn completed_tracks(data: &UserData, owner: &str) -> BTreeSet<EntityRef> {
    data.counts
        .iter()
        .filter(|row| row.owner == owner && row.completed_count > 0)
        .map(|row| row.track.clone())
        .chain(
            data.plays
                .iter()
                .filter(|row| row.owner == owner && row.completed)
                .map(|row| row.track.clone()),
        )
        .collect()
}
/// An edition is complete only when every catalogued placement has full-visit evidence.
/// Different sessions may contribute; empty releases, partial visits and scrobbles do not qualify.
pub fn completed_albums(catalog: &Catalog, data: &UserData, owner: &str) -> BTreeSet<Id> {
    let completed = completed_tracks(data, owner);
    catalog
        .releases
        .iter()
        .filter(|release| {
            !release.track_ids.is_empty()
                && release.track_ids.iter().all(|id| {
                    EntityRef::of(catalog, EntityKind::Track, *id)
                        .is_some_and(|reference| completed.contains(&reference))
                })
        })
        .map(|release| release.id)
        .collect()
}
#[cfg(test)]
#[path = "listening_tests.rs"]
mod tests;
