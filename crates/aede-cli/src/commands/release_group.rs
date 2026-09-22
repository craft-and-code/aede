//! The `release-group` command: the album identity shared by local editions.

use std::collections::BTreeSet;

use aede_core::model::{EDITION_OF, EntityKind, HAS_EDITION};
use aede_core::text;

use super::{Res, load, navigation::Navigation, totals};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show_release_group(args: &Args) -> Res {
    let catalog = load(args)?;
    let query = args.positionals.join(" ");
    if query.trim().is_empty() {
        return Err("give a release-group title or MusicBrainz ID".into());
    }
    let found = catalog.find_release_groups(&query);
    if found.is_empty() {
        return Err(format!("no release group matches \"{query}\"").into());
    }
    if found.len() > 1 {
        let choices = found
            .iter()
            .map(|group| format!("\n\t{} · {}", group.title, group.mbid))
            .collect::<String>();
        return Err(format!(
            "{} release groups match \"{query}\"; use the MusicBrainz ID:{choices}",
            found.len()
        )
        .into());
    }
    let group = found[0];
    println!("{}", ui::section(&group.title));
    println!(
        "  {}",
        ui::dim(&format!("MusicBrainz release group: {}", group.mbid))
    );

    let mut edition_ids = catalog.related_entities(
        EntityKind::ReleaseGroup,
        group.id,
        HAS_EDITION,
        EntityKind::Release,
    );
    edition_ids.sort_unstable();
    edition_ids.dedup();

    println!("{}", ui::section("Local editions"));
    let mut table = Table::new(&[
        "Year", "Edition", "Identity", "Artist", "Tracks", "Duration", "Size", "Format",
    ])
    .align(4, Align::Right)
    .align(5, Align::Right)
    .align(6, Align::Right)
    .limit(1, 38)
    .limit(2, 38)
    .limit(3, 28)
    .limit(7, 28);
    let mut navigation = Navigation::default();
    let mut artists = BTreeSet::new();
    for edition_id in edition_ids {
        let Some(release) = catalog.release(edition_id) else {
            continue;
        };
        // The inverse edge must agree with the edge used to reach this row.
        if !catalog
            .related_entities(
                EntityKind::Release,
                release.id,
                EDITION_OF,
                EntityKind::ReleaseGroup,
            )
            .contains(&group.id)
        {
            continue;
        }
        let (duration, size) = totals(&catalog, &release.track_ids);
        let formats: BTreeSet<String> = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .filter_map(|track| catalog.file(track.file_id))
            .map(|file| file.properties.quality_label())
            .collect();
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|artist| artist.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        if let Some(artist_id) = release.album_artist_id
            && artists.insert(artist_id)
        {
            navigation.entity(&catalog, "Album artist", EntityKind::Artist, artist_id);
        }
        navigation.entity(&catalog, "Edition", EntityKind::Release, release.id);
        table.push(vec![
            release
                .year
                .map(|year| year.to_string())
                .unwrap_or_else(|| "—".into()),
            release.title.clone(),
            release.mbid.clone().unwrap_or_else(|| "local".into()),
            artist,
            release.track_ids.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            formats.into_iter().collect::<Vec<_>>().join(", "),
        ]);
    }
    print!("{}", table.render());
    super::panel_for(args, &catalog, EntityKind::ReleaseGroup, group.id);
    navigation.print();
    Ok(())
}
