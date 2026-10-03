//! Human and machine-readable descriptions of the proposed or published scan.

use std::path::Path;

use aede_core::json::Json;
use aede_core::model::Catalog;
use aede_core::scan::ScanReport;
use aede_core::stats;

use crate::ui::{self, Align, Table};

pub(super) fn print(
    report: &ScanReport,
    catalog: &Catalog,
    path: &Path,
    dry_run: bool,
    machine: bool,
) {
    if machine {
        println!("{}", as_json(report, catalog, dry_run).to_string_compact());
        return;
    }
    println!(
        "{}",
        ui::section(if dry_run {
            "Scan preview"
        } else {
            "Scan complete"
        })
    );
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Files found".into(), report.found.to_string()]);
    table.push(vec!["Read from disk".into(), report.read.to_string()]);
    table.push(vec![
        "Reused from previous scan".into(),
        report.reused.to_string(),
    ]);
    if report.preserved > 0 {
        table.push(vec![
            "Kept from inaccessible paths".into(),
            report.preserved.to_string(),
        ]);
    }
    if report.removed > 0 {
        table.push(vec![
            "Gone since last scan".into(),
            report.removed.to_string(),
        ]);
    }
    if report.reports > 0 {
        table.push(vec![
            "Analyses imported".into(),
            format!(
                "{} from {}",
                report.analyses,
                ui::plural(report.reports, "report")
            ),
        ]);
    }
    if report.attached > 0 {
        table.push(vec![
            "Analyses now attached".into(),
            report.attached.to_string(),
        ]);
    }
    table.push(vec!["Elapsed".into(), ui::elapsed(report.elapsed_ms)]);
    print!("{}", table.render());

    if dry_run {
        paths("Would add", &report.added_paths);
        paths("Would reread changed files", &report.changed_paths);
        paths("Would remove", &report.removed_paths);
    }
    if !report.failures.is_empty() {
        println!("{}", ui::section("Unreadable paths"));
        let mut table = Table::new(&["Path", "Reason"]).path_limit(0, 60);
        for (path, reason) in report.failures.iter().take(20) {
            table.push(vec![path.clone(), reason.clone()]);
        }
        print!("{}", table.render());
        remaining(report.failures.len());
    }
    let totals = stats::compute(catalog);
    println!(
        "\n{} {} · {} · {} · {}",
        ui::green("→"),
        ui::plural(totals.tracks, "track"),
        ui::plural(totals.releases, "album"),
        ui::plural(totals.artists, "artist"),
        ui::long_duration(totals.total_duration_ms)
    );
    if dry_run {
        println!(
            "{}",
            ui::dim("  preview only: catalog, watched folders and personal data were not changed")
        );
    }
    println!("{}", ui::dim(&format!("  catalog: {}", path.display())));
}

fn paths(title: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    println!("{}", ui::section(&format!("{title} ({})", paths.len())));
    let mut table = Table::new(&["File"]).path_limit(0, 80);
    for path in paths.iter().take(20) {
        table.push(vec![path.clone()]);
    }
    print!("{}", table.render());
    remaining(paths.len());
}

fn remaining(count: usize) {
    if count > 20 {
        println!("{}", ui::dim(&format!("  … and {} more", count - 20)));
    }
}

pub(super) fn as_json(report: &ScanReport, catalog: &Catalog, dry_run: bool) -> Json {
    let mut result = Json::obj();
    result.set("dry_run", dry_run.into());
    result.set("roots", strings(&catalog.roots));
    result.set("added", strings(&report.added_paths));
    result.set("changed", strings(&report.changed_paths));
    result.set("removed", strings(&report.removed_paths));
    result.set(
        "unreadable",
        Json::Arr(
            report
                .failures
                .iter()
                .map(|(path, reason)| {
                    let mut failure = Json::obj();
                    failure.set("path", path.as_str().into());
                    failure.set("reason", reason.as_str().into());
                    failure
                })
                .collect(),
        ),
    );
    let mut counts = Json::obj();
    for (name, count) in [
        ("found", report.found),
        ("read", report.read),
        ("reused", report.reused),
        ("preserved", report.preserved),
        ("added", report.added_paths.len()),
        ("changed", report.changed_paths.len()),
        ("removed", report.removed),
        ("catalogued", catalog.files.len()),
    ] {
        counts.set(name, (count as u64).into());
    }
    result.set("counts", counts);
    result
}

fn strings(paths: &[String]) -> Json {
    Json::Arr(paths.iter().map(|path| path.as_str().into()).collect())
}

#[cfg(test)]
#[path = "scan_report_tests.rs"]
mod tests;
