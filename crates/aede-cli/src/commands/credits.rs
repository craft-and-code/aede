//! Coverage of externally sourced recording and work credits.

use std::collections::BTreeSet;
use std::path::Path;

use aede_core::credit_coverage::{self, CreditStatus, RecordingCreditCoverage};
use aede_core::json::Json;
use aede_core::model::{Catalog, Id, Release};

use super::{Res, announce_window, load, navigation::shell_arg, sources_held};
use crate::args::{Args, Window};
use crate::ui::{self, Align, Table};

#[derive(Default)]
struct Counts {
    credited: usize,
    empty: usize,
    waiting: usize,
    untrusted: usize,
    unidentified: usize,
}

impl Counts {
    fn add(&mut self, status: CreditStatus) {
        match status {
            CreditStatus::Credited => self.credited += 1,
            CreditStatus::Empty => self.empty += 1,
            CreditStatus::Waiting => self.waiting += 1,
            CreditStatus::Untrusted => self.untrusted += 1,
            CreditStatus::Unidentified => self.unidentified += 1,
        }
    }

    fn total(&self) -> usize {
        self.credited + self.empty + self.waiting + self.untrusted + self.unidentified
    }

    fn json(&self) -> Json {
        let mut object = Json::obj();
        object.set("total", (self.total() as u64).into());
        object.set("credited", (self.credited as u64).into());
        object.set("empty", (self.empty as u64).into());
        object.set("waiting", (self.waiting as u64).into());
        object.set("untrusted", (self.untrusted as u64).into());
        object.set("unidentified", (self.unidentified as u64).into());
        object
    }
}

fn album_recordings<'a>(
    catalog: &Catalog,
    release: &Release,
    coverage: &'a [RecordingCreditCoverage],
) -> Vec<&'a RecordingCreditCoverage> {
    let mut seen = BTreeSet::new();
    release
        .track_ids
        .iter()
        .filter_map(|&track_id| catalog.track(track_id))
        .filter(|track| seen.insert(track.recording_id))
        .filter_map(|track| coverage.get(track.recording_id as usize))
        .collect()
}

fn counts(rows: impl IntoIterator<Item = CreditStatus>) -> Counts {
    let mut result = Counts::default();
    for status in rows {
        result.add(status);
    }
    result
}

fn file_for_recording<'a>(
    catalog: &'a Catalog,
    release: &Release,
    recording_id: Id,
) -> Option<&'a str> {
    release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .find(|track| track.recording_id == recording_id)
        .and_then(|track| catalog.file(track.file_id))
        .map(|file| file.path.as_str())
}

/// Summarize the whole catalog, or inspect recordings in matching albums.
pub fn show_credits(args: &Args) -> Res {
    let catalog = load(args)?;
    let sources = sources_held(args)?;
    let coverage = credit_coverage::recordings(&catalog, &sources);
    let query = args.positionals.join(" ");
    if !query.is_empty() {
        let wanted = aede_core::text::normalize(&query);
        let canonical_path = super::canonical(Path::new(&query));
        let albums: Vec<_> = catalog
            .releases
            .iter()
            .filter(|release| {
                release.mbid.as_deref() == Some(query.as_str())
                    || aede_core::text::normalize(&release.title).contains(&wanted)
                    || Path::new(&release.folder) == canonical_path
            })
            .collect();
        if albums.is_empty() {
            return Err(format!("no album matches \"{query}\"").into());
        }
        let window = args.window(usize::MAX)?;
        if args.has("json") {
            println!(
                "{}",
                Json::Arr(
                    albums
                        .iter()
                        .map(|album| album_json(&catalog, album, &coverage, Some(window)))
                        .collect()
                )
                .to_string_pretty()
            );
            return Ok(());
        }
        for album in albums {
            show_album(&catalog, album, &coverage, window);
        }
        return Ok(());
    }

    let overall = counts(coverage.iter().map(|row| row.status));
    let window = args.window(25)?;
    if args.has("json") {
        let mut result = Json::obj();
        result.set("recordings", overall.json());
        result.set(
            "albums",
            Json::Arr(
                catalog
                    .releases
                    .iter()
                    .skip(window.offset)
                    .take(window.limit)
                    .map(|release| album_json(&catalog, release, &coverage, None))
                    .collect(),
            ),
        );
        println!("{}", result.to_string_pretty());
        return Ok(());
    }
    println!("{}", ui::section("MusicBrainz credit coverage"));
    println!(
        "  {} recordings · {} credited · {} queried empty · {} waiting · {} untrusted · {} without ID",
        overall.total(),
        overall.credited,
        overall.empty,
        overall.waiting,
        overall.untrusted,
        overall.unidentified
    );
    let mut table = Table::new(&[
        "Album",
        "Total",
        "With",
        "Empty",
        "Waiting",
        "Untrusted",
        "No ID",
    ])
    .align(1, Align::Right)
    .align(2, Align::Right)
    .align(3, Align::Right)
    .align(4, Align::Right)
    .align(5, Align::Right)
    .align(6, Align::Right);
    for release in catalog
        .releases
        .iter()
        .skip(window.offset)
        .take(window.limit)
    {
        let tally = counts(
            album_recordings(&catalog, release, &coverage)
                .iter()
                .map(|row| row.status),
        );
        table.push(vec![
            release.title.clone(),
            tally.total().to_string(),
            tally.credited.to_string(),
            tally.empty.to_string(),
            tally.waiting.to_string(),
            tally.untrusted.to_string(),
            tally.unidentified.to_string(),
        ]);
    }
    print!("{}", table.render());
    announce_window(window, catalog.releases.len(), "album");
    println!(
        "  {}",
        ui::dim("aede credits \"<album>\" shows each recording and a targeted fetch command")
    );
    Ok(())
}

fn show_album(
    catalog: &Catalog,
    release: &Release,
    coverage: &[RecordingCreditCoverage],
    window: Window,
) {
    let rows = album_recordings(catalog, release, coverage);
    let tally = counts(rows.iter().map(|row| row.status));
    println!("{}", ui::section(&release.title));
    println!("  {}", ui::dim(&release.folder));
    println!(
        "  {} recordings · {} credited · {} queried empty · {} waiting · {} untrusted · {} without ID",
        tally.total(),
        tally.credited,
        tally.empty,
        tally.waiting,
        tally.untrusted,
        tally.unidentified
    );
    let mut table = Table::new(&["Recording", "Status", "Direct", "Work", "File"])
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(4, 36);
    for row in rows.iter().skip(window.offset).take(window.limit) {
        let title = catalog
            .recording(row.recording_id)
            .map_or("—", |r| r.title.as_str());
        table.push(vec![
            title.to_string(),
            row.status.as_str().to_string(),
            row.recording_credits.to_string(),
            row.work_credits.to_string(),
            file_for_recording(catalog, release, row.recording_id)
                .and_then(|path| Path::new(path).file_name())
                .map_or_else(
                    || "—".to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
        ]);
    }
    print!("{}", table.render());
    announce_window(window, rows.len(), "recording");
    if tally.waiting > 0 {
        println!(
            "  {}",
            ui::dim(&format!(
                "Fetch waiting recordings: aede fetch --credits {}",
                shell_arg(&release.folder)
            ))
        );
    }
    if tally.untrusted > 0 {
        println!(
            "  {}",
            ui::dim("Inspect pending or rejected source identities: aede review --all")
        );
    }
    if tally.unidentified > 0 {
        println!(
            "  {}",
            ui::dim("Recordings without a MusicBrainz ID cannot be fetched by --credits")
        );
    }
}

fn album_json(
    catalog: &Catalog,
    release: &Release,
    coverage: &[RecordingCreditCoverage],
    detail_window: Option<Window>,
) -> Json {
    let rows = album_recordings(catalog, release, coverage);
    let tally = counts(rows.iter().map(|row| row.status));
    let mut result = Json::obj();
    result.set("album", release.title.clone().into());
    result.set("folder", release.folder.clone().into());
    result.set("musicbrainz_id", release.mbid.clone().into());
    result.set("coverage", tally.json());
    if let Some(window) = detail_window {
        result.set(
            "recordings",
            Json::Arr(
                rows.into_iter()
                    .skip(window.offset)
                    .take(window.limit)
                    .filter_map(|row| {
                        let recording = catalog.recording(row.recording_id)?;
                        let mut item = Json::obj();
                        item.set("title", recording.title.clone().into());
                        item.set("musicbrainz_id", recording.mbid.clone().into());
                        item.set("status", row.status.as_str().into());
                        item.set(
                            "file",
                            file_for_recording(catalog, release, row.recording_id)
                                .map(str::to_string)
                                .into(),
                        );
                        item.set("recording_credits", (row.recording_credits as u64).into());
                        item.set("work_credits", (row.work_credits as u64).into());
                        Some(item)
                    })
                    .collect(),
            ),
        );
    }
    result
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
