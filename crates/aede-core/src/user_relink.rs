//! Explicit reattachment with an owner-scoped, conflict-checked undo record.

use super::*;
use crate::json::Json;

/// A user's explicit decision to reattach personal data.
#[derive(Debug, Clone)]
pub struct Relink {
    /// Stable identifier inside this user store, used by undo.
    pub id: u64,
    /// Owner whose records were moved.
    pub owner: UserRef,
    /// Original unresolved reference.
    pub from: EntityRef,
    /// Explicitly selected destination.
    pub to: EntityRef,
    /// Decision time in Unix seconds.
    pub at: u64,
    /// Undo time, when this decision has been taken back.
    pub undone_at: Option<u64>,
    // Local IDs may be reassigned on import. Keep the original sequence to
    // distinguish successive decisions made in the same second on one store.
    pub(super) origin_id: u64,
    pub(super) before: UserData,
}

/// Counts of personal records moved by one explicit reattachment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelinkSummary {
    /// Entity annotations.
    pub annotations: usize,
    /// Recent listening events.
    pub plays: usize,
    /// All-time play counters.
    pub counts: usize,
    /// Relationship annotations with an affected endpoint.
    pub relations: usize,
}

fn snapshot(data: &UserData, owner: &str, target: &EntityRef) -> UserData {
    UserData {
        annotations: data
            .annotations
            .iter()
            .filter(|a| a.owner == owner && a.target == *target)
            .cloned()
            .collect(),
        plays: data
            .plays
            .iter()
            .filter(|p| p.owner == owner && p.track == *target)
            .cloned()
            .collect(),
        counts: data
            .counts
            .iter()
            .filter(|c| c.owner == owner && c.track == *target)
            .cloned()
            .collect(),
        relation_annotations: data
            .relation_annotations
            .iter()
            .filter(|a| {
                a.owner == owner && (a.relation.source == *target || a.relation.target == *target)
            })
            .cloned()
            .collect(),
        ..Default::default()
    }
}

fn summary(data: &UserData) -> RelinkSummary {
    RelinkSummary {
        annotations: data.annotations.len(),
        plays: data.plays.len(),
        counts: data.counts.len(),
        relations: data.relation_annotations.len(),
    }
}

fn has_records(data: &UserData) -> bool {
    summary(data) != RelinkSummary::default()
}

fn rewrite(data: &mut UserData, owner: &str, from: &EntityRef, to: &EntityRef) {
    for target in data
        .annotations
        .iter_mut()
        .filter(|a| a.owner == owner)
        .map(|a| &mut a.target)
        .chain(
            data.plays
                .iter_mut()
                .filter(|p| p.owner == owner)
                .map(|p| &mut p.track),
        )
        .chain(
            data.counts
                .iter_mut()
                .filter(|c| c.owner == owner)
                .map(|c| &mut c.track),
        )
        .chain(
            data.relation_annotations
                .iter_mut()
                .filter(|a| a.owner == owner)
                .flat_map(|a| [&mut a.relation.source, &mut a.relation.target]),
        )
    {
        if target == from {
            *target = to.clone();
        }
    }
}

impl UserData {
    /// Validates an explicit reattachment without changing any data.
    ///
    /// The source must be waiting, the destination must exist and have the same
    /// kind, and that owner's destination must hold no conflicting personal
    /// record. Other owners are untouched and never cause a conflict.
    pub fn preview_relink(
        &self,
        owner: &str,
        from: &EntityRef,
        to: &EntityRef,
        catalog: &Catalog,
    ) -> Result<RelinkSummary, String> {
        if from.kind != to.kind {
            return Err("reattachment requires two references of the same kind".into());
        }
        if from.resolve(catalog).is_some() {
            return Err(
                "the source still exists; only waiting references can be reattached".into(),
            );
        }
        if to.resolve(catalog).is_none() {
            return Err("the destination is not in the catalog".into());
        }
        let records = snapshot(self, owner, from);
        if !has_records(&records) {
            return Err("no personal record for this owner uses the source reference".into());
        }
        let destination_id = to.resolve(catalog);
        let conflicting = self
            .annotations
            .iter()
            .filter(|a| a.owner == owner)
            .map(|a| &a.target)
            .chain(
                self.plays
                    .iter()
                    .filter(|p| p.owner == owner)
                    .map(|p| &p.track),
            )
            .chain(
                self.counts
                    .iter()
                    .filter(|c| c.owner == owner)
                    .map(|c| &c.track),
            )
            .chain(
                self.relation_annotations
                    .iter()
                    .filter(|a| a.owner == owner)
                    .flat_map(|a| [&a.relation.source, &a.relation.target]),
            )
            .any(|reference| {
                reference.kind == to.kind && reference.resolve(catalog) == destination_id
            });
        if conflicting {
            return Err("the destination already has personal data; reattachment would merge or overwrite it".into());
        }
        Ok(summary(&records))
    }

    /// Applies a validated reattachment, preserving its original records for undo.
    pub fn relink(
        &mut self,
        owner: &str,
        from: &EntityRef,
        to: &EntityRef,
        catalog: &Catalog,
        now: u64,
    ) -> Result<u64, String> {
        self.preview_relink(owner, from, to, catalog)?;
        let destination = to
            .resolve(catalog)
            .and_then(|id| EntityRef::of(catalog, to.kind, id))
            .ok_or("the destination cannot be named stably")?;
        if destination != *to {
            self.preview_relink(owner, from, &destination, catalog)?;
        }
        let id = self
            .relinks
            .iter()
            .map(|row| row.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("reattachment identifiers are exhausted")?;
        let before = snapshot(self, owner, from);
        rewrite(self, owner, from, &destination);
        self.relinks.push(Relink {
            id,
            owner: owner.into(),
            from: from.clone(),
            to: destination,
            at: now,
            undone_at: None,
            origin_id: id,
            before,
        });
        reconcile(self, catalog);
        Ok(id)
    }

    /// Checks undo without mutation, refusing edits or newly conflicting data.
    ///
    /// Undo never discards new listening events or note edits. If the moved
    /// records have changed, it refuses and leaves the store unchanged.
    pub fn preview_undo_relink(&self, owner: &str, id: u64) -> Result<RelinkSummary, String> {
        let event = self
            .relinks
            .iter()
            .find(|row| row.id == id && row.owner == owner)
            .ok_or("no reattachment with that ID belongs to this owner")?;
        if event.undone_at.is_some() {
            return Err("this reattachment has already been undone".into());
        }
        if has_records(&snapshot(self, owner, &event.from)) {
            return Err(
                "the original reference now has personal data; undo would overwrite it".into(),
            );
        }
        let mut expected = event.before.clone();
        rewrite(&mut expected, owner, &event.from, &event.to);
        let current = snapshot(self, owner, &event.to);
        if current.annotations != expected.annotations
            || current.plays != expected.plays
            || current.counts != expected.counts
            || current.relation_annotations != expected.relation_annotations
        {
            return Err(
                "the reattached records have changed; undo would discard newer personal data"
                    .into(),
            );
        }
        Ok(summary(&expected))
    }

    /// Takes back one reattachment after the same checks as its preview.
    pub fn undo_relink(&mut self, owner: &str, id: u64, now: u64) -> Result<RelinkSummary, String> {
        let result = self.preview_undo_relink(owner, id)?;
        let event = self
            .relinks
            .iter()
            .find(|row| row.id == id && row.owner == owner)
            .ok_or("reattachment is missing")?;
        let (from, to) = (event.from.clone(), event.to.clone());
        rewrite(self, owner, &to, &from);
        if let Some(event) = self
            .relinks
            .iter_mut()
            .find(|row| row.id == id && row.owner == owner)
        {
            event.undone_at = Some(now);
        }
        Ok(result)
    }
}

impl Relink {
    pub(super) fn to_json(&self) -> Json {
        let mut row = Json::obj();
        row.set("id", self.id.into());
        row.set("origin_id", self.origin_id.into());
        row.set("owner", self.owner.as_str().into());
        row.set("from", self.from.to_token().into());
        row.set("to", self.to.to_token().into());
        row.set("at", self.at.into());
        if let Some(at) = self.undone_at {
            row.set("undone_at", at.into());
        }
        row.set("before", super::to_json(&self.before));
        row
    }

    pub(super) fn from_json(row: &Json) -> Option<Self> {
        let before = row.get("before")?;
        // Undo snapshots are flat records, never nested command histories.
        if !before
            .get("relinks")
            .and_then(Json::as_arr)
            .unwrap_or(&[])
            .is_empty()
        {
            return None;
        }
        let id = row.field_u64("id").filter(|id| *id > 0)?;
        let event = Self {
            id,
            origin_id: row
                .field_u64("origin_id")
                .filter(|id| *id > 0)
                .unwrap_or(id),
            owner: row.field_str("owner")?,
            from: EntityRef::parse_token(&row.field_str("from")?)?,
            to: EntityRef::parse_token(&row.field_str("to")?)?,
            at: row.field_u64("at")?,
            undone_at: row.field_u64("undone_at"),
            before: super::from_json(before).ok()?,
        };
        if event.from.kind != event.to.kind || !has_records(&event.before) {
            return None;
        }
        let selected = snapshot(&event.before, &event.owner, &event.from);
        if selected.annotations != event.before.annotations
            || selected.plays != event.before.plays
            || selected.counts != event.before.counts
            || selected.relation_annotations != event.before.relation_annotations
        {
            return None;
        }
        Some(event)
    }
}

#[cfg(test)]
#[path = "user_relink_tests.rs"]
mod tests;
