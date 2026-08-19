//! The `album` command: one page per release.

use aede_core::model::EntityKind;
use aede_core::text;

use super::{Res, load, role_label, selection_output};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show_album(args: &Args) -> Res {
    let catalog = load(args)?;
    let title = args.positionals.join(" ");
    if title.trim().is_empty() {
        return Err("give a title: aede album \"Kind of Blue\"".into());
    }
    let Some(release) = catalog.find_release(&title).or_else(|| {
        catalog
            .search(&title, 1)
            .first()
            .filter(|h| h.kind == EntityKind::Release)
            .and_then(|h| catalog.release(h.id))
    }) else {
        // `album` describes one album; the words are joined so that a title can
        // be typed without quotes. Someone naming two of them is after a list,
        // which is what the plural command is for.
        if args.positionals.len() > 1 {
            return Err(format!(
                "no album matches \"{title}\".\n\
                 aede album takes one title; for several, filter the list:\n\
                 \taede albums --csv --artist=\"<name>\"\n\
                 \taede albums --csv --year=1990"
            )
            .into());
        }
        return Err(format!("no album matches \"{title}\"").into());
    };

    let artist = release
        .album_artist_id
        .and_then(|id| catalog.artist(id))
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "Various Artists".into());

    if let Some(result) = selection_output(&catalog, &release.track_ids, args) {
        return result;
    }

    println!("{}", ui::section(&release.title));
    println!("  {}", ui::bold(&artist));
    if let Some(year) = release.year {
        println!("  {year}");
    }
    let labels: Vec<String> = release
        .label_ids
        .iter()
        .filter_map(|&id| catalog.label(id))
        .map(|l| l.name.clone())
        .collect();
    if !labels.is_empty() {
        let mut line = labels.join(", ");
        if let Some(cat) = &release.catalog_number {
            line.push_str(&format!(" — {cat}"));
        }
        println!("  {}", ui::dim(&line));
    }
    let genres: Vec<String> = catalog
        .genres_of(EntityKind::Release, release.id)
        .into_iter()
        .map(|g| g.name.clone())
        .collect();
    if !genres.is_empty() {
        println!("  {}", ui::dim(&genres.join(", ")));
    }
    println!("  {}", ui::dim(&release.folder));

    println!("{}", ui::section("Tracks"));
    let mut t = Table::new(&["#", "Title", "Duration", "Size", "Format", "Artists"])
        .align(0, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(1, 45)
        .limit(5, 35);
    for &track_id in &release.track_ids {
        let Some(track) = catalog.track(track_id) else {
            continue;
        };
        let file = catalog.file(track.file_id);
        let performers: Vec<String> = catalog
            .credits_on(EntityKind::Track, track_id)
            .into_iter()
            .filter(|(_, role)| *role == "main")
            .map(|(a, _)| a.name.clone())
            .collect();
        t.push(vec![
            track
                .track_no
                .map(|n| n.to_string())
                .unwrap_or_else(|| "—".into()),
            track.title.clone(),
            track
                .duration_ms
                .map(text::format_duration)
                .unwrap_or_else(|| "—".into()),
            file.map(|f| text::format_size(f.size)).unwrap_or_default(),
            file.map(|f| f.properties.quality_label())
                .unwrap_or_default(),
            performers.join(", "),
        ]);
    }
    print!("{}", t.render());

    let duration: u64 = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| t.duration_ms)
        .sum();
    let size: u64 = release
        .track_ids
        .iter()
        .filter_map(|&id| catalog.track(id))
        .filter_map(|t| catalog.file(t.file_id))
        .map(|f| f.size)
        .sum();
    println!(
        "  {}",
        ui::dim(&format!(
            "{} · {} · {}",
            ui::plural(release.track_ids.len(), "track"),
            text::format_duration(duration),
            text::format_size(size)
        ))
    );

    // Credits other than the main performance.
    let mut others: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    for &track_id in &release.track_ids {
        for (artist, role) in catalog.credits_on(EntityKind::Track, track_id) {
            if role != "main" {
                others
                    .entry(role.to_string())
                    .or_default()
                    .insert(artist.name.clone());
            }
        }
    }
    if !others.is_empty() {
        println!("{}", ui::section("Credits"));
        let mut t = Table::new(&["Role", "Artists"]).limit(1, 60);
        for (role, names) in others {
            t.push(vec![
                role_label(&role),
                names.into_iter().collect::<Vec<_>>().join(", "),
            ]);
        }
        print!("{}", t.render());
    }
    Ok(())
}
