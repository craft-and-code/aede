//! The `doctor` command: what is wrong with the library.

use aede_core::doctor::{self, Issue, Severity};
use aede_core::json::Json;

use super::{Res, announce_window, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show_doctor(args: &Args) -> Res {
    let catalog = load(args)?;
    let mut issues = doctor::diagnose(&catalog);

    if let Some(filter) = args.value("severity") {
        let wanted = match filter {
            "error" | "errors" => Some(Severity::Error),
            "warning" | "warnings" => Some(Severity::Warning),
            "info" | "infos" => Some(Severity::Info),
            _ => None,
        };
        if let Some(wanted) = wanted {
            issues.retain(|i| i.severity() == wanted);
        }
    }

    if args.has("json") {
        println!("{}", issues_to_json(&issues).to_string_pretty());
        return Ok(());
    }

    let summary = doctor::summary(&issues);
    println!("{}", ui::section("Diagnosis"));
    if issues.is_empty() {
        println!("  {}", ui::green("No issue found."));
        print_unverified(&catalog);
        print_waiting_analyses(&catalog);
        return Ok(());
    }
    for (severity, count) in &summary {
        let line = ui::plural(*count, severity.label());
        let coloured = match severity {
            Severity::Error => ui::red(&line),
            Severity::Warning => ui::yellow(&line),
            Severity::Info => ui::dim(&line),
        };
        println!("  {coloured}");
    }

    // Grouping by kind: a damaged library produces thousands of lines, so the
    // summary comes first.
    println!("{}", ui::section("By issue type"));
    let mut by_kind: std::collections::BTreeMap<&str, usize> = Default::default();
    for issue in &issues {
        *by_kind.entry(issue.kind.label()).or_insert(0) += 1;
    }
    let mut t = Table::new(&["Issue", "Count"]).align(1, Align::Right);
    let mut rows: Vec<(&str, usize)> = by_kind.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (label, count) in rows {
        t.push(vec![label.to_string(), count.to_string()]);
    }
    print!("{}", t.render());

    let window = args.window(25)?;
    println!("{}", ui::section("Details"));
    for issue in issues.iter().skip(window.offset).take(window.limit) {
        let mark = match issue.severity() {
            Severity::Error => ui::red("✗"),
            Severity::Warning => ui::yellow("!"),
            Severity::Info => ui::dim("·"),
        };
        println!(
            "  {mark} {} — {}",
            ui::bold(issue.kind.label()),
            issue.detail
        );
        for file in issue.files.iter().take(4) {
            println!("      {}", ui::dim(file));
        }
        if issue.files.len() > 4 {
            println!(
                "      {}",
                ui::dim(&format!("… and {} more", issue.files.len() - 4))
            );
        }
    }
    println!();
    announce_window(window, issues.len(), "issue");
    print_unverified(&catalog);
    print_waiting_analyses(&catalog);
    Ok(())
}

fn issues_to_json(issues: &[Issue]) -> Json {
    Json::Arr(
        issues
            .iter()
            .map(|i| {
                let mut o = Json::obj();
                o.set("type", i.kind.label().into());
                o.set("severity", i.severity().label().into());
                o.set("detail", i.detail.clone().into());
                o.set(
                    "files",
                    Json::Arr(i.files.iter().map(|f| Json::Str(f.clone())).collect()),
                );
                o
            })
            .collect(),
    )
}

/// Says how many files carry no integrity verdict.
///
/// `doctor` reads no file, so it can only report what `aede check` established.
/// Staying silent would let a library look healthy when nothing was verified.
fn print_unverified(catalog: &aede_core::model::Catalog) {
    let unverified = catalog
        .files
        .iter()
        .filter(|f| f.integrity.is_none())
        .count();
    if unverified == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} not verified — run aede check",
            ui::plural(unverified, "file")
        ))
    );
}

/// Says how many imported analyses describe files the catalog does not hold.
///
/// They are not lost and they are not a defect: they are waiting for the folder
/// they name to be scanned. Saying so is the difference between "waiting" and
/// "swallowed without a word".
fn print_waiting_analyses(catalog: &aede_core::model::Catalog) {
    let waiting = catalog.pending_analyses();
    if waiting == 0 {
        return;
    }
    println!(
        "  {}",
        ui::dim(&format!(
            "{} waiting for the folders they name to be scanned",
            ui::plural(waiting, "imported analysis")
        ))
    );
}
