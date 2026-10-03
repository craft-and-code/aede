//! Waiting personal references and explicit, reversible reattachment.

use super::*;

/// One unresolved reference, with one owner's records counted together.
struct WaitingReference {
    reference: EntityRef,
    annotations: usize,
    plays: usize,
    counts: usize,
    relations: usize,
    playlists: usize,
    scrobbles: usize,
}

fn waiting_references(
    catalog: &Catalog,
    data: &UserData,
    owner: &str,
    label: Option<&str>,
) -> Vec<WaitingReference> {
    let mut rows: std::collections::BTreeMap<EntityRef, WaitingReference> =
        std::collections::BTreeMap::new();
    let tagged = |tags: &std::collections::BTreeSet<String>| {
        label.is_none_or(|wanted| {
            tags.iter()
                .any(|tag| text::normalize(tag) == text::normalize(wanted))
        })
    };
    let selected: std::collections::BTreeSet<_> = data
        .annotations
        .iter()
        .filter(|annotation| annotation.owner == owner && tagged(&annotation.tags))
        .map(|annotation| &annotation.target)
        .chain(
            data.relation_annotations
                .iter()
                .filter(|annotation| annotation.owner == owner && tagged(&annotation.tags))
                .flat_map(|annotation| [&annotation.relation.source, &annotation.relation.target]),
        )
        .chain(
            data.plays
                .iter()
                .filter(|play| play.owner == owner && label.is_none())
                .map(|play| &play.track),
        )
        .chain(
            data.counts
                .iter()
                .filter(|count| count.owner == owner && label.is_none())
                .map(|count| &count.track),
        )
        .chain(
            data.playlists
                .iter()
                .filter(|playlist| playlist.owner == owner && label.is_none())
                .flat_map(|playlist| &playlist.tracks),
        )
        .chain(
            data.scrobbles
                .iter()
                .filter(|report| report.owner == owner && label.is_none())
                .map(|report| &report.track),
        )
        .filter(|reference| reference.resolve(catalog).is_none())
        .cloned()
        .collect();
    for reference in selected {
        let row = WaitingReference {
            annotations: data
                .annotations
                .iter()
                .filter(|annotation| annotation.owner == owner && annotation.target == reference)
                .count(),
            plays: data
                .plays
                .iter()
                .filter(|play| play.owner == owner && play.track == reference)
                .count(),
            counts: data
                .counts
                .iter()
                .filter(|count| count.owner == owner && count.track == reference)
                .count(),
            relations: data
                .relation_annotations
                .iter()
                .filter(|annotation| {
                    annotation.owner == owner
                        && (annotation.relation.source == reference
                            || annotation.relation.target == reference)
                })
                .count(),
            playlists: data
                .playlists
                .iter()
                .filter(|playlist| playlist.owner == owner && playlist.tracks.contains(&reference))
                .count(),
            scrobbles: data
                .scrobbles
                .iter()
                .filter(|report| report.owner == owner && report.track == reference)
                .count(),
            reference: reference.clone(),
        };
        rows.insert(reference, row);
    }
    rows.into_values().collect()
}

pub(super) fn waiting_notes(args: &Args, catalog: &Catalog, data: &UserData) -> Res {
    let window = args.window(DEFAULT_LIMIT)?;
    let rows = waiting_references(catalog, data, &owner(args), args.value("tag"));
    let selected = rows
        .iter()
        .skip(window.offset)
        .take(window.limit)
        .collect::<Vec<_>>();
    if args.has("json") {
        let values = selected
            .iter()
            .map(|row| {
                let mut value = aede_core::json::Json::obj();
                value.set("kind", row.reference.kind.as_str().into());
                value.set("name", row.reference.display_name(catalog).into());
                value.set("reference", row.reference.to_token().into());
                value.set("annotations", row.annotations.into());
                value.set("plays", row.plays.into());
                value.set("counts", row.counts.into());
                value.set("relations", row.relations.into());
                value.set("playlists", row.playlists.into());
                value.set("scrobbles", row.scrobbles.into());
                value
            })
            .collect();
        return super::super::export::emit(
            args,
            &aede_core::json::Json::Arr(values).to_string_pretty(),
        );
    }
    let table_rows = selected
        .iter()
        .map(|row| {
            vec![
                row.reference.kind.as_str().into(),
                row.reference.display_name(catalog),
                row.reference.to_token(),
                row.annotations.to_string(),
                row.plays.to_string(),
                row.counts.to_string(),
                row.relations.to_string(),
                row.playlists.to_string(),
                row.scrobbles.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    if args.has("csv") {
        return super::super::export::rows_table(
            &[
                "kind",
                "name",
                "reference",
                "annotations",
                "plays",
                "counts",
                "relations",
                "playlists",
                "scrobbles",
            ],
            &table_rows,
            args,
        );
    }
    if rows.is_empty() {
        println!("{}", ui::dim("no personal references are waiting"));
        return Ok(());
    }
    println!(
        "{}",
        ui::section(&format!("Waiting references ({})", rows.len()))
    );
    let mut table = Table::new(&[
        "Kind",
        "Name",
        "Annotations",
        "Listens",
        "Counters",
        "Relations",
        "Playlists",
        "Client listens",
    ]);
    for row in &selected {
        table.push(vec![
            row.reference.kind.as_str().into(),
            row.reference.display_name(catalog),
            row.annotations.to_string(),
            row.plays.to_string(),
            row.counts.to_string(),
            row.relations.to_string(),
            row.playlists.to_string(),
            row.scrobbles.to_string(),
        ]);
    }
    print!("{}", table.render());
    for row in selected {
        println!("  {}", ui::literal(&row.reference.to_token()));
    }
    super::super::announce_window(window, rows.len(), "reference");
    Ok(())
}

pub(super) fn validate_notes_options(args: &Args) -> Res {
    if let Some(message) = args.output_conflict(&["csv", "json"]) {
        return Err(message.into());
    }
    if args.has("separator") {
        if !args.has("csv") {
            return Err("notes --separator requires --csv".into());
        }
        super::super::export::separator(args)?;
    }
    let modes = ["export", "import", "relink", "undo-relink", "relinks"]
        .into_iter()
        .filter(|mode| args.has(mode))
        .count();
    if modes > 1 {
        return Err(
            "choose one notes operation: export, import, relink, undo-relink or relinks".into(),
        );
    }
    if args.has("to") != args.has("relink") {
        return Err("use --relink <old-reference> together with --to <new-reference>".into());
    }
    if args.has("dry-run") && !args.has("relink") && !args.has("undo-relink") {
        return Err("notes --dry-run requires --relink or --undo-relink".into());
    }
    if modes > 0 && args.has("waiting") {
        return Err("--waiting is a notes listing filter".into());
    }
    if (args.has("waiting") || args.has("relinks")) && args.has("search") {
        return Err("waiting references and reattachment history do not support --search".into());
    }
    if args.has("output") && !args.has("export") && !args.has("json") && !args.has("csv") {
        return Err("notes --output requires --export, --json or --csv".into());
    }
    if (args.has("relink") || args.has("undo-relink") || args.has("export") || args.has("import"))
        && (args.has("tag")
            || args.has("search")
            || args.has("limit")
            || args.has("offset")
            || args.has("all"))
    {
        return Err("listing filters do not apply to a notes write or export operation".into());
    }
    if args.has("relinks") && args.has("tag") {
        return Err("--tag filters annotations, not reattachment history".into());
    }
    if args.has("import") && (args.has("csv") || args.has("json") || args.has("output")) {
        return Err(
            "notes --import reports its merge in the terminal; output format options do not apply"
                .into(),
        );
    }
    if args.has("export") && args.has("csv") {
        return Err("notes --export writes the complete personal store as JSON".into());
    }
    if !args.positionals.is_empty() {
        return Err("notes uses options; name the exact source with --relink".into());
    }
    Ok(())
}

pub(super) fn relink_notes(args: &Args, catalog: &Catalog, data: &mut UserData) -> Res {
    let owner = owner(args);
    if args.has("relinks") {
        let window = args.window(DEFAULT_LIMIT)?;
        let total = data
            .relinks
            .iter()
            .filter(|event| event.owner == owner)
            .count();
        let rows = data
            .relinks
            .iter()
            .filter(|event| event.owner == owner)
            .rev()
            .skip(window.offset)
            .take(window.limit)
            .map(|event| {
                vec![
                    event.id.to_string(),
                    event.from.to_token(),
                    event.to.to_token(),
                    event.at.to_string(),
                    event.undone_at.map(|at| at.to_string()).unwrap_or_default(),
                ]
            })
            .collect::<Vec<_>>();
        if args.has("json") || args.has("csv") {
            return super::super::export::rows_table(
                &["id", "from", "to", "at", "undone_at"],
                &rows,
                args,
            );
        }
        if total == 0 {
            println!(
                "{}",
                ui::dim("no reattachment decisions have been recorded")
            );
            return Ok(());
        }
        let mut table = Table::new(&["ID", "From", "To", "At", "Undone at"]);
        for row in rows {
            table.push(row);
        }
        print!("{}", table.render());
        super::super::announce_window(window, total, "decision");
        return Ok(());
    }
    let dry_run = args.has("dry-run");
    let (id, from, to, summary) = if let Some(token) = args.value("relink") {
        let from = parse_reference(token).ok_or(
            "--relink needs a stable kind:key reference, as listed by notes --waiting --json",
        )?;
        let to = relink_destination(
            args.value("to").ok_or("give the destination with --to")?,
            catalog,
        )?;
        let summary = data.preview_relink(&owner, &from, &to, catalog)?;
        let id = if dry_run {
            None
        } else {
            Some(data.relink(&owner, &from, &to, catalog, clock::now_seconds())?)
        };
        (id, from, to, summary)
    } else {
        let id: u64 = args
            .value("undo-relink")
            .ok_or("give the reattachment ID")?
            .parse()
            .map_err(|_| "--undo-relink needs a positive integer ID")?;
        let summary = data.preview_undo_relink(&owner, id)?;
        let event = data
            .relinks
            .iter()
            .find(|event| event.owner == owner && event.id == id)
            .ok_or("reattachment is missing")?;
        let (from, to) = (event.to.clone(), event.from.clone());
        if !dry_run {
            data.undo_relink(&owner, id, clock::now_seconds())?;
        }
        (Some(id), from, to, summary)
    };
    if !dry_run {
        write(args, data, catalog)?;
    }
    if args.has("json") {
        let mut row = aede_core::json::Json::obj();
        if let Some(id) = id {
            row.set("id", id.into());
        }
        row.set("dry_run", aede_core::json::Json::Bool(dry_run));
        row.set("from", from.to_token().into());
        row.set("to", to.to_token().into());
        row.set("annotations", summary.annotations.into());
        row.set("plays", summary.plays.into());
        row.set("counts", summary.counts.into());
        row.set("relations", summary.relations.into());
        row.set("playlists", summary.playlists.into());
        row.set("scrobbles", summary.scrobbles.into());
        return super::super::export::emit(args, &row.to_string_pretty());
    }
    if args.has("csv") {
        return super::super::export::rows_table(
            &[
                "id",
                "from",
                "to",
                "dry_run",
                "annotations",
                "plays",
                "counts",
                "relations",
                "playlists",
                "scrobbles",
            ],
            &[vec![
                id.map(|id| id.to_string()).unwrap_or_default(),
                from.to_token(),
                to.to_token(),
                dry_run.to_string(),
                summary.annotations.to_string(),
                summary.plays.to_string(),
                summary.counts.to_string(),
                summary.relations.to_string(),
                summary.playlists.to_string(),
                summary.scrobbles.to_string(),
            ]],
            args,
        );
    }
    println!(
        "{} {} → {}: {} annotations, {} listens, {} counters, {} relation notes, {} playlists, {} client listens{}",
        if dry_run {
            "Would reattach"
        } else {
            "Reattached"
        },
        ui::literal(&from.to_token()),
        ui::literal(&to.to_token()),
        summary.annotations,
        summary.plays,
        summary.counts,
        summary.relations,
        summary.playlists,
        summary.scrobbles,
        id.map(|id| format!(" (reattachment {id})"))
            .unwrap_or_default()
    );
    Ok(())
}

fn relink_destination(token: &str, catalog: &Catalog) -> Result<EntityRef, Box<dyn Error>> {
    let reference =
        parse_reference(token).ok_or("--to needs kind:name or a stable kind:key reference")?;
    if let Some(canonical) = reference
        .resolve(catalog)
        .and_then(|id| EntityRef::of(catalog, reference.kind, id))
    {
        return Ok(canonical);
    }
    find(catalog, reference.kind, &reference.key)
}

#[cfg(test)]
#[path = "annotate_relink_tests.rs"]
mod tests;
