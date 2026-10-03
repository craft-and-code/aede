//! Reproducible hot-cache companion planning benchmark for the copy audit.
//!
//! Run with `cargo run -p aede-core --release --example audit_copy_bench`.
//! The disposable tree contains 1,200 albums and 19,200 companion files;
//! setup and catalog construction are outside the measured interval.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use aede_core::copy::{Extras, Recipe};
use aede_core::model::{self, ScannedFile};
use aede_core::tags::RawTags;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("aede_copy_bench_{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let result = measure(&root);
    std::fs::remove_dir_all(&root)?;
    result
}

fn measure(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut scanned = Vec::new();
    for album in 0..1200 {
        let folder = root.join(format!("Album {album:04}"));
        let nested = folder.join("spectrograms");
        std::fs::create_dir_all(&nested)?;
        let path = folder.join("01.flac");
        std::fs::write(&path, b"fixture")?;
        for number in 0..8 {
            std::fs::write(folder.join(format!("{number:02}.jpg")), b"image")?;
            std::fs::write(nested.join(format!("{number:02}.png")), b"image")?;
        }
        let mut tags = RawTags::default();
        tags.insert("artist", "Benchmark");
        tags.insert("album", format!("Album {album:04}"));
        tags.insert("title", "Track");
        scanned.push(ScannedFile {
            path: path.to_string_lossy().into_owned(),
            size: 7,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let catalog = model::build(scanned, vec![root.to_string_lossy().into_owned()], 0, &[]);
    let tracks: Vec<_> = catalog.tracks.iter().map(|track| track.id).collect();
    let recipe = Recipe {
        extras: Extras::All,
        ..Default::default()
    };
    let mut samples = Vec::new();
    let mut expected_hash = None;
    for iteration in 0..12 {
        let start = Instant::now();
        let plan = aede_core::copy::plan(&catalog, &tracks, &recipe);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        assert!(plan.rejected.is_empty());
        assert_eq!(plan.items.len(), 20400);
        let mut hash = DefaultHasher::new();
        for item in &plan.items {
            item.relative.hash(&mut hash);
            item.size.hash(&mut hash);
            (item.kind as u8).hash(&mut hash);
        }
        let hash = hash.finish();
        assert_eq!(*expected_hash.get_or_insert(hash), hash);
        if iteration >= 3 {
            samples.push(elapsed);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "albums=1200 companions=19200 outputs=20400 hash={:016x} median_ms={:.3} min_ms={:.3} max_ms={:.3}",
        expected_hash.unwrap_or(0),
        samples[4],
        samples[0],
        samples[8]
    );
    Ok(())
}
