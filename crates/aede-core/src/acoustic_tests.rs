use super::*;

#[test]
fn dated_reports_remain_readable_upstream_without_losing_integer_precision() {
    let folder = std::env::temp_dir().join(format!("aede_dated_report_{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("track.flac");
    std::fs::write(&path, include_bytes!("../tests/fixtures/track.flac")).unwrap();
    let mut report = analyze_album(
        &folder,
        std::slice::from_ref(&path),
        &ScanOptions::default(),
        1,
    );
    assert!(report.files[0].error.is_none());
    report.files[0].size_bytes = u64::MAX;
    let date = 1_750_000_000_000_000_100;
    let dates = BTreeMap::from([(report.files[0].path.clone(), date)]);
    let output = report_json_with_dates(&report, &dates, &BTreeMap::new()).unwrap();
    let upstream = fc::report::parse_json(&output).unwrap();
    assert_eq!(upstream.files[0].size_bytes, u64::MAX);
    assert_eq!(
        upstream.files[0].dr_db,
        report.files[0]
            .dr_db
            .map(|value| (value * 1e9).round() / 1e9)
    );
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    let exact_date = date.to_string();
    assert_eq!(
        value["report"]["files"][0]["aede_result_at_ns"].as_str(),
        Some(exact_date.as_str())
    );
    let mut extended = value;
    extended["report"]["files"][0]["future_integer"] = serde_json::json!(u64::MAX);
    extended["report"]["files"][0]["clipping"]["future_peak"] = serde_json::json!(0.125);
    let text = serde_json::to_string(&extended).unwrap();
    let saved = saved_report(&text, date + 100).unwrap();
    assert_eq!(saved[0].result_at_ns, date);
    let reused = BTreeMap::from([(saved[0].analysis.path.clone(), &saved[0])]);
    let output = report_json_with_dates(&report, &dates, &reused).unwrap();
    let reused_value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(reused_value, extended);
    assert!(report_json_with_dates(&report, &BTreeMap::new(), &BTreeMap::new()).is_err());
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_multi_disc_release_writes_one_report_in_its_album_folder() {
    use crate::model::{AudioFile, Release, Track};

    let catalog = Catalog {
        files: vec![
            AudioFile {
                id: 0,
                path: "/music/album/Disc 1/01.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 1,
                path: "/music/album/Disc 2/01.flac".into(),
                ..Default::default()
            },
        ],
        tracks: vec![
            Track {
                id: 0,
                file_id: 0,
                release_id: Some(0),
                ..Default::default()
            },
            Track {
                id: 1,
                file_id: 1,
                release_id: Some(0),
                ..Default::default()
            },
        ],
        releases: vec![Release {
            id: 0,
            folder: "/music/album".into(),
            track_ids: vec![0, 1],
            ..Default::default()
        }],
        ..Default::default()
    };

    let grouped = album_files(&catalog, &[]);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[Path::new("/music/album")].len(), 2);
}

#[test]
fn albums_with_same_track_name_remain_in_separate_reports() {
    use crate::model::AudioFile;
    let catalog = Catalog {
        files: vec![
            AudioFile {
                id: 0,
                path: "/music/first/01.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 1,
                path: "/music/second/01.flac".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    assert_eq!(album_files(&catalog, &[]).len(), 2);
}

#[test]
fn progress_is_reported_before_decoding_the_file() {
    let folder = std::env::temp_dir().join(format!("aede_start_progress_{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("track.wav");
    std::fs::write(&path, b"not audio").unwrap();
    let report = analyze_album_with_progress(
        &folder,
        std::slice::from_ref(&path),
        &ScanOptions::default(),
        1,
        |index, total, file| {
            assert_eq!((index, total), (1, 1));
            // Removal in the callback proves it runs before the decoder opens the file.
            std::fs::remove_file(file).unwrap();
        },
    );
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].size_bytes, 0);
    assert!(report.files[0].error.is_some());
    std::fs::remove_dir(&folder).unwrap();
}
