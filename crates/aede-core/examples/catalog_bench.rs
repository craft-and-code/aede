//! Reproducible synthetic-catalog benchmark for storage decisions.
//!
//! No music file is read or written. Generate and load run in separate
//! processes so the measured load peak does not retain the builder's graph.

use std::error::Error;
use std::io;
use std::path::Path;
use std::time::Instant;

use aede_core::model::{ScannedFile, build};
use aede_core::store;
use aede_core::tags::RawTags;

fn synthetic_file(index: usize) -> ScannedFile {
    let album_index = index / 12;
    let artist_index = album_index / 10;
    let track_number = index % 12 + 1;
    let artist = format!("Artist {artist_index:05}");
    let album = format!("Album {album_index:06}");
    let title = format!("Track {track_number:02} of {album}");
    let mut tags = RawTags::default();
    for (key, value) in [
        ("title", title),
        ("artist", artist.clone()),
        ("albumartist", artist.clone()),
        ("album", album.clone()),
        ("tracknumber", track_number.to_string()),
        ("discnumber", "1".into()),
        ("date", (1970 + album_index % 55).to_string()),
        ("genre", format!("Genre {}", artist_index % 12)),
        ("label", format!("Label {}", artist_index % 20)),
        (
            "musicbrainz_artistid",
            format!("artist-id-{artist_index:05}"),
        ),
        (
            "musicbrainz_albumartistid",
            format!("artist-id-{artist_index:05}"),
        ),
        ("musicbrainz_albumid", format!("album-id-{album_index:06}")),
        (
            "musicbrainz_recordingid",
            format!("recording-id-{index:07}"),
        ),
    ] {
        tags.insert(key, value);
    }
    tags.properties.codec = "flac".into();
    tags.properties.container = "flac".into();
    tags.properties.sample_rate = Some(44_100);
    tags.properties.bit_depth = Some(16);
    tags.properties.channels = Some(2);
    tags.properties.duration_ms = Some(240_000);
    tags.properties.lossless = true;

    ScannedFile {
        path: format!("/aede-benchmark/{artist}/{album}/{track_number:02} {index:07}.flac"),
        size: 24_000_000,
        mtime: 1_700_000_000,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

fn generate(count: usize, path: &Path) -> Result<(), Box<dyn Error>> {
    if count == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "count must be positive").into());
    }
    if path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "benchmark output already exists",
        )
        .into());
    }
    let start = Instant::now();
    let scanned: Vec<_> = (0..count).map(synthetic_file).collect();
    let catalog = build(scanned, vec!["/aede-benchmark".into()], 1_700_000_000, &[]);
    if catalog.tracks.len() != count || catalog.files.len() != count {
        return Err(io::Error::other("synthetic catalog lost tracks").into());
    }
    let build_ms = start.elapsed().as_millis();
    let start = Instant::now();
    store::save_catalog_only(&catalog, path)?;
    let save_ms = start.elapsed().as_millis();
    println!("tracks={count}");
    println!("artists={}", catalog.artists.len());
    println!("releases={}", catalog.releases.len());
    println!("build_ms={build_ms}");
    println!("save_ms={save_ms}");
    println!("catalog_bytes={}", std::fs::metadata(path)?.len());
    Ok(())
}

fn load(path: &Path) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let catalog = store::load(path)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "benchmark catalog is missing"))?;
    let load_ms = start.elapsed().as_millis();
    println!("tracks={}", catalog.tracks.len());
    println!("load_ms={load_ms}");
    std::hint::black_box(catalog);
    Ok(())
}

fn queries(count: usize) -> Result<(), Box<dyn Error>> {
    use aede_core::model::EntityKind;
    use aede_core::query::{self, Context, Sort};
    use aede_core::user::{Annotation, EntityRef, PlayCount, UserData};

    let catalog = build((0..count).map(synthetic_file).collect(), vec![], 0, &[]);
    let mut data = UserData::default();
    for track in &catalog.tracks {
        let target = EntityRef::of(&catalog, EntityKind::Track, track.id).unwrap();
        data.annotations.push(Annotation {
            owner: "local".into(),
            target: target.clone(),
            rating: Some(4),
            ..Default::default()
        });
        data.counts.push(PlayCount {
            owner: "local".into(),
            track: target,
            count: 3,
            last_played: 1_700_000_000,
        });
    }
    for expression in ["artist:Artist", "genre:Genre", "rating:>=4 played:>=2"] {
        let parsed = query::parse(expression)?;
        let start = Instant::now();
        let context = Context::new(&catalog, &data, "local");
        let mut selected = query::run(&parsed, &context);
        query::sort(&mut selected, Sort::parse("artist")?, &context);
        println!(
            "query={expression:?},tracks={},elapsed_us={}",
            selected.len(),
            start.elapsed().as_micros()
        );
        std::hint::black_box(selected);
    }
    Ok(())
}

fn parse_queries(count: usize) -> Result<(), Box<dyn Error>> {
    let expression = "-title:x ".repeat(count);
    for _ in 0..3 {
        let start = Instant::now();
        let parsed = aede_core::query::parse(&expression)?;
        println!("terms={count},parse_us={}", start.elapsed().as_micros());
        std::hint::black_box(parsed);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.as_slice() {
        [_, action, raw_count, path] if action == "generate" => {
            generate(raw_count.parse()?, Path::new(path))
        }
        [_, action, path] if action == "load" => load(Path::new(path)),
        [_, action, raw_count] if action == "query" => queries(raw_count.parse()?),
        [_, action, raw_count] if action == "parse" => parse_queries(raw_count.parse()?),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: catalog_bench generate <tracks> <catalog.json> | load <catalog.json> | query <tracks> | parse <terms>",
        )
        .into()),
    }
}

#[cfg(test)]
#[path = "catalog_bench/catalog_bench_tests.rs"]
mod tests;
