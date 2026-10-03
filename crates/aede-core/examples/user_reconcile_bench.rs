//! Measures reconciliation with one attached note per synthetic track.
//!
//! No audio or store is read or written. Evidence is primed before timing, and
//! each result is the median of three identical passes. There is no timing
//! assertion. To reproduce the audit's debug-library/optimized-driver setup:
//! `cargo rustc -p aede-core --example user_reconcile_bench --offline -- -C opt-level=3`
//! then run `target/debug/examples/user_reconcile_bench`.

use aede_core::model::{AudioFile, Catalog, EntityKind, Track};
use aede_core::user::{self, Annotation, EntityRef, LOCAL_USER, UserData};
use std::time::Instant;

fn main() {
    for count in [2_000usize, 8_000] {
        let catalog = Catalog {
            files: (0..count)
                .map(|id| AudioFile {
                    id: id as u32,
                    path: format!("/music/{id}.flac"),
                    size: 100,
                    ..Default::default()
                })
                .collect(),
            tracks: (0..count)
                .map(|id| Track {
                    id: id as u32,
                    file_id: id as u32,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut data = UserData {
            annotations: catalog
                .files
                .iter()
                .map(|file| Annotation {
                    owner: LOCAL_USER.into(),
                    target: EntityRef::new(EntityKind::Track, &file.path),
                    note: Some("kept".into()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        assert_eq!(user::reconcile(&mut data, &catalog).attached, count);
        let mut times = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            assert_eq!(user::reconcile(&mut data, &catalog).attached, count);
            times.push(start.elapsed().as_secs_f64() * 1_000.0);
        }
        times.sort_by(f64::total_cmp);
        println!("{count} references: {:.3} ms median", times[1]);
    }
}
