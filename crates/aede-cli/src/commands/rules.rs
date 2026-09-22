//! Portable personal corrections and decisions.
//!
//! A backup preserves everything. A rules bundle is deliberately narrower:
//! the small set of human decisions that can be replayed on another catalog
//! without copying listening history or re-fetched biographies and artwork.

use aede_core::json::Json;
use aede_core::{sources, user};

use super::{Res, data_dir};
use crate::args::Args;
use crate::ui::{self, Table};

const RULES_FORMAT_VERSION: u32 = 1;

/// Lists, exports, or imports reproducible personal rules.
pub fn rules(args: &Args) -> Res {
    if args.has("export") && args.has("import") {
        return Err("--export and --import ask for opposite operations".into());
    }
    if args.value("output").is_some() && !args.has("export") {
        return Err("--output means nothing without --export on rules".into());
    }
    let directory = data_dir(args);
    let user_path = user::user_path(&directory);
    let sources_path = sources::sources_path(&directory);
    let mut personal = user::load(&user_path)?.unwrap_or_default();
    let mut evidence = sources::load(&sources_path)?.unwrap_or_default();

    if args.has("export") {
        return export(args, &personal, &evidence);
    }
    if let Some(path) = args.value("import") {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read \"{path}\": {error}"))?;
        let root = aede_core::json::parse(&text)
            .map_err(|error| format!("\"{path}\" is not JSON: {error}"))?;
        if root.field_str("format").as_deref() != Some("aede-rules") {
            return Err("this is not an Aède rules bundle".into());
        }
        let version = root.field_u32("format_version").unwrap_or(0);
        if version != RULES_FORMAT_VERSION {
            return Err(format!(
                "rules format version {version} is not supported; expected {RULES_FORMAT_VERSION}"
            )
            .into());
        }
        let incoming_user = user::from_json(
            root.get("user")
                .ok_or_else(|| "rules bundle has no user document".to_string())?,
        )?;
        let incoming_sources = sources::from_json(
            root.get("sources")
                .ok_or_else(|| "rules bundle has no sources document".to_string())?,
        )?;

        let report = user::merge(&mut personal, incoming_user);
        let mut source_changes = 0usize;
        for record in incoming_sources.records {
            evidence.set(record);
            source_changes += 1;
        }
        for review in incoming_sources.reviews {
            evidence.set_review(review);
            source_changes += 1;
        }
        user::save(&personal, &user_path)?;
        sources::save(&evidence, &sources_path)?;
        println!(
            "{} {} added, {} updated, {} kept, {} source decisions imported",
            ui::green("→"),
            report.added,
            report.updated,
            report.kept,
            source_changes
        );
        return Ok(());
    }

    print_summary(&personal, &evidence);
    Ok(())
}

fn export(args: &Args, personal: &user::UserData, evidence: &sources::Sources) -> Res {
    let portable_user = user::UserData {
        relation_annotations: personal.relation_annotations.clone(),
        collections: personal.collections.clone(),
        set_aside: personal.set_aside.clone(),
        same_artist: personal.same_artist.clone(),
        ..Default::default()
    };
    let portable_sources = sources::Sources {
        records: evidence
            .records
            .iter()
            .filter(|record| record.source == "manual")
            .cloned()
            .collect(),
        reviews: evidence.reviews.clone(),
    };
    let mut root = Json::obj();
    root.set("format", "aede-rules".into());
    root.set("format_version", RULES_FORMAT_VERSION.into());
    root.set("exported_at", aede_core::clock::now_seconds().into());
    root.set("user", user::to_json(&portable_user));
    root.set("sources", sources::to_json(&portable_sources));
    super::export::emit(args, &root.to_string_pretty())
}

fn print_summary(personal: &user::UserData, evidence: &sources::Sources) {
    let accepted = evidence
        .reviews
        .iter()
        .filter(|review| review.decision == sources::ReviewDecision::Accepted)
        .count();
    let rejected = evidence.reviews.len().saturating_sub(accepted);
    let manual = evidence
        .records
        .iter()
        .filter(|record| record.source == "manual")
        .count();
    println!("{}", ui::section("Personal rules"));
    let mut table = Table::new(&["Rule", "Count"]);
    for (name, count) in [
        ("accepted source identities", accepted),
        ("rejected source identities", rejected),
        ("manual source records", manual),
        ("artist filing rules", personal.same_artist.len()),
        ("missing releases set aside", personal.set_aside.len()),
        ("relation annotations", personal.relation_annotations.len()),
        ("saved queries", personal.collections.len()),
    ] {
        table.push(vec![name.into(), count.to_string()]);
    }
    print!("{}", table.render());
    println!(
        "  {}",
        ui::dim("aede rules --export --output=rules.json makes these decisions portable")
    );
}
