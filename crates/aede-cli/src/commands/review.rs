//! Explicit resolution of uncertain and conflicting source identities.
//!
//! Confidence belongs to the source; acceptance belongs to the user. This
//! command keeps those two statements separate and makes the latter durable,
//! reversible, and visible without ever rewriting an audio tag.

use aede_core::sources::{self, ReviewDecision, ReviewItem, ReviewReason};

use super::{Res, announce_window, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Table};

const DEFAULT_LIMIT: usize = 50;

pub fn review(args: &Args) -> Res {
    let catalog = load(args)?;
    let path = sources::sources_path(&data_dir(args));
    let mut held = sources::load(&path)?.unwrap_or_default();

    let actions = ["accept", "reject", "undo"]
        .into_iter()
        .filter(|action| args.has(action))
        .collect::<Vec<_>>();
    if actions.len() > 1 {
        return Err(format!(
            "{} ask for different decisions; choose one",
            actions
                .iter()
                .map(|action| format!("--{action}"))
                .collect::<Vec<_>>()
                .join(" and ")
        )
        .into());
    }

    if let Some(action) = actions.first() {
        if !args.positionals.is_empty() {
            return Err(format!(
                "a name filters the review list and means nothing with --{action}; drop \"{}\"",
                args.positionals.join(" ")
            )
            .into());
        }
        let unused = ["all", "limit", "offset", "source"]
            .into_iter()
            .filter(|option| args.has(option))
            .map(|option| format!("--{option}"))
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            return Err(format!(
                "{} only shape the review list and mean nothing with --{action}",
                unused.join(" and ")
            )
            .into());
        }
        let selector = args
            .value(action)
            .ok_or_else(|| format!("--{action} expects the ID printed by aede review"))?;
        let item = match *action {
            "accept" => held.decide(
                &catalog,
                selector,
                ReviewDecision::Accepted,
                aede_core::clock::now_seconds(),
            )?,
            "reject" => held.decide(
                &catalog,
                selector,
                ReviewDecision::Rejected,
                aede_core::clock::now_seconds(),
            )?,
            "undo" => held.clear_review(&catalog, selector)?,
            _ => unreachable!("the action list is closed"),
        };
        sources::save(&held, &path)?;
        print_decision(action, &item, &path);
        return Ok(());
    }

    let only_source = args.value("source");
    let wanted = aede_core::text::normalize(&args.positionals.join(" "));
    let all_items = held.review_items(&catalog);
    let selected = all_items
        .iter()
        .filter(|item| only_source.is_none_or(|source| item.source == source))
        .filter(|item| {
            wanted.is_empty()
                || aede_core::text::normalize(&item.entity.key).contains(&wanted)
                || item
                    .source_id
                    .as_deref()
                    .is_some_and(|id| aede_core::text::normalize(id).contains(&wanted))
        })
        .filter(|item| args.has("all") || item.decision.is_none())
        .collect::<Vec<_>>();

    println!("{}", ui::section("Source review"));
    if selected.is_empty() {
        let resolved = all_items
            .iter()
            .filter(|item| item.decision.is_some())
            .count();
        if resolved > 0 && !args.has("all") && wanted.is_empty() && only_source.is_none() {
            println!("  {}", ui::green("Nothing is waiting for review."));
            println!(
                "  {}",
                ui::dim(&format!(
                    "{} already resolved — aede review --all shows them",
                    ui::plural(resolved, "claim")
                ))
            );
        } else {
            let message = match wanted.is_empty() {
                true => "Nothing needs review.".to_string(),
                false => format!(
                    "Nothing needing review matches \"{}\".",
                    args.positionals.join(" ")
                ),
            };
            println!("  {}", ui::green(&message));
        }
        return Ok(());
    }

    let window = args.window(DEFAULT_LIMIT)?;
    let mut table = Table::new(&["ID", "Entity", "Source", "Why", "Status"])
        .limit(1, 34)
        .limit(3, 46);
    for item in selected.iter().skip(window.offset).take(window.limit) {
        table.push(vec![
            item.id.clone(),
            format!("{} {}", item.entity.kind.as_str(), item.entity.key),
            item.source.clone(),
            reason(item),
            status(item),
        ]);
    }
    print!("{}", table.render());
    announce_window(window, selected.len(), "claim");
    println!(
        "  {}",
        ui::dim("aede review --accept=<ID> trusts it; --reject=<ID> keeps it as evidence only")
    );
    println!(
        "  {}",
        ui::dim("aede review --undo=<ID> makes either decision pending again; tags never change")
    );
    Ok(())
}

fn reason(item: &ReviewItem) -> String {
    match &item.reason {
        ReviewReason::Approximate { score } => format!("approximate match ({score}%)"),
        ReviewReason::IdentityConflict {
            local_id,
            sourced_id,
        } => format!("local {local_id} ≠ source {sourced_id}"),
        ReviewReason::PriorDecision => "explicit decision retained".to_string(),
    }
}

fn status(item: &ReviewItem) -> String {
    match item.decision {
        None => ui::yellow("pending").to_string(),
        Some(ReviewDecision::Accepted) => format!(
            "accepted · {}",
            ui::since(item.reviewed_at.unwrap_or_default())
        ),
        Some(ReviewDecision::Rejected) => format!(
            "rejected · {}",
            ui::since(item.reviewed_at.unwrap_or_default())
        ),
    }
}

fn print_decision(action: &str, item: &ReviewItem, path: &std::path::Path) {
    println!("{}", ui::section("Source review"));
    let effect = match action {
        "accept" => "accepted — this claim may now participate in navigation and queries",
        "reject" => "rejected — it remains visible evidence but is not a graph link",
        "undo" => "returned to pending review",
        _ => unreachable!("the action list is closed"),
    };
    println!(
        "{} {} {} from {}: {effect}",
        ui::green("→"),
        item.entity.kind.as_str(),
        item.entity.key,
        item.source
    );
    println!("  {}", ui::dim(&path.display().to_string()));
}
