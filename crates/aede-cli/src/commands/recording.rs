//! The `recording` command: one recorded performance and its local placements.

use aede_core::model::EntityKind;
use aede_core::sources;

use super::{Res, data_dir, load, navigation::Navigation, navigation::shell_arg};
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
    let mut navigation = Navigation::default();
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
        if let Some(release_id) = track.release_id {
            navigation.entity(&catalog, "Album placement", EntityKind::Release, release_id);
        }
        navigation.entity(&catalog, "Track placement", EntityKind::Track, track.id);
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
            navigation.entity(&catalog, "Work", EntityKind::Work, work.id);
        }
    }
    let held = sources::load(&sources::sources_path(&data_dir(args)))?.unwrap_or_default();
    for link in held
        .work_links(&catalog)
        .into_iter()
        .filter(|link| link.recording_id == recording.id)
    {
        let attributes = link
            .work
            .attributes
            .iter()
            .map(|attribute| attribute.name.as_str())
            .collect::<Vec<_>>();
        let qualified = match attributes.is_empty() {
            true => String::new(),
            false => format!(" · {}", attributes.join(", ")),
        };
        println!(
            "  source work: {} ({}){qualified} — {} · {} · {}",
            link.work.title,
            link.work.mbid,
            link.source,
            super::source_status(link.confidence, link.review, link.trusted),
            ui::since(link.fetched_at)
        );
        if link.trusted {
            navigation.add(
                "Source work",
                format!("aede work {}", shell_arg(&link.work.mbid)),
            );
        }
    }
    let source_credits: Vec<_> = held
        .credit_links(&catalog)
        .into_iter()
        .filter(|link| link.recording_id == recording.id)
        .collect();
    for link in &source_credits {
        if !link.trusted {
            continue;
        }
        if let Some(artist) = catalog
            .artists
            .iter()
            .find(|artist| artist.mbid.as_deref() == Some(&link.credit.artist_mbid))
        {
            navigation.entity(&catalog, "Credited artist", EntityKind::Artist, artist.id);
        }
    }
    super::print_sourced_credits(&catalog, source_credits);
    super::panel_for(args, &catalog, EntityKind::Recording, recording.id);
    navigation.print();
    Ok(())
}
