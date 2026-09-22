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

    /// Prints nothing when the page has nowhere else to go.
    pub fn print(self) {
        if self.rows.is_empty() {
            return;
        }
        const LIMIT: usize = 30;
        println!("{}", ui::section("Continue"));
        let mut table = Table::new(&["Relation", "Command"])
            .limit(0, 30)
            .limit(1, 90);
        let total = self.rows.len();
        for (relation, command) in self.rows.into_iter().take(LIMIT) {
            table.push(vec![relation, command]);
        }
        print!("{}", table.render());
        if total > LIMIT {
            println!(
                "  {}",
                ui::dim(&format!(
                    "{} more navigation targets not shown",
                    total - LIMIT
                ))
            );
        }
    }
}

/// Command that opens one entity, using an external identity when possible.
pub fn open_command(catalog: &Catalog, kind: EntityKind, id: Id) -> Option<String> {
    let (command, value) = match kind {
        EntityKind::Artist => ("artist", catalog.artist(id)?.name.clone()),
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
            (
                "recording",
                recording
                    .mbid
                    .clone()
                    .unwrap_or_else(|| recording.title.clone()),
            )
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
mod tests {
    use super::*;

    #[test]
    fn a_printed_name_is_one_safe_shell_argument() {
        assert_eq!(shell_arg("Guns N' Roses"), "'Guns N'\\'' Roses'");
    }

    #[test]
    fn graph_pages_prefer_identifiers_that_remove_title_ambiguity() {
        let mut catalog = Catalog::default();
        catalog.recordings.push(aede_core::model::Recording {
            id: 0,
            title: "Same Title".into(),
            mbid: Some("recording-id".into()),
            ..Default::default()
        });
        let recording = &catalog.recordings[0];
        let command = open_command(&catalog, EntityKind::Recording, recording.id).expect("command");
        assert_eq!(command, "aede recording 'recording-id'");
    }
}
