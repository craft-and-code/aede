//! Copyable paths from one catalog page to the next.
//!
//! A table naming related entities is informative; it becomes navigation only
//! when it also says how to open them. These helpers keep that spelling in one
//! place, prefer stable MusicBrainz identities where the command accepts one,
//! and quote names so the printed command can be pasted into a shell safely.

use std::collections::BTreeSet;

use aede_core::model::{Catalog, EntityKind, Id};

use crate::ui::{self, Table};

/// A small, ordered set of commands that continue from the current page.
#[derive(Default)]
pub struct Navigation {
    rows: Vec<(String, String)>,
    commands: BTreeSet<String>,
}

impl Navigation {
    /// Adds a ready-to-run command, ignoring duplicate destinations.
    pub fn add(&mut self, relation: impl Into<String>, command: impl Into<String>) {
        let command = command.into();
        if self.commands.insert(command.clone()) {
            self.rows.push((relation.into(), command));
        }
    }

    /// Adds the command that opens a canonical catalog entity.
    pub fn entity(
        &mut self,
        catalog: &Catalog,
        relation: impl Into<String>,
        kind: EntityKind,
        id: Id,
    ) {
        if let Some(command) = open_command(catalog, kind, id) {
            self.add(relation, command);
        }
    }

    /// Open an externally credited artist by the identity that source named.
    ///
    /// The artist need not occur in local tags, so a catalog ID cannot be
    /// assumed. Callers add only trusted source relationships here.
    pub fn source_artist(&mut self, mbid: &str) {
        if !mbid.trim().is_empty() {
            self.add(
                "Credited artist",
                format!("aede artist {}", shell_arg(mbid)),
            );
        }
    }

    /// Prints nothing when the page has nowhere else to go.
    pub fn print(self) {
        print!("{}", self.render());
    }

    fn render(self) -> String {
        if self.rows.is_empty() {
            return String::new();
        }
        const LIMIT: usize = 30;
        let mut table = Table::new(&["Relation", "Command"]).limit(0, 30);
        let total = self.rows.len();
        for (relation, command) in self.rows.into_iter().take(LIMIT) {
            table.push(vec![relation, command]);
        }
        let mut output = format!("{}\n{}", ui::section("Continue"), table.render());
        if total > LIMIT {
            output.push_str(&format!(
                "  {}\n",
                ui::dim(&format!(
                    "{} more navigation targets not shown",
                    total - LIMIT
                ))
            ));
        }
        output
    }
}

/// Command that opens one entity, using an external identity when possible.
pub fn open_command(catalog: &Catalog, kind: EntityKind, id: Id) -> Option<String> {
    let (command, value) = match kind {
        EntityKind::Artist => {
            let artist = catalog.artist(id)?;
            (
                "artist",
                artist.mbid.clone().unwrap_or_else(|| artist.name.clone()),
            )
        }
        EntityKind::Release => {
            let release = catalog.release(id)?;
            (
                "album",
                release
                    .mbid
                    .clone()
                    .unwrap_or_else(|| release.title.clone()),
            )
        }
        EntityKind::Track => ("track", catalog.track(id)?.title.clone()),
        EntityKind::Recording => {
            let recording = catalog.recording(id)?;
            let identity = recording.mbid.clone().or_else(|| {
                recording.track_ids.iter().find_map(|&track_id| {
                    let track = catalog.track(track_id)?;
                    let file = catalog.file(track.file_id)?;
                    Some(format!("local:{}", file.path))
                })
            })?;
            ("recording", identity)
        }
        EntityKind::Work => ("work", catalog.work(id)?.mbid.clone()),
        EntityKind::ReleaseGroup => ("release-group", catalog.release_group(id)?.mbid.clone()),
        EntityKind::Label => ("label", catalog.label(id)?.name.clone()),
        EntityKind::Genre => ("genre", catalog.genres.get(id as usize)?.name.clone()),
    };
    Some(format!("aede {command} {}", shell_arg(&value)))
}

/// One POSIX-shell argument, safe even when a catalog name contains a quote.
pub fn shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
