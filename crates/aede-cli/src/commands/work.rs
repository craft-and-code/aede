//! The `work` command: a composition and the recordings that realize it.

use std::collections::BTreeSet;

use aede_core::{model::EntityKind, sources};

use super::{Res, data_dir, load, navigation::Navigation};
use crate::args::Args;
use crate::ui::{self, Table};

pub fn show_work(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give a work title or MusicBrainz ID".into());
    }
    let held = sources::load(&sources::sources_path(&data_dir(args)))?.unwrap_or_default();
    let found = catalog.find_works(&query);
    let mut sourced = held.find_sourced_works(&catalog, &query);
    sourced.retain(|work| !found.iter().any(|local| local.mbid == work.mbid));
    let count = found.len() + sourced.len();
    if count == 0 {
        return Err(format!("no work matches \"{query}\"").into());
    }
    if count > 1 {
        return Err(format!("{count} works match \"{query}\"; use its MusicBrainz ID").into());
    }

    if let Some(work) = found.first() {
        println!("{}", ui::section(&work.title));
        println!("  {}", ui::dim(&format!("MusicBrainz work: {}", work.mbid)));
        let recording_ids: BTreeSet<_> = work.recording_ids.iter().copied().collect();
        print_recordings(&catalog, &recording_ids);
        let mut navigation = Navigation::default();
        for &recording_id in &recording_ids {
            navigation.entity(&catalog, "Recording", EntityKind::Recording, recording_id);
        }
        for link in held
            .work_links(&catalog)
            .into_iter()
            .filter(|link| link.work.mbid == work.mbid)
        {
            let confidence = match link.confidence {
                sources::Confidence::Identified => "identified".to_string(),
                sources::Confidence::Matched(score) => format!("matched {score}%"),
            };
            println!(
                "  {}",
                ui::dim(&format!(
                    "source evidence: {} · {} · {}",
                    link.source,
                    confidence,
                    ui::since(link.fetched_at)
                ))
            );
        }
        let credits: Vec<_> = held
            .credit_links(&catalog)
            .into_iter()
            .filter(|link| {
                link.work
                    .as_ref()
                    .is_some_and(|linked| linked.mbid == work.mbid)
            })
            .collect();
        for link in &credits {
            if let Some(artist) = catalog
                .artists
                .iter()
                .find(|artist| artist.mbid.as_deref() == Some(&link.credit.artist_mbid))
            {
                navigation.entity(&catalog, "Credited artist", EntityKind::Artist, artist.id);
            }
        }
        super::print_sourced_credits(&catalog, credits);
        super::panel_for(args, &catalog, EntityKind::Work, work.id);
        navigation.print();
        return Ok(());
    }

    let work = &sourced[0];
    let title = if work.title.is_empty() {
        "Untitled work"
    } else {
        &work.title
    };
    println!("{}", ui::section(title));
    println!("  {}", ui::dim(&format!("MusicBrainz work: {}", work.mbid)));
    println!(
        "  {}",
        ui::dim("external evidence; local tags are unchanged")
    );
    let recording_ids: BTreeSet<_> = work.links.iter().map(|link| link.recording_id).collect();
    print_recordings(&catalog, &recording_ids);
    let mut navigation = Navigation::default();
    for &recording_id in &recording_ids {
        navigation.entity(&catalog, "Recording", EntityKind::Recording, recording_id);
    }
    for link in &work.links {
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
            "  {}",
            ui::dim(&format!(
                "{} · identified · {}{qualified}",
                link.source,
                ui::since(link.fetched_at)
            ))
        );
    }
    let credits: Vec<_> = held
        .credit_links(&catalog)
        .into_iter()
        .filter(|link| {
            link.work
                .as_ref()
                .is_some_and(|linked| linked.mbid == work.mbid)
        })
        .collect();
    for link in &credits {
        if let Some(artist) = catalog
            .artists
            .iter()
            .find(|artist| artist.mbid.as_deref() == Some(&link.credit.artist_mbid))
        {
            navigation.entity(&catalog, "Credited artist", EntityKind::Artist, artist.id);
        }
    }
    super::print_sourced_credits(&catalog, credits);
    navigation.print();
    Ok(())
}

fn print_recordings(catalog: &aede_core::model::Catalog, recording_ids: &BTreeSet<u32>) {
    let mut rows = Table::new(&["Recording", "MusicBrainz ID", "Placements"]);
    for &recording_id in recording_ids {
        if let Some(recording) = catalog.recording(recording_id) {
            rows.push(vec![
                recording.title.clone(),
                recording.mbid.clone().unwrap_or_else(|| "—".into()),
                recording.track_ids.len().to_string(),
            ]);
        }
    }
    println!("{}", rows.render());
}
