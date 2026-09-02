//! Tests for [`super`], split out of `analysis.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;

/// A report holding one file, with the fields a real one carries.
fn example(extra: &str) -> String {
    format!(
        r#"{{
          "format": "flaccompagnon-report",
          "version": 1,
          "report": {{
            "root": "/music/Danzig",
            "files": [{{
              "path": "/music/Danzig/01 7th House.flac",
              "file_name": "01 7th House.flac",
              "size_bytes": 33551356,
              "modified_unix": 1782122103,
              "ext_mismatch": false,
              "detections": {{
                "upscaling": false,
                "upsampling": false,
                "transcoding": "none",
                "summary": "Clean",
                "detail": "Clean — full-band content to ~22.1 kHz."
              }},
              "cutoff_hz": 22050.0,
              "cutoff_ratio": 1.0,
              "real_bit_depth": 16,
              "requant_rate": 0.12820514,
              "fake_stereo": false,
              "clipping": {{
                "clipped_samples": 0,
                "clip_events": 0,
                "peak_dbfs": -0.13382691,
                "true_peak_dbtp": 0.28483155,
                "clipped": false
              }},
              "dr_db": 9.345364,
              "flac_md5": {{ "state": "Match" }},
              "error": null
              {extra}
            }}]
          }}
        }}"#
    )
}

fn write(name: &str, text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, text).expect("writing the report");
    path
}

#[test]
fn reads_a_report() {
    let path = write("aede_report_ok.json", &example(""));
    let report = read_report(&path).expect("a readable report");
    std::fs::remove_file(&path).ok();

    assert_eq!(report.root, "/music/Danzig");
    assert_eq!(report.version, 1);
    assert_eq!(report.files.len(), 1);

    let a = &report.files[0];
    assert_eq!(a.path, "/music/Danzig/01 7th House.flac");
    assert_eq!(a.file_name(), "01 7th House.flac");
    assert_eq!(a.source, "flaccompagnon");
    assert_eq!(a.size_bytes, 33_551_356);
    assert_eq!(a.md5_state.as_deref(), Some("Match"));
    assert_eq!(a.real_bit_depth, Some(16));
    assert_eq!(a.cutoff_hz, Some(22_050.0));
    assert_eq!(a.transcoding.as_deref(), Some("none"));
    assert_eq!(a.dr_db, Some(9.345_364));
    // Zero measured is not the same as nothing measured, and a peak below
    // zero has to survive the sign.
    assert_eq!(a.clipped_samples, Some(0));
    assert_eq!(a.clipped, Some(false));
    assert!(a.peak_dbfs.unwrap() < 0.0);
    assert!(!a.md5_failed());
    assert!(!a.suspect_encoding());
    // Nothing was measured about this, and the record says so rather than
    // guessing a default.
    assert_eq!(a.error, None);
}

#[test]
fn a_measurement_the_reader_does_not_know_is_not_an_error() {
    // The other tool will grow fields; a report carrying one still imports.
    let path = write(
        "aede_report_new_field.json",
        &example(", \"loudness_lufs\": -9.4"),
    );
    let report = read_report(&path).expect("unknown fields are ignored");
    std::fs::remove_file(&path).ok();
    assert_eq!(report.files.len(), 1);
}

#[test]
fn another_tools_json_is_refused_by_name() {
    let path = write(
        "aede_report_foreign.json",
        r#"{"format": "something-else"}"#,
    );
    let error = read_report(&path).expect_err("not a report");
    std::fs::remove_file(&path).ok();
    match error {
        ImportError::NotAReport { found } => assert_eq!(found, "something-else"),
        other => panic!("wrong error: {other:?}"),
    }
}

#[test]
fn a_report_is_recognised_without_being_parsed() {
    // A scan walks past every file in the library; it must be able to tell
    // a report from any other .json without reading the whole of it.
    let report = write("aede_sniff_report.json", &example(""));
    let other = write("aede_sniff_other.json", r#"{"hello": "world"}"#);
    let padded = write(
        "aede_sniff_padded.json",
        &format!("{}{}", " ".repeat(SNIFF_BYTES * 2), example("")),
    );
    assert!(looks_like_a_report(&report));
    assert!(!looks_like_a_report(&other), "someone else's JSON");
    assert!(
        !looks_like_a_report(&padded),
        "the marker has to be near the head, or the sniff is not cheap"
    );
    assert!(
        !looks_like_a_report(Path::new("/nowhere/at/all.json")),
        "an unreadable file is not a report"
    );
    for path in [report, other, padded] {
        std::fs::remove_file(&path).ok();
    }
}

#[test]
fn a_report_from_a_later_tool_is_refused_rather_than_half_read() {
    let text = example("").replace("\"version\": 1", "\"version\": 99");
    let path = write("aede_report_future.json", &text);
    let error = read_report(&path).expect_err("too new");
    std::fs::remove_file(&path).ok();
    assert!(matches!(error, ImportError::Version { found: 99 }));
}

/// A catalog holding one file, built by hand: matching needs a path, a
/// size and a date, and nothing else.
fn catalog_holding(path: &str, size: u64, mtime: u64) -> Catalog {
    let mut catalog = Catalog::default();
    catalog.files.push(crate::model::AudioFile {
        id: 0,
        path: path.to_string(),
        size,
        mtime,
        ..Default::default()
    });
    catalog
}

fn record_for(path: &str, size: u64, mtime: u64) -> FileAnalysis {
    FileAnalysis {
        path: path.to_string(),
        source: "flaccompagnon".into(),
        source_version: 1,
        size_bytes: size,
        modified_unix: mtime,
        md5_state: Some("Match".into()),
        ..Default::default()
    }
}

#[test]
fn a_record_naming_the_file_by_another_route_still_finds_it() {
    // Watched folders are stored canonical, so a report written against a
    // symbolic link — or against /var where macOS says /private/var — names
    // the same file by a string that will never compare equal.
    let mut catalog = catalog_holding("/private/var/music/01 So What.flac", 500, 10);
    let record = record_for("/var/music/01 So What.flac", 500, 10);

    let outcome = merge_into(&mut catalog, vec![record], 99);
    assert_eq!(outcome.matched, 0, "the path itself does not match");
    assert_eq!(outcome.moved, 1, "but the name and the size do");
    assert_eq!(outcome.attached(), 1);
    assert_eq!(
        catalog.analyses[0].path, "/private/var/music/01 So What.flac",
        "and it is refiled under where the file is, so it attaches directly next time"
    );
    assert_eq!(catalog.analyses[0].imported_at, 99);
    assert_eq!(catalog.pending_analyses(), 0);
}

#[test]
fn matching_a_file_is_not_the_same_as_describing_it() {
    // The trap: name and size agree, so the record is *about* this file —
    // but the date says it was written to since, so it no longer describes
    // it. Matching by name and size must not skip that test.
    let mut catalog = catalog_holding("/music/01 So What.flac", 500, 20);
    let record = record_for("/elsewhere/01 So What.flac", 500, 10);

    let outcome = merge_into(&mut catalog, vec![record], 0);
    assert_eq!(outcome.moved, 0);
    assert_eq!(outcome.stale, 1);
    assert!(catalog.analyses.is_empty(), "nothing is stored");
}

#[test]
fn a_record_waits_when_nothing_matches_it_yet() {
    let mut catalog = Catalog::default();
    let outcome = merge_into(&mut catalog, vec![record_for("/music/a.flac", 500, 10)], 0);
    assert_eq!(outcome.waiting, 1);
    assert_eq!(
        outcome.waiting_folders,
        BTreeMap::from([("/music".to_string(), 1)]),
        "reported by the folder to scan, not by the file name in it"
    );
    assert_eq!(catalog.analyses.len(), 1, "kept, not thrown away");

    // The scan brings the file in — under another name for the same path.
    catalog.files.push(crate::model::AudioFile {
        id: 0,
        path: "/library/a.flac".into(),
        size: 500,
        mtime: 10,
        ..Default::default()
    });
    assert_eq!(reconcile(&mut catalog), 1);
    assert_eq!(catalog.analyses[0].path, "/library/a.flac");
    assert_eq!(reconcile(&mut catalog), 0, "and it is not done twice");
}

#[test]
fn an_album_of_waiting_records_is_reported_as_one_folder() {
    // A report of a whole album is one decision — scan that folder, or
    // decide it is gone — and naming every track spends fourteen rows to
    // say it once. The count is what carries the volume.
    let mut catalog = Catalog::default();
    let records: Vec<FileAnalysis> = (1..=14)
        .map(|n| {
            record_for(
                &format!("/music/Blizzard of Ozz/{n:02} track.flac"),
                500,
                10,
            )
        })
        .collect();
    let outcome = merge_into(&mut catalog, records, 0);
    assert_eq!(outcome.waiting, 14);
    assert_eq!(
        outcome.waiting_folders,
        BTreeMap::from([("/music/Blizzard of Ozz".to_string(), 14)]),
        "one row, and it says how many"
    );
}

#[test]
fn the_cap_bounds_the_rows_and_not_the_counts() {
    // More folders than can be shown: the ones listed must still carry
    // their true totals, or the report understates what is waiting in the
    // very folders it does name.
    let mut catalog = Catalog::default();
    let mut records = Vec::new();
    for folder in 0..WAITING_SHOWN + 5 {
        for track in 0..3 {
            records.push(record_for(
                &format!("/music/album{folder:02}/{track}.flac"),
                500,
                10,
            ));
        }
    }
    let outcome = merge_into(&mut catalog, records, 0);
    assert_eq!(outcome.waiting, (WAITING_SHOWN + 5) * 3);
    assert_eq!(outcome.waiting_folders.len(), WAITING_SHOWN);
    assert!(
        outcome.waiting_folders.values().all(|&n| n == 3),
        "a folder that is shown is shown whole: {:?}",
        outcome.waiting_folders
    );
}

#[test]
fn a_record_filed_under_the_real_path_beats_one_that_is_guessed_at() {
    // Two records of the same source could otherwise land on one file: the
    // one that was attached deliberately must survive.
    let mut catalog = catalog_holding("/music/a.flac", 500, 10);
    catalog.analyses.push(FileAnalysis {
        detail: Some("attached".into()),
        ..record_for("/music/a.flac", 500, 10)
    });
    catalog.analyses.push(FileAnalysis {
        detail: Some("waiting".into()),
        ..record_for("/elsewhere/a.flac", 500, 10)
    });

    assert_eq!(reconcile(&mut catalog), 0);
    assert_eq!(catalog.analyses.len(), 2, "the waiting one is left waiting");
    let attached: Vec<&str> = catalog
        .analyses
        .iter()
        .filter(|a| a.path == "/music/a.flac")
        .filter_map(|a| a.detail.as_deref())
        .collect();
    assert_eq!(attached, vec!["attached"]);
}

#[test]
fn an_analysis_expires_with_the_bytes_it_describes() {
    let a = FileAnalysis {
        size_bytes: 100,
        modified_unix: 10,
        ..FileAnalysis::default()
    };
    assert!(a.still_applies(100, 10));
    assert!(!a.still_applies(101, 10), "re-encoded");
    assert!(!a.still_applies(100, 11), "re-tagged");
}

#[test]
fn a_lossy_ancestry_is_suspect_however_it_was_found() {
    let detected = FileAnalysis {
        transcoding: Some("detected".into()),
        ..FileAnalysis::default()
    };
    let suspected = FileAnalysis {
        transcoding: Some("suspected".into()),
        ..FileAnalysis::default()
    };
    let upscaled = FileAnalysis {
        upscaling: Some(true),
        ..FileAnalysis::default()
    };
    let clean = FileAnalysis {
        transcoding: Some("none".into()),
        upscaling: Some(false),
        upsampling: Some(false),
        ..FileAnalysis::default()
    };
    assert!(detected.suspect_encoding());
    assert!(suspected.suspect_encoding());
    assert!(upscaled.suspect_encoding());
    assert!(!clean.suspect_encoding());
    // Nothing measured is not a suspicion.
    assert!(!FileAnalysis::default().suspect_encoding());
}
