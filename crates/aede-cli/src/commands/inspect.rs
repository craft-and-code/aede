//! Reading a single file, and exporting the catalog.

use std::path::Path;

use aede_core::store;
use aede_core::tags;
use aede_core::text;

use super::{Res, load};
use crate::args::Args;
use crate::ui::{self, Table};

/// Inspects a single file, without going through the catalog. Handy to
/// understand why a file ends up misfiled.
pub fn inspect(args: &Args) -> Res {
    let Some(raw) = args.positionals.first() else {
        return Err("give a file: aede file track.flac".into());
    };
    let path = Path::new(raw);
    let tags = tags::read(path)?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    println!("{}", ui::section(raw));
    let p = &tags.properties;
    let mut t = Table::plain(2);
    t.push(vec!["Container".into(), p.container.clone()]);
    t.push(vec!["Codec".into(), p.codec.clone()]);
    t.push(vec!["Quality".into(), p.quality_label()]);
    t.push(vec![
        "Sample rate".into(),
        p.sample_rate
            .map(|r| format!("{r} Hz"))
            .unwrap_or_else(|| "—".into()),
    ]);
    t.push(vec![
        "Bit depth".into(),
        p.bit_depth
            .map(|b| format!("{b} bits"))
            .unwrap_or_else(|| "—".into()),
    ]);
    t.push(vec![
        "Channels".into(),
        p.channels
            .map(|c| c.to_string())
            .unwrap_or_else(|| "—".into()),
    ]);
    t.push(vec![
        "Duration".into(),
        p.duration_ms
            .map(text::format_duration)
            .unwrap_or_else(|| "—".into()),
    ]);
    t.push(vec![
        "Bitrate".into(),
        p.bitrate_kbps
            .map(|b| format!("{b} kbps"))
            .unwrap_or_else(|| "—".into()),
    ]);
    t.push(vec![
        "Lossless".into(),
        if p.lossless { "yes" } else { "no" }.into(),
    ]);
    t.push(vec![
        "Embedded cover art".into(),
        if tags.has_embedded_art { "yes" } else { "no" }.into(),
    ]);
    t.push(vec!["Size".into(), text::format_size(size)]);
    print!("{}", t.render());

    println!("{}", ui::section("Tags"));
    if tags.is_empty() {
        println!("  {}", ui::yellow("no tag in this file"));
    } else {
        let mut t = Table::new(&["Field", "Value"]).limit(1, 70);
        for (key, values) in &tags.fields {
            t.push(vec![key.clone(), values.join(" / ")]);
        }
        print!("{}", t.render());
    }
    Ok(())
}

pub fn export(args: &Args) -> Res {
    let catalog = load(args)?;
    let json = store::to_json(&catalog).to_string_pretty();
    match args.value("output") {
        Some(path) => {
            std::fs::write(path, &json)?;
            println!(
                "{} {}",
                ui::green("→"),
                format_args!("catalog exported to {path}")
            );
        }
        None => println!("{json}"),
    }
    Ok(())
}
