//! The `work` command: a composition and the recordings that realize it.

use std::collections::{BTreeMap, BTreeSet};

use aede_core::{model::EntityKind, sources, text};

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
    let family = held.work_parent_links(&catalog);
    let found = catalog.find_works(&query);
    let mut sourced = held.find_sourced_works(&catalog, &query);
    sourced.retain(|work| !found.iter().any(|local| local.mbid == work.mbid));
    let wanted = text::normalize(&query);
    let mut parent_only: BTreeMap<String, String> = BTreeMap::new();
    for link in family.iter().filter(|link| link.trusted) {
        let title = text::normalize(&link.parent.title);
        if link.parent.mbid == query
            || (!wanted.is_empty() && (title == wanted || title.contains(&wanted)))
        {
            let title = parent_only.entry(link.parent.mbid.clone()).or_default();
            if title.is_empty() && !link.parent.title.is_empty() {
                *title = link.parent.title.clone();
            }
        }
    }
    parent_only.retain(|mbid, _| {
        !found.iter().any(|work| &work.mbid == mbid)
            && !sourced.iter().any(|work| &work.mbid == mbid)
    });
    let count = found.len() + sourced.len() + parent_only.len();
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
        print_family(&family, &work.mbid, &mut navigation);
        for link in held
            .work_links(&catalog)
            .into_iter()
            .filter(|link| link.work.mbid == work.mbid)
        {
            let confidence = super::source_status(link.confidence, link.review, link.trusted);
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
            if !link.trusted {
                continue;
            }
            navigation.source_artist(&link.credit.artist_mbid);
        }
        super::print_sourced_credits(&catalog, credits);
        super::panel_for(args, &catalog, EntityKind::Work, work.id);
        navigation.print();
        return Ok(());
    }

    if let Some((mbid, title)) = parent_only.into_iter().next() {
        println!(
            "{}",
            ui::section(if title.is_empty() { &mbid } else { &title })
        );
        println!("  {}", ui::dim(&format!("MusicBrainz work: {mbid}")));
        println!(
            "  {}",
            ui::dim("external part-of-work evidence; local tags are unchanged")
        );
        let mut navigation = Navigation::default();
        print_family(&family, &mbid, &mut navigation);
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
    print_family(&family, &work.mbid, &mut navigation);
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
        if link.trusted {
            navigation.source_artist(&link.credit.artist_mbid);
        }
    }
    super::print_sourced_credits(&catalog, credits);
    navigation.print();
    Ok(())
}

/// Shows only asserted parent/part identities. A matching title never invents
/// a work relationship, and a rejected/uncertain source is evidence but not a
/// navigation target.
fn print_family(
    family: &[sources::SourcedWorkParentLink],
    mbid: &str,
    navigation: &mut Navigation,
) {
    let mut parents: Vec<_> = family
        .iter()
        .filter(|link| link.child_mbid == mbid)
        .collect();
    parents.sort_by(|left, right| {
        left.parent
            .mbid
            .cmp(&right.parent.mbid)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| right.trusted.cmp(&left.trusted))
    });
    parents.dedup_by(|left, right| {
        left.parent.mbid == right.parent.mbid && left.source == right.source
    });
    if !parents.is_empty() {
        println!("{}", ui::section("Part of"));
        let mut rows = Table::new(&["Work", "Position", "Evidence"]);
        for link in parents {
            let title = if link.parent.title.is_empty() {
                &link.parent.mbid
            } else {
                &link.parent.title
            };
            rows.push(vec![
                title.clone(),
                part_position(&link.parent),
                format!(
                    "{} · {}",
                    link.source,
                    super::source_status(link.confidence, link.review, link.trusted)
                ),
            ]);
            if link.trusted {
                navigation.add(
                    "Parent work",
                    format!(
                        "aede work {}",
                        super::navigation::shell_arg(&link.parent.mbid)
                    ),
                );
            }
        }
        print!("{}", rows.render());
    }

    let mut children: Vec<_> = family
        .iter()
        .filter(|link| link.parent.mbid == mbid)
        .collect();
    children.sort_by(|left, right| {
        left.child_mbid
            .cmp(&right.child_mbid)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| right.trusted.cmp(&left.trusted))
    });
    children
        .dedup_by(|left, right| left.child_mbid == right.child_mbid && left.source == right.source);
    children.sort_by(|left, right| {
        left.parent
            .order
            .unwrap_or(u32::MAX)
            .cmp(&right.parent.order.unwrap_or(u32::MAX))
            .then_with(|| left.child_title.cmp(&right.child_title))
            .then_with(|| left.child_mbid.cmp(&right.child_mbid))
    });
    if !children.is_empty() {
        println!("{}", ui::section("Parts in the library"));
        let mut rows = Table::new(&["Part", "Position", "Evidence"]);
        for link in children {
            rows.push(vec![
                if link.child_title.is_empty() {
                    link.child_mbid.clone()
                } else {
                    link.child_title.clone()
                },
                part_position(&link.parent),
                format!(
                    "{} · {}",
                    link.source,
                    super::source_status(link.confidence, link.review, link.trusted)
                ),
            ]);
            if link.trusted {
                navigation.add(
                    "Part",
                    format!(
                        "aede work {}",
                        super::navigation::shell_arg(&link.child_mbid)
                    ),
                );
            }
        }
        print!("{}", rows.render());
    }
}

fn part_position(parent: &sources::WorkParentLink) -> String {
    let mut parts: Vec<String> = parent
        .attributes
        .iter()
        .map(|attribute| attribute.name.clone())
        .collect();
    if let Some(order) = parent.order {
        parts.push(format!("#{order}"));
    }
    if parts.is_empty() {
        "—".into()
    } else {
        parts.join(" · ")
    }
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
