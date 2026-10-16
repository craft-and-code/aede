//! Synthetic diagnostic scaling probe; no filesystem or network access.
//! Run `cargo run --offline -p aede-core --release --example doctor_bench`.

use std::time::Instant;

use aede_core::{
    doctor,
    model::{self, ScannedFile},
    sources::Sources,
    tags::RawTags,
};

fn main() {
    for count in [1_000, 2_000, 4_000] {
        let files = (0..count)
            .map(|index| {
                let mut tags = RawTags::default();
                for (key, value) in [
                    ("artist", "Benchmark Band".to_string()),
                    ("album", format!("Record {index}")),
                    ("title", format!("Song {index}")),
                    ("tracknumber", "1".to_string()),
                    ("date", "2000".to_string()),
                ] {
                    tags.insert(key, value);
                }
                tags.properties.duration_ms = Some(60_000);
                tags.properties.codec = "flac".to_string();
                tags.properties.sample_rate = Some(44_100);
                ScannedFile {
                    path: format!("/fictional/Record {index}/01.flac"),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: Some(format!("/fictional/Record {index}/cover.jpg")),
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect();
        let catalog = model::build(files, vec!["/fictional".to_string()], 1, &[]);
        let mut times = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let issues = doctor::diagnose(&catalog, &Sources::default());
            times.push(started.elapsed().as_micros());
            assert!(issues.is_empty());
            std::hint::black_box(issues);
        }
        times.sort_unstable();
        println!(
            "releases={count} edges={} median_us={} min_us={} max_us={} issues=0",
            catalog.relations.len(),
            times[2],
            times[0],
            times[4],
        );
    }
}
