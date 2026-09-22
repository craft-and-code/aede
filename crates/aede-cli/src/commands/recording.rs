//! The `recording` command: one recorded performance and its local placements.

use aede_core::sources;

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Table};

pub fn show_recording(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give a recording title or MusicBrainz ID".into());
    }
    let found = catalog.find_recordings(&query);
    if found.is_empty() {
        return Err(format!("no recording matches \"{query}\"").into());
    }
    if found.len() > 1 {
        return Err(format!(
            "{} recordings match \"{query}\"; use its MusicBrainz ID",
            found.len()
        )
        .into());
    }
    let recording = found[0];
    println!("{}", ui::section(&recording.title));
    if let Some(mbid) = &recording.mbid {
        println!("  {}", ui::dim(&format!("MusicBrainz recording: {mbid}")));
    }
    let mut placements = Table::new(&["Album", "Track", "File"]);
    for &track_id in &recording.track_ids {
        let Some(track) = catalog.track(track_id) else {
            continue;
        };
        let album = track
            .release_id
            .and_then(|id| catalog.release(id))
            .map(|r| r.title.as_str())
            .unwrap_or("—");
        let file = catalog
            .file(track.file_id)
            .map(|f| f.path.as_str())
            .unwrap_or("—");
        placements.push(vec![
            album.to_string(),
            track.title.clone(),
            file.to_string(),
        ]);
    }
    println!("{}", placements.render());
    for &work_id in &recording.work_ids {
        if let Some(work) = catalog.work(work_id) {
            println!("  work: {} ({})", work.title, work.mbid);
        }
    }
    let held = sources::load(&sources::sources_path(&data_dir(args)))?.unwrap_or_default();
    for link in held
        .work_links(&catalog)
        .into_iter()
        .filter(|link| link.recording_id == recording.id)
    {
        println!(
            "  source work: {} ({}) — {} · {} · {}",
            link.work.title,
            link.work.mbid,
            link.source,
            confidence_label(link.confidence),
            ui::since(link.fetched_at)
        );
    }
    Ok(())
}

fn confidence_label(confidence: sources::Confidence) -> String {
    match confidence {
        sources::Confidence::Identified => "identified".into(),
        sources::Confidence::Matched(score) => format!("matched {score}%"),
    }
}
