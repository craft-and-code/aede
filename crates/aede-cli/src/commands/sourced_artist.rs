//! A sourced contributor who has no corresponding artist in local tags.

use std::collections::{BTreeMap, BTreeSet};

use aede_core::contributors::SourcedContributor;
use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::sources::{SourcedCreditLink, Sources};

use super::{
    Res, announce_window,
    navigation::{Navigation, shell_arg},
    role_label, selection_output,
};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show(args: &Args, catalog: &Catalog, sources: &Sources, artist: &SourcedContributor) -> Res {
    if args.has("members") {
        return Err("--members needs a local artist whose MusicBrainz membership was fetched; this contributor is known only through recording/work credits".into());
    }
    if args.has("with") {
        return Err("--with currently compares artists known in local tags; sourced contributors do not yet have a collaboration comparison".into());
    }
    let mut links: Vec<SourcedCreditLink> = sources
        .credit_links(catalog)
        .into_iter()
        .filter(|link| link.trusted && link.credit.artist_mbid == artist.mbid)
        .collect();
    let mut edition_links: Vec<_> = sources
        .edition_credit_links(catalog)
        .into_iter()
        .filter(|link| link.trusted && link.credit.artist_mbid == artist.mbid)
        .collect();
    if let Some(role) = args.value("role") {
        let wanted = aede_core::text::normalize(role);
        let offered: BTreeSet<_> = links
            .iter()
            .map(|link| role_label(&link.credit.role))
            .chain(
                edition_links
                    .iter()
                    .map(|link| role_label(&link.credit.role)),
            )
            .collect();
        links.retain(|link| {
            aede_core::text::normalize(&link.credit.role) == wanted
                || aede_core::text::normalize(&role_label(&link.credit.role)) == wanted
        });
        edition_links.retain(|link| {
            aede_core::text::normalize(&link.credit.role) == wanted
                || aede_core::text::normalize(&role_label(&link.credit.role)) == wanted
        });
        if links.is_empty() && edition_links.is_empty() {
            return Err(format!(
                "{} has no sourced credit as {role}. Credited as: {}",
                artist.name,
                offered.into_iter().collect::<Vec<_>>().join(", ")
            )
            .into());
        }
    }
    // Page the same order the shared credit table prints. Otherwise a small
    // --limit would cut source-record order first and sort only that slice.
    links.sort_by(|left, right| {
        left.work
            .as_ref()
            .map(|work| work.title.as_str())
            .cmp(&right.work.as_ref().map(|work| work.title.as_str()))
            .then_with(|| left.credit.order.cmp(&right.credit.order))
            .then_with(|| left.credit.role.cmp(&right.credit.role))
            .then_with(|| left.credit.artist_name.cmp(&right.credit.artist_name))
            .then_with(|| left.recording_id.cmp(&right.recording_id))
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.credit.relation_id.cmp(&right.credit.relation_id))
    });
    let recording_ids: BTreeSet<Id> = links.iter().map(|link| link.recording_id).collect();
    let tracks: Vec<Id> = recording_ids
        .iter()
        .filter_map(|&id| catalog.recording(id))
        .flat_map(|recording| recording.track_ids.iter().copied())
        .chain(
            edition_links
                .iter()
                .filter_map(|link| catalog.release(link.release_id))
                .flat_map(|release| release.track_ids.iter().copied()),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if let Some(result) = selection_output(catalog, &tracks, args) {
        return result;
    }
    super::refuse_output_without_a_format(args)?;
    let window = args.window(30)?;

    println!("{}", ui::section(&artist.name));
    println!(
        "  {}",
        ui::dim(&format!("MusicBrainz artist: {}", artist.mbid))
    );
    println!(
        "  {}",
        ui::dim("source-backed contributor; no matching artist identity in local tags")
    );
    if artist.names.len() > 1 {
        println!(
            "  {}",
            ui::dim(&format!("source spellings: {}", artist.names.join(" · ")))
        );
    }
    println!(
        "  {}",
        ui::plural(recording_ids.len(), "credited local recording")
    );

    let mut albums: BTreeMap<Id, BTreeSet<Id>> = BTreeMap::new();
    for &recording_id in &recording_ids {
        if let Some(recording) = catalog.recording(recording_id) {
            for &track_id in &recording.track_ids {
                if let Some(release_id) = catalog.track(track_id).and_then(|track| track.release_id)
                {
                    albums.entry(release_id).or_default().insert(recording_id);
                }
            }
        }
    }
    for link in &edition_links {
        albums.entry(link.release_id).or_default();
    }
    let mut navigation = Navigation::default();
    if !albums.is_empty() {
        println!("{}", ui::section("Local albums carrying these credits"));
        let mut table = Table::new(&["Year", "Album", "Credited recordings", "Edition credit"])
            .align(2, Align::Right)
            .limit(1, 48);
        for (release_id, recordings) in &albums {
            if let Some(release) = catalog.release(*release_id) {
                table.push(vec![
                    release
                        .year
                        .map_or_else(|| "—".into(), |year| year.to_string()),
                    release.title.clone(),
                    recordings.len().to_string(),
                    if edition_links
                        .iter()
                        .any(|link| link.release_id == *release_id)
                    {
                        "yes".to_string()
                    } else {
                        "—".to_string()
                    },
                ]);
                navigation.entity(catalog, "Album", EntityKind::Release, *release_id);
            }
        }
        print!("{}", table.render());
    }

    for &recording_id in &recording_ids {
        let identity = catalog.recording(recording_id).and_then(|recording| {
            recording.mbid.clone().or_else(|| {
                aede_core::user::EntityRef::of(catalog, EntityKind::Recording, recording_id)
                    .map(|reference| reference.key)
            })
        });
        if let Some(identity) = identity {
            navigation.add(
                "Credited recording",
                format!("aede recording {}", shell_arg(&identity)),
            );
        }
    }
    for link in &links {
        if let Some(work) = &link.work {
            navigation.add(
                "Credited work",
                format!("aede work {}", shell_arg(&work.mbid)),
            );
        }
    }
    let total = links.len();
    super::print_sourced_credits(
        catalog,
        links
            .into_iter()
            .skip(window.offset)
            .take(window.limit)
            .collect(),
    );
    super::print_sourced_edition_credits(catalog, edition_links);
    announce_window(window, total, "source credit");
    navigation.print();
    Ok(())
}
