//! Browsing and annotating the relationships of the unified music graph.

use std::collections::BTreeSet;

use aede_core::graph::{self, GraphEdge};
use aede_core::json::Json;
use aede_core::user::{self, LOCAL_USER, RelationAnnotation};

use super::{Res, announce_window, data_dir, load, sources_held};
use crate::args::Args;
use crate::ui::{self, Table};

const DEFAULT_LIMIT: usize = 50;

/// Lists local and source-backed graph relationships.
pub fn relations(args: &Args) -> Res {
    let catalog = load(args)?;
    let sources = sources_held(args)?;
    let user = user::load(&user::user_path(&data_dir(args)))?.unwrap_or_default();
    let wanted = aede_core::text::normalize(&args.positionals.join(" "));
    let source = args.value("source");
    let tag = args.value("tag").map(aede_core::text::normalize);
    let mut edges = graph::edges(&catalog, &sources)
        .into_iter()
        .filter(|edge| source.is_none_or(|source| edge.reference.provenance == source))
        .filter(|edge| {
            wanted.is_empty()
                || aede_core::text::normalize(&edge.source_name).contains(&wanted)
                || aede_core::text::normalize(&edge.target_name).contains(&wanted)
                || aede_core::text::normalize(&edge.reference.kind).contains(&wanted)
                || edge.reference.id().starts_with(&wanted)
        })
        .filter(|edge| {
            tag.as_ref().is_none_or(|wanted| {
                annotation(&user, edge).is_some_and(|annotation| {
                    annotation
                        .tags
                        .iter()
                        .any(|tag| aede_core::text::normalize(tag) == *wanted)
                })
            })
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| {
        left.source_name
            .cmp(&right.source_name)
            .then_with(|| left.reference.kind.cmp(&right.reference.kind))
            .then_with(|| left.target_name.cmp(&right.target_name))
    });

    if edges.is_empty() {
        println!("{}", ui::section("Relations"));
        println!("  {}", ui::dim("no relationship matches"));
        return Ok(());
    }

    let window = args.window(DEFAULT_LIMIT)?;
    if args.has("json") {
        let rows = edges
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|edge| edge_json(edge, annotation(&user, edge)))
            .collect();
        return super::export::emit(args, &Json::Arr(rows).to_string_pretty());
    }

    println!("{}", ui::section(&format!("Relations ({})", edges.len())));
    let mut table = Table::new(&["ID", "From", "Relation", "To", "Source", "Personal"])
        .limit(1, 26)
        .limit(2, 28)
        .limit(3, 26)
        .limit(5, 24);
    for edge in edges.iter().skip(window.offset).take(window.limit) {
        let personal = annotation(&user, edge).map_or_else(String::new, annotation_summary);
        table.push(vec![
            edge.reference.id(),
            edge.source_name.clone(),
            edge.reference.kind.clone(),
            edge.target_name.clone(),
            provenance(edge),
            personal,
        ]);
    }
    print!("{}", table.render());
    announce_window(window, edges.len(), "relation");
    println!(
        "  {}",
        ui::dim("aede relation <ID> opens one; --text and --tag add your own context")
    );
    Ok(())
}

/// Shows or changes one relationship annotation.
pub fn relation(args: &Args) -> Res {
    let selector = args.positionals.join(" ");
    if selector.trim().is_empty() {
        return Err("give the ID printed by aede relations: aede relation <ID>".into());
    }
    let catalog = load(args)?;
    let sources = sources_held(args)?;
    let edges = graph::edges(&catalog, &sources);
    let path = user::user_path(&data_dir(args));
    let mut data = user::load(&path)?.unwrap_or_default();
    let edge = match graph::select(&edges, &selector) {
        Ok(edge) => edge,
        Err(_) if args.has("remove") && !args.has("text") && !args.has("tag") => {
            let relation = select_annotation(&data, &selector)?;
            data.relation_annotations.retain(|annotation| {
                annotation.owner != LOCAL_USER || annotation.relation != relation
            });
            user::save(&data, &path)?;
            println!("{} orphaned relation annotation removed", ui::green("→"));
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };

    let changes = args.has("text") || args.has("tag") || args.has("remove");
    if changes {
        change_annotation(args, &edge, &mut data)?;
        user::save(&data, &path)?;
    }

    print_edge(&edge, data.find_relation(LOCAL_USER, &edge.reference));
    if changes {
        println!("  {}", ui::dim(&format!("saved in {}", path.display())));
    }
    Ok(())
}

fn select_annotation(
    data: &user::UserData,
    selector: &str,
) -> Result<aede_core::graph::RelationRef, String> {
    let selector = selector.trim().to_ascii_lowercase();
    let mut found = data.relation_annotations.iter().filter(|annotation| {
        annotation.owner == LOCAL_USER && annotation.relation.id().starts_with(&selector)
    });
    let Some(annotation) = found.next() else {
        return Err(format!(
            "no relation or saved annotation starts with \"{selector}\""
        ));
    };
    if found.next().is_some() {
        return Err(format!("relation ID \"{selector}\" is ambiguous"));
    }
    Ok(annotation.relation.clone())
}

fn change_annotation(args: &Args, edge: &GraphEdge, data: &mut user::UserData) -> Res {
    if args.has("remove") && args.has("text") {
        return Err("--text writes a note while --remove removes it; give one".into());
    }
    let now = aede_core::clock::now_seconds();
    if args.has("remove") && !args.has("tag") {
        let before = data.relation_annotations.len();
        data.relation_annotations.retain(|annotation| {
            annotation.owner != LOCAL_USER || annotation.relation != edge.reference
        });
        println!(
            "{} {}",
            ui::green("→"),
            if data.relation_annotations.len() < before {
                "personal relation annotation removed"
            } else {
                "this relation had no personal annotation"
            }
        );
        return Ok(());
    }

    let entry = data.relation_entry(LOCAL_USER, &edge.reference, now);
    if let Some(text) = args.value("text") {
        if text.trim().is_empty() {
            return Err("--text cannot be blank; use --remove to erase the annotation".into());
        }
        entry.note = Some(text.to_string());
    }
    if let Some(tags) = args.value("tag") {
        let tags = parse_tags(tags)?;
        if args.has("remove") {
            for tag in tags {
                entry.tags.retain(|held| {
                    aede_core::text::normalize(held) != aede_core::text::normalize(&tag)
                });
            }
        } else {
            entry.tags.extend(tags);
        }
    }
    entry.updated_at = now;
    data.forget_empty();
    Ok(())
}

fn parse_tags(value: &str) -> Result<BTreeSet<String>, String> {
    let tags = value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if tags.is_empty() {
        Err("--tag expects one or more comma-separated labels".into())
    } else {
        Ok(tags)
    }
}

fn annotation<'a>(data: &'a user::UserData, edge: &GraphEdge) -> Option<&'a RelationAnnotation> {
    data.find_relation(LOCAL_USER, &edge.reference)
}

fn annotation_summary(annotation: &RelationAnnotation) -> String {
    let mut parts = annotation.tags.iter().cloned().collect::<Vec<_>>();
    if annotation.note.is_some() {
        parts.push("note".into());
    }
    parts.join(", ")
}

fn provenance(edge: &GraphEdge) -> String {
    let trust = if edge.trusted { "trusted" } else { "evidence" };
    format!("{} · {trust}", edge.reference.provenance)
}

fn print_edge(edge: &GraphEdge, annotation: Option<&RelationAnnotation>) {
    println!("{}", ui::section("Relation"));
    println!("  {}", ui::bold(&edge.source_name));
    println!("    {}", ui::cyan(&edge.reference.kind));
    println!("  {}", ui::bold(&edge.target_name));
    println!("\n  ID          {}", edge.reference.id());
    println!("  Source      {}", edge.reference.provenance);
    println!(
        "  Trust       {}",
        if edge.trusted {
            "trusted"
        } else {
            "evidence only"
        }
    );
    if let Some(confidence) = edge.confidence {
        let confidence = match confidence {
            aede_core::sources::Confidence::Identified => "identified".to_string(),
            aede_core::sources::Confidence::Matched(score) => format!("matched {score}%"),
        };
        println!("  Confidence  {confidence}");
    }
    if let Some(review) = edge.review {
        let review = match review {
            aede_core::sources::ReviewDecision::Accepted => "accepted",
            aede_core::sources::ReviewDecision::Rejected => "rejected",
        };
        println!("  Review      {review}");
    }
    if edge.weight > 1 {
        println!("  Weight      {}", edge.weight);
    }
    if let Some(source_id) = &edge.reference.source_id {
        println!("  Source ID   {source_id}");
    }
    if let Some(type_id) = &edge.relationship_type_id {
        println!("  Type ID     {type_id}");
    }
    if let Some(direction) = &edge.direction {
        println!("  Direction   {direction}");
    }
    if let Some(credited_as) = &edge.credited_as {
        println!("  Credited as {credited_as}");
    }
    if !edge.attributes.is_empty() {
        let attributes = edge
            .attributes
            .iter()
            .map(attribute_text)
            .collect::<Vec<_>>()
            .join(", ");
        println!("  Attributes  {attributes}");
    }
    let mut dates = match (&edge.began, &edge.ended) {
        (Some(began), Some(ended)) => format!("{began}–{ended}"),
        (Some(began), None) => format!("from {began}"),
        (None, Some(ended)) => format!("until {ended}"),
        (None, None) => String::new(),
    };
    if edge.over == Some(true) && edge.ended.is_none() {
        if !dates.is_empty() {
            dates.push_str(" · ");
        }
        dates.push_str("ended");
    } else if edge.reference.kind.starts_with("membership:")
        && edge.over == Some(false)
        && edge.ended.is_none()
    {
        if !dates.is_empty() {
            dates.push_str(" · ");
        }
        dates.push_str("current");
    }
    if !dates.is_empty() {
        println!("  Dates       {dates}");
    }
    if let Some(order) = edge.order {
        println!("  Order       {order}");
    }
    if let Some(fetched_at) = edge.fetched_at {
        println!("  Fetched     {}", ui::since(fetched_at));
    }
    if let Some(annotation) = annotation {
        println!("{}", ui::section("Your annotation"));
        if !annotation.tags.is_empty() {
            println!(
                "  Tags        {}",
                annotation
                    .tags
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let Some(note) = &annotation.note {
            println!("  Note");
            for line in ui::wrap(note, 72) {
                println!("    {line}");
            }
        }
    } else {
        println!(
            "\n  {}",
            ui::dim("add context with --text=\"…\" or --tag=\"…\"")
        );
    }
}

/// JSON representation shared by relation listings and the complete graph export.
pub fn edge_json(edge: &GraphEdge, annotation: Option<&RelationAnnotation>) -> Json {
    let mut row = Json::obj();
    row.set("id", edge.reference.id().into());
    row.set("source", edge.reference.source.to_token().into());
    row.set("source_name", edge.source_name.clone().into());
    row.set("relation", edge.reference.kind.clone().into());
    row.set("target", edge.reference.target.to_token().into());
    row.set("target_name", edge.target_name.clone().into());
    row.set("provenance", edge.reference.provenance.clone().into());
    row.set(
        "source_id",
        edge.reference
            .source_id
            .clone()
            .map(Json::Str)
            .unwrap_or(Json::Null),
    );
    row.set("weight", edge.weight.into());
    row.set("trusted", Json::Bool(edge.trusted));
    row.set(
        "confidence",
        edge.confidence
            .map(|confidence| match confidence {
                aede_core::sources::Confidence::Identified => "identified".into(),
                aede_core::sources::Confidence::Matched(score) => format!("matched:{score}"),
            })
            .map(Json::Str)
            .unwrap_or(Json::Null),
    );
    row.set(
        "review",
        edge.review
            .map(|review| match review {
                aede_core::sources::ReviewDecision::Accepted => "accepted",
                aede_core::sources::ReviewDecision::Rejected => "rejected",
            })
            .map(|value| value.into())
            .unwrap_or(Json::Null),
    );
    row.set(
        "fetched_at",
        edge.fetched_at.map(Json::from).unwrap_or(Json::Null),
    );
    row.set(
        "relationship_type_id",
        edge.relationship_type_id
            .clone()
            .map(Json::Str)
            .unwrap_or(Json::Null),
    );
    row.set(
        "direction",
        edge.direction.clone().map(Json::Str).unwrap_or(Json::Null),
    );
    row.set(
        "credited_as",
        edge.credited_as
            .clone()
            .map(Json::Str)
            .unwrap_or(Json::Null),
    );
    row.set(
        "attributes",
        Json::Arr(edge.attributes.iter().map(attribute_json).collect()),
    );
    row.set(
        "began",
        edge.began.clone().map(Json::Str).unwrap_or(Json::Null),
    );
    row.set(
        "ended",
        edge.ended.clone().map(Json::Str).unwrap_or(Json::Null),
    );
    row.set("over", edge.over.map(Json::Bool).unwrap_or(Json::Null));
    row.set("order", edge.order.map(Json::from).unwrap_or(Json::Null));
    if let Some(annotation) = annotation {
        let mut personal = Json::obj();
        personal.set(
            "note",
            annotation.note.clone().map(Json::Str).unwrap_or(Json::Null),
        );
        personal.set(
            "tags",
            Json::Arr(annotation.tags.iter().cloned().map(Json::Str).collect()),
        );
        personal.set("updated_at", annotation.updated_at.into());
        row.set("annotation", personal);
    } else {
        row.set("annotation", Json::Null);
    }
    row
}

fn attribute_text(attribute: &aede_core::model::CreditAttribute) -> String {
    match (&attribute.value, &attribute.credited_as) {
        (Some(value), Some(credited_as)) => {
            format!("{}: {value} (as {credited_as})", attribute.name)
        }
        (Some(value), None) => format!("{}: {value}", attribute.name),
        (None, Some(credited_as)) => format!("{} (as {credited_as})", attribute.name),
        (None, None) => attribute.name.clone(),
    }
}

fn attribute_json(attribute: &aede_core::model::CreditAttribute) -> Json {
    let mut row = Json::obj();
    row.set(
        "id",
        attribute.id.clone().map(Json::Str).unwrap_or(Json::Null),
    );
    row.set("name", attribute.name.clone().into());
    row.set(
        "value",
        attribute.value.clone().map(Json::Str).unwrap_or(Json::Null),
    );
    row.set(
        "credited_as",
        attribute
            .credited_as
            .clone()
            .map(Json::Str)
            .unwrap_or(Json::Null),
    );
    row
}
