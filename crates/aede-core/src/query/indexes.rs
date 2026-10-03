//! Borrowed lookup tables built once for a query or sort, without changing stores.

use super::*;
use crate::model::{Credit, Genre};
use crate::user::Annotation;

#[derive(Default)]
pub(super) struct LocalIndexes<'a> {
    catalog: Option<&'a Catalog>,
    data: Option<&'a UserData>,
    owner: &'a str,
    credits: BTreeMap<(EntityKind, Id), Vec<&'a Credit>>,
    genres: BTreeMap<(EntityKind, Id), Vec<&'a Genre>>,
    annotations: BTreeMap<(EntityKind, &'a str), &'a Annotation>,
    counts: BTreeMap<(EntityKind, &'a str), u32>,
}

impl<'a> LocalIndexes<'a> {
    pub(super) fn new(catalog: &'a Catalog, data: &'a UserData, owner: &'a str) -> Self {
        let mut indexed = Self {
            catalog: Some(catalog),
            data: Some(data),
            owner,
            ..Self::default()
        };
        for credit in &catalog.credits {
            indexed
                .credits
                .entry((credit.entity_kind, credit.entity_id))
                .or_default()
                .push(credit);
        }
        for link in &catalog.genre_links {
            if let Some(genre) = catalog.genre(link.genre_id) {
                indexed
                    .genres
                    .entry((link.entity_kind, link.entity_id))
                    .or_default()
                    .push(genre);
            }
        }
        // Keep the first row, exactly like UserData::find/play_count, including
        // legacy duplicates. Other owners and waiting references stay separate.
        for annotation in data.annotations.iter().filter(|row| row.owner == owner) {
            indexed
                .annotations
                .entry((annotation.target.kind, &annotation.target.key))
                .or_insert(annotation);
        }
        for count in data.counts.iter().filter(|row| row.owner == owner) {
            indexed
                .counts
                .entry((count.track.kind, &count.track.key))
                .or_insert(count.count);
        }
        indexed
    }

    pub(super) fn credits(
        &self,
        catalog: &'a Catalog,
        kind: EntityKind,
        id: Id,
    ) -> std::borrow::Cow<'_, [&'a Credit]> {
        if self
            .catalog
            .is_some_and(|original| std::ptr::eq(original, catalog))
        {
            self.credits
                .get(&(kind, id))
                .map(Vec::as_slice)
                .unwrap_or_default()
                .into()
        } else {
            catalog
                .credits
                .iter()
                .filter(|credit| credit.entity_kind == kind && credit.entity_id == id)
                .collect::<Vec<_>>()
                .into()
        }
    }

    pub(super) fn genres(
        &self,
        catalog: &'a Catalog,
        kind: EntityKind,
        id: Id,
    ) -> std::borrow::Cow<'_, [&'a Genre]> {
        if self
            .catalog
            .is_some_and(|original| std::ptr::eq(original, catalog))
        {
            self.genres
                .get(&(kind, id))
                .map(Vec::as_slice)
                .unwrap_or_default()
                .into()
        } else {
            catalog.genres_of(kind, id).into()
        }
    }

    pub(super) fn annotation(
        &self,
        data: &'a UserData,
        owner: &str,
        target: &EntityRef,
    ) -> Option<&'a Annotation> {
        if self.owner == owner
            && self
                .data
                .is_some_and(|original| std::ptr::eq(original, data))
        {
            self.annotations
                .get(&(target.kind, target.key.as_str()))
                .copied()
        } else {
            data.find(owner, target)
        }
    }

    pub(super) fn play_count(&self, data: &'a UserData, owner: &str, target: &EntityRef) -> u32 {
        if self.owner == owner
            && self
                .data
                .is_some_and(|original| std::ptr::eq(original, data))
        {
            self.counts
                .get(&(target.kind, target.key.as_str()))
                .copied()
                .unwrap_or(0)
        } else {
            data.play_count(owner, target)
        }
    }
}
