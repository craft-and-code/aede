//! The `search` command: one query across every entity.

use aede_core::json::Json;
use aede_core::model::{EntityKind, Id};

use super::{Res, load, selection_output};
use crate::args::Args;
use crate::ui::{self, Table};

pub fn search(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give some text to search for".into());
    }
    let hits = catalog.search(&query, args.usize_value("limit", 30));

    // Only the tracks: an artist or an album is not something to play, nor a
    // row in a table of tracks.
    let ids: Vec<Id> = hits
        .iter()
        .filter(|h| h.kind == EntityKind::Track)
        .map(|h| h.id)
        .collect();
    if let Some(result) = selection_output(&catalog, &ids, args) {
        return result;
    }

    if args.has("json") {
        let json = Json::Arr(
            hits.iter()
                .map(|h| {
                    let mut o = Json::obj();
                    o.set("type", h.kind.as_str().into());
                    o.set("id", h.id.into());
                    o.set("name", h.name.clone().into());
                    o.set("context", h.detail.clone().into());
                    o
                })
                .collect(),
        );
        println!("{}", json.to_string_pretty());
        return Ok(());
    }

    println!("{}", ui::section(&format!("Results for \"{query}\"")));
    let mut t = Table::new(&["Type", "Name", "Context"])
        .limit(1, 45)
        .limit(2, 35);
    for hit in &hits {
        let kind = match hit.kind {
            EntityKind::Artist => "artist",
            EntityKind::Release => "album",
            EntityKind::Track => "track",
            EntityKind::Label => "label",
        };
        t.push(vec![kind.to_string(), hit.name.clone(), hit.detail.clone()]);
    }
    print!("{}", t.render());
    Ok(())
}
