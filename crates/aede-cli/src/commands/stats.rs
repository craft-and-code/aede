//! The `stats` command: library-wide figures.

use aede_core::json::Json;
use aede_core::model::Catalog;
use aede_core::stats;
use aede_core::text;

use super::{Res, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show_stats(args: &Args) -> Res {
    let catalog = load(args)?;
    let s = stats::compute(&catalog);

    if args.has("json") {
        println!("{}", stats_to_json(&catalog, &s).to_string_pretty());
        return Ok(());
    }

    println!("{}", ui::section("Library"));
    let mut t = Table::plain(2).align(1, Align::Right);
    t.push(vec!["Tracks".into(), s.tracks.to_string()]);
    t.push(vec!["Albums".into(), s.releases.to_string()]);
    t.push(vec![
        "  of which compilations".into(),
        s.compilations.to_string(),
    ]);
    t.push(vec!["Artists".into(), s.artists.to_string()]);
    t.push(vec![
        "  of which album artists".into(),
        s.album_artists.to_string(),
    ]);
    t.push(vec!["Labels".into(), s.labels.to_string()]);
    t.push(vec!["Genres".into(), s.genres.to_string()]);
    t.push(vec![
        "Total duration".into(),
        ui::long_duration(s.total_duration_ms),
    ]);
    t.push(vec![
        "Size on disk".into(),
        text::format_size(s.total_bytes),
    ]);
    if s.orphan_tracks > 0 {
        t.push(vec![
            "Tracks outside an album".into(),
            s.orphan_tracks.to_string(),
        ]);
    }
    print!("{}", t.render());

    print_buckets("Formats", &s.by_codec, true);
    print_buckets("Quality", &s.by_quality, true);
    print_buckets("Sample rates", &s.by_sample_rate, false);
    print_buckets("Decades (albums)", &s.by_decade, false);

    println!("{}", ui::section("Metadata completeness"));
    let mut t = Table::plain(3).align(1, Align::Right);
    for (label, ratio) in [
        ("Covers", s.cover_ratio),
        ("Years", s.year_ratio),
        ("Genres", s.genre_ratio),
        ("MusicBrainz IDs", s.mbid_ratio),
    ] {
        t.push(vec![
            label.into(),
            ui::percent(ratio),
            ui::bar((ratio * 100.0) as usize, 100, 24),
        ]);
    }
    print!("{}", t.render());

    // The credit vocabulary as it actually exists in *this* library. Without
    // it, `--role composer` returning nothing is indistinguishable from a bug:
    // the user knows their library holds composers, and has no way to see that
    // their files never carried a `composer` tag. A count of zero is an
    // answer; an empty screen is not.
    print_roles(&catalog);

    let limit = args.usize_value("limit", 10);
    println!(
        "{}",
        ui::section(&format!("Most present performers (top {limit})"))
    );
    let mut t = Table::new(&["Artist", "Tracks", ""])
        .align(1, Align::Right)
        .limit(0, 40);
    let top = stats::top_artists(&catalog, limit);
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    for (id, count) in &top {
        let name = catalog
            .artist(*id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        t.push(vec![name, count.to_string(), ui::bar(*count, max, 20)]);
    }
    print!("{}", t.render());

    let writers = stats::top_writers(&catalog, limit);
    if !writers.is_empty() {
        println!(
            "{}",
            ui::section(&format!(
                "Most credited writers and producers (top {limit})"
            ))
        );
        let mut t = Table::new(&["Name", "Tracks", ""])
            .align(1, Align::Right)
            .limit(0, 40);
        let max = writers.first().map(|(_, n)| *n).unwrap_or(0);
        for (id, count) in &writers {
            let name = catalog
                .artist(*id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            t.push(vec![name, count.to_string(), ui::bar(*count, max, 20)]);
        }
        print!("{}", t.render());
    }

    let genres = stats::top_genres(&catalog, limit);
    if !genres.is_empty() {
        println!(
            "{}",
            ui::section(&format!("Most frequent genres (top {limit})"))
        );
        let mut t = Table::new(&["Genre", "Tracks", ""])
            .align(1, Align::Right)
            .limit(0, 40);
        let max = genres.first().map(|(_, n)| *n).unwrap_or(0);
        for (id, count) in &genres {
            let name = catalog
                .genre(*id)
                .map(|g| g.name.clone())
                .unwrap_or_default();
            t.push(vec![name, count.to_string(), ui::bar(*count, max, 20)]);
        }
        print!("{}", t.render());
    }

    Ok(())
}

fn print_buckets(title: &str, buckets: &[stats::Bucket], with_size: bool) {
    if buckets.is_empty() {
        return;
    }
    println!("{}", ui::section(title));
    let headers: Vec<&str> = if with_size {
        vec!["", "Count", "Size", ""]
    } else {
        vec!["", "Count", ""]
    };
    let mut t = Table::new(&headers).align(1, Align::Right);
    if with_size {
        t = t.align(2, Align::Right);
    }
    let max = buckets.iter().map(|b| b.count).max().unwrap_or(0);
    for bucket in buckets {
        let mut row = vec![bucket.label.clone(), bucket.count.to_string()];
        if with_size {
            row.push(text::format_size(bucket.bytes));
        }
        row.push(ui::bar(bucket.count, max, 20));
        t.push(row);
    }
    print!("{}", t.render());
}

/// Who is credited how, over the whole catalog.
///
/// `main` and `album` are left out: they are on every track and every release
/// by construction, and would say nothing about the library. What is left is
/// exactly what `aede artists --role <role>` can be asked for.
fn print_roles(catalog: &Catalog) {
    let mut rows: Vec<(&str, usize, usize)> = Vec::new();
    for role in catalog.roles_in_use() {
        if role == "main" || role == "album" {
            continue;
        }
        let artists = catalog.artists_in_role(role);
        let credits: usize = artists.iter().map(|(_, n)| n).sum();
        rows.push((role, artists.len(), credits));
    }
    if rows.is_empty() {
        println!("{}", ui::section("Roles"));
        println!(
            "  {}",
            ui::dim("no credit beyond the main artists: the files carry no")
        );
        println!(
            "  {}",
            ui::dim("composer, producer or performer tag to build them from")
        );
        return;
    }
    rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(b.0)));

    println!("{}", ui::section("Roles"));
    let mut t = Table::new(&["Role", "Artists", "Credits"])
        .align(1, Align::Right)
        .align(2, Align::Right);
    for (role, artists, credits) in rows {
        t.push(vec![
            super::role_label(role),
            artists.to_string(),
            credits.to_string(),
        ]);
    }
    print!("{}", t.render());
    println!(
        "  {}",
        ui::dim("aede artists --role <role> lists the people behind one of them")
    );
}

fn stats_to_json(catalog: &Catalog, s: &stats::Stats) -> Json {
    let mut root = Json::obj();
    root.set("tracks", s.tracks.into());
    root.set("albums", s.releases.into());
    root.set("compilations", s.compilations.into());
    root.set("artists", s.artists.into());
    root.set("album_artists", s.album_artists.into());
    root.set("labels", s.labels.into());
    root.set("genres", s.genres.into());
    root.set("duration_ms", s.total_duration_ms.into());
    root.set("bytes", s.total_bytes.into());
    root.set("tracks_without_album", s.orphan_tracks.into());

    let buckets = |list: &[stats::Bucket]| {
        Json::Arr(
            list.iter()
                .map(|b| {
                    let mut o = Json::obj();
                    o.set("label", b.label.clone().into());
                    o.set("count", b.count.into());
                    o.set("bytes", b.bytes.into());
                    o
                })
                .collect(),
        )
    };
    // The credit vocabulary, so a client can build a role picker from what the
    // library actually holds rather than from a list frozen in its own code.
    root.set(
        "roles",
        Json::Arr(
            catalog
                .roles_in_use()
                .into_iter()
                .filter(|r| *r != "main" && *r != "album")
                .map(|role| {
                    let artists = catalog.artists_in_role(role);
                    let mut o = Json::obj();
                    o.set("role", role.to_string().into());
                    o.set("artists", artists.len().into());
                    o.set(
                        "credits",
                        artists.iter().map(|(_, n)| *n).sum::<usize>().into(),
                    );
                    o
                })
                .collect(),
        ),
    );

    root.set("by_codec", buckets(&s.by_codec));
    root.set("by_quality", buckets(&s.by_quality));
    root.set("by_sample_rate", buckets(&s.by_sample_rate));
    root.set("by_decade", buckets(&s.by_decade));

    let mut completeness = Json::obj();
    completeness.set("covers", s.cover_ratio.into());
    completeness.set("years", s.year_ratio.into());
    completeness.set("genres", s.genre_ratio.into());
    completeness.set("mbid", s.mbid_ratio.into());
    root.set("completeness", completeness);

    let mut top = Json::obj();
    top.set(
        "artists",
        Json::Arr(
            stats::top_artists(catalog, 20)
                .into_iter()
                .map(|(id, count)| {
                    let mut o = Json::obj();
                    o.set(
                        "name",
                        catalog
                            .artist(id)
                            .map(|a| a.name.clone())
                            .unwrap_or_default()
                            .into(),
                    );
                    o.set("tracks", count.into());
                    o
                })
                .collect(),
        ),
    );
    root.set("top", top);
    root
}
