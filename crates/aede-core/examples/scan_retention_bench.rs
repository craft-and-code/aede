//! Synthetic inaccessible-folder benchmark; never creates or reads music files.
//! Run `cargo run -p aede-core --example scan_retention_bench -- 4000`.

use aede_core::{
    model::{self, ScannedFile},
    scan::{self, ScanOptions},
    tags::RawTags,
};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .ok_or("usage: scan_retention_bench <folders>")?
        .parse::<usize>()?;
    if count == 0 {
        return Err("folder count must be positive".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "aede_absent_scan_bench_{}_{nonce}",
        std::process::id()
    ));
    let roots: Vec<PathBuf> = (0..count)
        .map(|index| base.join(format!("absent-{index:05}")))
        .collect();
    let files = roots
        .iter()
        .map(|root| ScannedFile {
            path: root.join("01.flac").to_string_lossy().into_owned(),
            size: 1,
            mtime: 1,
            tags: RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        })
        .collect();
    let catalog = model::build(
        files,
        roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        0,
        &[],
    );
    for run in 0..3 {
        let start = Instant::now();
        let (fresh, report) = scan::scan(&roots, Some(&catalog), &ScanOptions::default(), |_| {})?;
        if fresh.files.len() != count || report.preserved != count {
            return Err("scan lost an inaccessible entry".into());
        }
        println!(
            "folders={count},run={run},elapsed_ms={}",
            start.elapsed().as_millis()
        );
    }
    Ok(())
}
