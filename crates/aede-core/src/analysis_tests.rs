//! Tests for [`super`], split out of `analysis.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;

#[test]
fn imported_bit_depth_never_wraps_into_a_plausible_measurement() {
    for invalid in ["65552", "4294967295", "-1", "16.5", "\"16\"", "true"] {
        let text = example("").replace(
            "\"real_bit_depth\": 16",
            &format!("\"real_bit_depth\": {invalid}"),
        );
        assert!(parse_report(&text).is_err(), "accepted depth {invalid}");
    }
    for (field, expected) in [("0", Some(0)), ("65535", Some(u16::MAX)), ("null", None)] {
        let text = example("").replace(
            "\"real_bit_depth\": 16",
            &format!("\"real_bit_depth\": {field}"),
        );
        assert_eq!(
            parse_report(&text).unwrap().files[0].real_bit_depth,
            expected
        );
    }
    let legacy = example("").replace("\"real_bit_depth\": 16,", "");
    assert_eq!(parse_report(&legacy).unwrap().files[0].real_bit_depth, None);
}

#[test]
fn report_rewrites_keep_per_entry_dates_and_legacy_reports_use_the_file_date() {
    let dir = TestDirectory::new("per_entry_dates");
    let date = 1_750_000_000_000_000_100_u64;
    let text = example(&format!(", \"aede_result_at_ns\": \"{date}\""));
    let path = dir.file("report.json", text.as_bytes());
    let metadata = std::fs::metadata(&path).unwrap();
    assert_ne!(crate::clock::mtime_nanoseconds(&metadata), date);
    assert_eq!(
        read_report(Path::new(&path)).unwrap().files[0].result_at_ns,
        date
    );
    assert_eq!(parse_report(&text).unwrap().files[0].result_at_ns, date);
    assert_eq!(
        parse_report_at(&example(""), 123).unwrap().files[0].result_at_ns,
        123
    );
    assert_eq!(parse_report(&example("")).unwrap().files[0].result_at_ns, 0);
    let legacy = dir.file("legacy.json", example("").as_bytes());
    let file_date = crate::clock::mtime_nanoseconds(&std::fs::metadata(&legacy).unwrap());
    assert_eq!(
        read_report(Path::new(&legacy)).unwrap().files[0].result_at_ns,
        file_date
    );
    for invalid in ["null", "123", "\"-1\"", "\"18446744073709551616\"", "\"\""] {
        assert!(parse_report(&example(&format!(", \"aede_result_at_ns\": {invalid}"))).is_err());
    }
    assert_eq!(
        parse_report_at(&example(", \"aede_result_at_ns\": \"0\""), 123)
            .unwrap()
            .files[0]
            .result_at_ns,
        0
    );
}

#[test]
fn an_older_report_cannot_replace_a_newer_result_after_a_storage_roundtrip() {
    let newer = FileAnalysis {
        path: "/music/one.flac".into(),
        source: "flaccompagnon".into(),
        result_at_ns: 1_750_000_000_000_000_100,
        summary: Some("newer".into()),
        ..Default::default()
    };
    let stored = crate::conclusions::Conclusions {
        analyses: vec![newer.clone()],
        ..Default::default()
    };
    let json = crate::conclusions::to_json(&stored).to_string_compact();
    let read = crate::conclusions::from_json(&crate::json::parse(&json).unwrap()).unwrap();
    let mut catalog = Catalog::default();
    read.attach(&mut catalog);
    let older = FileAnalysis {
        result_at_ns: newer.result_at_ns - 100,
        summary: Some("older".into()),
        ..newer.clone()
    };
    assert_eq!(merge_into(&mut catalog, vec![older], 20).older, 1);
    assert_eq!(catalog.analyses, vec![newer]);
}

#[test]
fn the_newest_report_result_wins_independently_of_import_order() {
    let older = FileAnalysis {
        path: "/music/Artist/Album/01.flac".into(),
        source: "flaccompagnon".into(),
        result_at_ns: 1_750_000_000_000_000_100,
        summary: Some("older group result".into()),
        ..Default::default()
    };
    let newer = FileAnalysis {
        result_at_ns: older.result_at_ns + 100,
        summary: Some("newer album result".into()),
        ..older.clone()
    };
    for records in [
        vec![newer.clone(), older.clone()],
        vec![older.clone(), newer.clone()],
    ] {
        let mut catalog = Catalog::default();
        merge_into(&mut catalog, records, 99);
        assert_eq!(
            catalog.analyses,
            vec![FileAnalysis {
                imported_at: 99,
                ..newer.clone()
            }]
        );
        let outcome = merge_into(&mut catalog, vec![older.clone()], 100);
        assert_eq!(outcome.older, 1);
        assert_eq!(catalog.analyses[0].imported_at, 99);
        assert_eq!(catalog.analyses[0].summary, newer.summary);
    }
    let legacy = crate::store::analysis_from_json(
        &crate::json::parse(
            r#"{"path":"/music/01.flac","source":"flaccompagnon","imported_at":10}"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        legacy.result_at_ns, 0,
        "older stored records remain readable"
    );
}

#[test]
fn a_valid_report_replaces_a_stale_measurement_regardless_of_report_dates() {
    let path = "/music/Artist/Album/01.flac";
    let current = FileAnalysis {
        path: path.into(),
        source: "flaccompagnon".into(),
        size_bytes: 10,
        modified_unix: 3,
        result_at_ns: 100,
        summary: Some("current bytes".into()),
        ..Default::default()
    };
    let mut catalog = Catalog {
        files: vec![crate::model::AudioFile {
            path: path.into(),
            size: 10,
            mtime: 3,
            ..Default::default()
        }],
        analyses: vec![FileAnalysis {
            modified_unix: 2,
            result_at_ns: 200,
            summary: Some("different bytes".into()),
            ..current.clone()
        }],
        ..Default::default()
    };
    let outcome = merge_into(&mut catalog, vec![current], 300);
    assert_eq!(outcome.older, 0);
    assert_eq!(
        catalog.analyses[0].summary.as_deref(),
        Some("current bytes")
    );
}

#[test]
fn a_waiting_current_result_replaces_stale_exact_path_evidence_after_a_scan() {
    for (index, waiting_name) in ["A pending.flac", "z pending.flac"].into_iter().enumerate() {
        let dir = TestDirectory::new(&format!("stale_exact_reconciliation_{index}"));
        let music = dir.path.join("Music");
        std::fs::create_dir(&music).unwrap();
        let path = music.join("track.flac");
        std::fs::write(&path, include_bytes!("../tests/fixtures/track.flac")).unwrap();
        let roots = [music];
        let (mut previous, _) =
            crate::scan::scan(&roots, None, &Default::default(), |_| {}).unwrap();
        let file = &previous.files[0];
        let old = FileAnalysis {
            result_at_ns: 1000,
            detail: Some("different earlier bytes".into()),
            ..record_for(&file.path, file.size, file.mtime)
        };
        assert_eq!(merge_into(&mut previous, vec![old.clone()], 10).matched, 1);

        let bytes = include_bytes!("../tests/fixtures/hires.flac");
        let before_move = dir.file(waiting_name, bytes);
        let mut source_data = Json::obj();
        source_data.set("path", before_move.clone().into());
        source_data.set("unknown_measurement", true.into());
        let current = FileAnalysis {
            file_md5: Some(file_md5(Path::new(&before_move)).unwrap()),
            result_at_ns: 200,
            detail: Some("current bytes".into()),
            source_data: Some(source_data.clone()),
            ..record_for(&before_move, bytes.len() as u64, 10)
        };
        assert_eq!(merge_into(&mut previous, vec![current], 20).waiting, 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&before_move, &path).unwrap();
        let (refreshed, _) =
            crate::scan::scan(&roots, Some(&previous), &Default::default(), |_| {}).unwrap();
        assert_eq!(refreshed.analyses.len(), 1, "{waiting_name}");
        let record = &refreshed.analyses[0];
        let file = &refreshed.files[0];
        assert_eq!(record.path, file.path);
        assert!(record.still_applies(file.size, file.mtime));
        assert_eq!(record.detail.as_deref(), Some("current bytes"));
        assert_eq!(record.result_at_ns, 200);
        assert_eq!(record.source_data, Some(source_data));
        assert_eq!(refreshed.pending_analyses(), 0);

        let mut no_replacement = Catalog {
            files: refreshed.files.clone(),
            analyses: vec![old.clone()],
            ..Default::default()
        };
        assert_eq!(reconcile(&mut no_replacement), 0);
        assert_eq!(
            no_replacement.analyses,
            vec![old],
            "stale evidence stays held when nothing replaces it"
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn waiting_reports_keep_the_newest_result_when_their_paths_converge() {
    let dir = TestDirectory::new("dated_waiting_reports");
    let bytes = include_bytes!("../tests/fixtures/track.flac");
    let path = dir.file("renamed.flac", bytes);
    let digest = file_md5(Path::new(&path)).unwrap();
    let size = bytes.len() as u64;
    for (first_date, last_date, expected_date) in [
        (200, 100, 200),
        (100, 200, 200),
        (200, 200, 200),
        (0, 0, 0),
        (200, 0, 0),
    ] {
        for reverse_import in [false, true] {
            let mut records = Vec::new();
            for (old_path, result_at_ns) in [
                ("/old-library/a/track.flac", first_date),
                ("/old-library/z/track.flac", last_date),
            ] {
                let text = example(&format!(", \"file_md5\": \"{digest}\""))
                    .replace("/music/Danzig/01 7th House.flac", old_path)
                    .replace(
                        "\"file_name\": \"01 7th House.flac\"",
                        "\"file_name\": \"track.flac\"",
                    )
                    .replace("33551356", &size.to_string())
                    .replace("1782122103", "10");
                let mut record = parse_report(&text).unwrap().files.remove(0);
                record.result_at_ns = result_at_ns;
                records.push(record);
            }
            let expected = if first_date != 0 && last_date != 0 && first_date > last_date {
                records[0].clone()
            } else {
                records[1].clone()
            };
            if reverse_import {
                records.reverse();
            }
            let mut catalog = Catalog::default();
            assert_eq!(merge_into(&mut catalog, records, 99).waiting, 2);
            catalog.files = catalog_holding(&path, size, 20).files;
            assert!(reconcile(&mut catalog) > 0);
            assert_eq!(catalog.analyses.len(), 1);
            let attached = &catalog.analyses[0];
            assert_eq!(attached.path, path);
            assert_eq!(attached.result_at_ns, expected_date);
            assert_eq!(attached.modified_unix, 20);
            assert_eq!(attached.source_data, expected.source_data);
            assert_eq!(attached.imported_at, 99);
            assert_eq!(catalog.pending_analyses(), 0);
        }
    }
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn malformed_report_envelopes_are_refused_without_rejecting_empty_legacy_reports() {
    for version in ["null", "\"1\"", "true", "-1", "1.5", "4294967296"] {
        let text = format!(
            r#"{{"format":"flaccompagnon-report","version":{version},"report":{{"files":[]}}}}"#
        );
        assert!(parse_report(&text).is_err(), "accepted version {version}");
    }
    for text in [
        r#"{"format":"flaccompagnon-report","version":1}"#,
        r#"{"format":"flaccompagnon-report","version":1,"report":null}"#,
        r#"{"format":"flaccompagnon-report","version":1,"report":[]}"#,
        r#"{"format":"flaccompagnon-report","version":1,"report":{}}"#,
        r#"{"format":"flaccompagnon-report","version":1,"report":{"files":null}}"#,
        r#"{"format":"flaccompagnon-report","version":1,"report":{"files":"not an array"}}"#,
    ] {
        assert!(
            parse_report(text).is_err(),
            "accepted malformed report {text}"
        );
    }
    for (version_field, expected_version) in [("\"version\":1,", 1), ("\"version\":0,", 0), ("", 0)]
    {
        let report = parse_report(&format!(
            r#"{{"format":"flaccompagnon-report",{version_field}"report":{{"root":"/music","files":[],"future_measurement":true}}}}"#
        ))
        .unwrap();
        assert_eq!(report.version, expected_version);
        assert!(report.files.is_empty());
    }
    let report = parse_report(
        r#"{"format":"flaccompagnon-report","version":1,"report":{"files":[{}, {"path":null}, {"path":"/music/a.flac","future_measurement":{"value":42}}]}}"#,
    )
    .unwrap();
    assert_eq!(report.files.len(), 1, "pathless entries remain skippable");
    assert_eq!(report.files[0].path, "/music/a.flac");
    assert!(
        report.files[0]
            .source_data
            .as_ref()
            .unwrap()
            .get("future_measurement")
            .is_some()
    );
}

#[cfg(unix)]
#[test]
fn a_catalogued_hash_candidate_replaced_by_a_fifo_does_not_block_import() {
    const CHILD_PATH: &str = "AEDE_ANALYSIS_FIFO_TEST_PATH";
    const CHILD_DIGEST: &str = "AEDE_ANALYSIS_FIFO_TEST_DIGEST";
    let bytes = include_bytes!("../tests/fixtures/track.flac");
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let path = path.to_str().unwrap();
        let mut catalog = catalog_holding(path, bytes.len() as u64, 10);
        let record = FileAnalysis {
            file_md5: Some(std::env::var(CHILD_DIGEST).unwrap()),
            ..record_for("/old-library/track.flac", bytes.len() as u64, 10)
        };
        let outcome = merge_into(&mut catalog, vec![record], 99);
        assert_eq!(outcome.waiting, 1);
        assert_eq!(outcome.moved, 0);
        assert_eq!(catalog.analyses[0].path, "/old-library/track.flac");
        return;
    }

    let dir = TestDirectory::new("fifo_hash_candidate");
    let path = dir.file("track.flac", bytes);
    let digest = file_md5(Path::new(&path)).unwrap();
    let link = dir.path.join("linked.flac");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert_eq!(file_md5(&link).unwrap(), digest);
    assert!(file_md5(&dir.path).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    for candidate in [Path::new(&path), link.as_path()] {
        // Isolate the public import call: an old blocking reader must be
        // killed and reaped instead of leaving the test suite waiting too.
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "analysis::tests::a_catalogued_hash_candidate_replaced_by_a_fifo_does_not_block_import",
                "--nocapture",
            ])
            .env(CHILD_PATH, candidate)
            .env(CHILD_DIGEST, &digest)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let timed_out = loop {
            if child.try_wait().unwrap().is_some() {
                break false;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                break true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        let output = child.wait_with_output().unwrap();
        assert!(!timed_out, "import blocked on {}", candidate.display());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

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
fn a_report_in_memory_uses_the_same_reader_as_an_imported_file() {
    let report = parse_report(&example("")).expect("a report in memory");
    assert_eq!(report.files[0].path, "/music/Danzig/01 7th House.flac");
    assert_eq!(report.files[0].md5_state.as_deref(), Some("Match"));
}

#[test]
fn reads_and_keeps_the_optional_whole_file_md5() {
    let digest = "5d41402abc4b2a76b9719d911017c592";
    let text = example(&format!(", \"file_md5\": \"{digest}\""));
    let report = parse_report(&text).expect("report with whole-file digest");
    assert_eq!(report.files[0].file_md5.as_deref(), Some(digest));

    let dir = TestDirectory::new("persist_digest");
    let catalog = Catalog {
        analyses: report.files,
        ..Default::default()
    };
    let path = dir.path.join("catalog.json");
    crate::store::save(&catalog, &path).expect("save analysis");
    let loaded = crate::store::load(&path)
        .expect("read catalog")
        .expect("catalog exists");
    assert_eq!(loaded.analyses[0].file_md5.as_deref(), Some(digest));
}

#[test]
fn measured_loudness_survives_import_and_catalog_storage() {
    let report = parse_report(&example(", \"integrated_lufs\": -14.25"))
        .expect("report with integrated loudness");
    assert_eq!(report.files[0].integrated_lufs, Some(-14.25));

    let dir = TestDirectory::new("persist_loudness");
    let catalog = Catalog {
        analyses: report.files,
        ..Default::default()
    };
    let path = dir.path.join("catalog.json");
    crate::store::save(&catalog, &path).expect("save analysis");
    let loaded = crate::store::load(&path)
        .expect("load catalog")
        .expect("catalog exists");
    assert_eq!(loaded.analyses[0].integrated_lufs, Some(-14.25));
}

#[test]
fn complete_flaccompagnon_results_survive_import_and_conclusions_storage() {
    let text = example(
        r#", "bit_depth_evidence": {"stored_bits": 16, "method": "Stored"},
        "lattice_score": 0.19,
        "phase_correlation": 0.77,
        "phase_inverted": false,
        "stereo_balance": {"state": "Measured", "right_minus_left_db": -0.11},
        "high_frequency_stereo": {"side_to_mid_db": -10.4, "narrowed": false},
        "dc_offset": {"channel_means": [-0.0003, -0.0002], "max_abs": 0.0003},
        "local_phase": {"bands": [{"low_hz": 20, "summary": {"correlation": 0.82}}]},
        "loudness_peaks": {"momentary": {"lufs": -9.67, "start_secs": 324.62}},
        "loudness_range_lu": 6.02,
        "discontinuities": {"clicks": {"count": 1080, "events": [{"channel": 1, "start_secs": 0.42}]}},
        "future_measurement": {"nested": [false, null, 3.25]}
        "#,
    )
    .replace("\"transcoding\": \"none\"", "\"transcoding\": true");
    let source = crate::json::parse(&text).expect("source report");
    let expected = source
        .get("report")
        .unwrap()
        .get("files")
        .unwrap()
        .as_arr()
        .unwrap()[0]
        .clone();
    let report = parse_report(&text).expect("report with advanced results");
    assert_eq!(report.files[0].transcoding.as_deref(), Some("detected"));
    assert!(report.files[0].suspect_encoding());
    assert_eq!(report.files[0].source_data.as_ref(), Some(&expected));

    let dir = TestDirectory::new("persist_full_report");
    let catalog = Catalog {
        analyses: report.files,
        ..Default::default()
    };
    let path = dir.path.join("catalog.json");
    crate::store::save(&catalog, &path).expect("save analysis");
    let loaded = crate::store::load(&path)
        .expect("load catalog")
        .expect("catalog exists");
    assert_eq!(loaded.analyses[0].source_data.as_ref(), Some(&expected));
}

#[test]
fn current_flaccompagnon_negative_transcoding_result_is_known() {
    let text = example("").replace("\"transcoding\": \"none\"", "\"transcoding\": false");
    let report = parse_report(&text).expect("report with a boolean verdict");
    assert_eq!(report.files[0].transcoding.as_deref(), Some("none"));
    assert!(!report.files[0].suspect_encoding());
}

struct TestDirectory {
    path: std::path::PathBuf,
}

impl TestDirectory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("aede_analysis_{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary directory");
        Self { path }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.path.join(name);
        std::fs::write(&path, bytes).expect("fixture bytes");
        path.to_string_lossy().into_owned()
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn a_renamed_file_is_found_by_its_bytes_even_when_its_date_changes() {
    let dir = TestDirectory::new("renamed_by_md5");
    let path = dir.file("new-name.flac", b"hello");
    let digest = "5d41402abc4b2a76b9719d911017c592";
    assert_eq!(file_md5(Path::new(&path)).unwrap(), digest);
    let mut catalog = catalog_holding(&path, 5, 20);
    let record = FileAnalysis {
        file_md5: Some(digest.into()),
        ..record_for("/old-library/old-name.flac", 5, 10)
    };

    let outcome = merge_into(&mut catalog, vec![record], 99);
    assert_eq!(outcome.moved, 1);
    assert_eq!(catalog.analyses[0].path, path);
    assert_eq!(catalog.analyses[0].modified_unix, 20);
    assert_eq!(catalog.pending_analyses(), 0);
}

#[test]
fn a_matching_name_and_size_with_different_bytes_is_not_attached() {
    let dir = TestDirectory::new("different_bytes");
    let path = dir.file("track.flac", b"world");
    let mut catalog = catalog_holding(&path, 5, 10);
    let record = FileAnalysis {
        file_md5: Some("5d41402abc4b2a76b9719d911017c592".into()),
        ..record_for("/old-library/track.flac", 5, 10)
    };

    let outcome = merge_into(&mut catalog, vec![record], 0);
    assert_eq!(outcome.waiting, 1);
    assert_eq!(outcome.moved, 0);
}

#[test]
fn identical_copies_do_not_claim_a_moved_analysis() {
    let dir = TestDirectory::new("identical_copies");
    let first = dir.file("first.flac", b"hello");
    let second = dir.file("second.flac", b"hello");
    let mut catalog = catalog_holding(&first, 5, 10);
    catalog.files.push(crate::model::AudioFile {
        id: 1,
        path: second,
        size: 5,
        mtime: 10,
        ..Default::default()
    });
    let record = FileAnalysis {
        file_md5: Some("5d41402abc4b2a76b9719d911017c592".into()),
        ..record_for("/old-library/track.flac", 5, 10)
    };

    let outcome = merge_into(&mut catalog, vec![record], 0);
    assert_eq!(outcome.waiting, 1);
    assert_eq!(outcome.moved, 0);
}

#[test]
fn a_waiting_digest_attaches_after_the_new_location_is_scanned() {
    let dir = TestDirectory::new("reconcile_digest");
    let path = dir.file("renamed.flac", b"hello");
    let record = FileAnalysis {
        file_md5: Some("5d41402abc4b2a76b9719d911017c592".into()),
        ..record_for("/old-library/track.flac", 5, 10)
    };
    let mut catalog = Catalog::default();
    assert_eq!(merge_into(&mut catalog, vec![record], 0).waiting, 1);
    catalog.files.push(crate::model::AudioFile {
        id: 0,
        path: path.clone(),
        size: 5,
        mtime: 20,
        ..Default::default()
    });

    assert_eq!(reconcile(&mut catalog), 1);
    assert_eq!(catalog.analyses[0].path, path);
    assert_eq!(catalog.analyses[0].modified_unix, 20);
}

#[test]
fn ambiguous_name_and_size_never_attach_to_the_first_album() {
    let mut catalog = catalog_holding("/music/first/01 Intro.flac", 500, 10);
    catalog.files.push(crate::model::AudioFile {
        id: 1,
        path: "/music/second/01 Intro.flac".into(),
        size: 500,
        mtime: 10,
        ..Default::default()
    });
    let record = record_for("/old-library/01 Intro.flac", 500, 10);
    let outcome = merge_into(&mut catalog, vec![record], 0);
    assert_eq!(outcome.moved, 0);
    assert_eq!(outcome.waiting, 1);
    assert_eq!(catalog.analyses[0].path, "/old-library/01 Intro.flac");
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
    // Named by where the *file* actually is, not by what the report called
    // it — the record found the file by name and size, and that is the
    // folder somebody needs to point FlacCompagnon at again.
    assert_eq!(
        outcome.stale_folders,
        BTreeMap::from([("/music".to_string(), 1)]),
        "a discarded record still says where FlacCompagnon has to run again"
    );
}

#[test]
fn a_record_stale_by_its_own_path_still_names_the_folder() {
    // The other route to the same outcome: the path matches exactly, so
    // there was never a name-and-size lookup to read a folder from — the
    // record's own path is the only one there is.
    let mut catalog = catalog_holding("/music/Blizzard of Ozz/01.flac", 500, 20);
    let record = record_for("/music/Blizzard of Ozz/01.flac", 500, 10);

    let outcome = merge_into(&mut catalog, vec![record], 0);
    assert_eq!(outcome.matched, 0);
    assert_eq!(outcome.stale, 1);
    assert_eq!(
        outcome.stale_folders,
        BTreeMap::from([("/music/Blizzard of Ozz".to_string(), 1)])
    );
}

#[test]
fn stale_folders_are_capped_the_same_way_waiting_ones_are() {
    // The exact shape `the_cap_bounds_the_rows_and_not_the_counts` checks
    // for `waiting_folders`: more distinct folders than can be listed must
    // not lose the true count of the ones that are.
    let mut catalog = Catalog::default();
    let mut records = Vec::new();
    for folder in 0..FOLDERS_SHOWN + 5 {
        let path = format!("/music/album{folder:02}/01.flac");
        catalog.files.push(crate::model::AudioFile {
            id: folder as u32,
            path: path.clone(),
            size: 500,
            mtime: 20,
            ..Default::default()
        });
        for track in 0..3 {
            let _ = track;
            records.push(record_for(&path, 500, 10));
        }
    }
    let outcome = merge_into(&mut catalog, records, 0);
    assert_eq!(outcome.stale, (FOLDERS_SHOWN + 5) * 3);
    assert_eq!(outcome.stale_folders.len(), FOLDERS_SHOWN);
    assert!(
        outcome.stale_folders.values().all(|&n| n == 3),
        "a folder that is shown is shown whole: {:?}",
        outcome.stale_folders
    );
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
    for folder in 0..FOLDERS_SHOWN + 5 {
        for track in 0..3 {
            records.push(record_for(
                &format!("/music/album{folder:02}/{track}.flac"),
                500,
                10,
            ));
        }
    }
    let outcome = merge_into(&mut catalog, records, 0);
    assert_eq!(outcome.waiting, (FOLDERS_SHOWN + 5) * 3);
    assert_eq!(outcome.waiting_folders.len(), FOLDERS_SHOWN);
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
