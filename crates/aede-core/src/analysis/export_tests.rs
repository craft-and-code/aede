use super::*;
use crate::{
    analysis::FileAnalysis,
    json,
    model::{AudioFile, Release, Track},
};
fn fixture() -> Catalog {
    let mut catalog = Catalog::default();
    for id in 0..3 {
        let path = format!("/music/album/{id}.flac");
        catalog.files.push(AudioFile {
            id,
            path: path.clone(),
            size: 10,
            mtime: 2,
            ..Default::default()
        });
        catalog.tracks.push(Track {
            id,
            file_id: id,
            ..Default::default()
        });
        catalog.analyses.push(FileAnalysis { path: path.clone(), source: "flaccompagnon".into(), source_version: 1, size_bytes: 10, modified_unix: 2,
            source_data: Some(json::parse(&format!(r#"{{"path":"{path}","flac_md5":{{"state":"Match"}},"unknown_future":{{"value":42}},"size_bytes":10,"modified_unix":2}}"#)).unwrap()), ..Default::default() });
    }
    catalog.releases.push(Release {
        id: 0,
        folder: "/music/album".into(),
        track_ids: vec![1, 0],
        ..Default::default()
    });
    catalog
}

#[test]
fn downloaded_reports_keep_legacy_measurement_dates_when_imported_again() {
    let mut catalog = fixture();
    let date = 1_750_000_000_000_000_100;
    catalog.analyses[0].result_at_ns = date;
    let original = catalog.analyses[0].clone();
    let downloaded = report(&catalog, EntityKind::Track, 0).unwrap();
    assert_eq!(catalog.analyses[0], original, "export remains read-only");
    let imported = crate::analysis::parse_report_at(&downloaded, date + 200).unwrap();
    assert_eq!(imported.files[0].result_at_ns, date);
    let mut entry = imported.files[0].source_data.clone().unwrap();
    if let json::Json::Obj(fields) = &mut entry {
        fields.remove(crate::analysis::RESULT_DATE_FIELD);
    }
    assert_eq!(entry, original.source_data.clone().unwrap());
    catalog.analyses[0].result_at_ns = date + 100;
    let outcome = crate::analysis::merge_into(&mut catalog, imported.files, 42);
    assert_eq!(outcome.older, 1);
    assert_eq!(catalog.analyses[0].result_at_ns, date + 100);

    let undated = report(&fixture(), EntityKind::Track, 0).unwrap();
    assert_eq!(
        crate::analysis::parse_report_at(&undated, date)
            .unwrap()
            .files[0]
            .result_at_ns,
        0,
        "an explicit unknown date must not become the download date"
    );
}

#[test]
fn extracts_one_track_from_an_album_or_artist_import_without_losing_source_fields() {
    let c = fixture();
    let output = json::parse(&report(&c, EntityKind::Track, 1).unwrap()).unwrap();
    assert_eq!(
        output.field_str("format").as_deref(),
        Some("flaccompagnon-report")
    );
    let inner = output.get("report").unwrap();
    let mut expected = c.analyses[1].source_data.clone().unwrap();
    expected.set("aede_result_at_ns", "0".into());
    assert_eq!(inner.get("files").unwrap().as_arr().unwrap(), &[expected]);
    assert_eq!(inner.get("has_flac").unwrap().as_bool(), Some(true));
    assert_eq!(
        crate::analysis::parse_report(&report(&c, EntityKind::Track, 1).unwrap())
            .unwrap()
            .files
            .len(),
        1
    );
}
#[test]
fn combines_individual_track_reports_in_album_order_without_unrelated_files() {
    let c = fixture();
    let output = json::parse(&report(&c, EntityKind::Release, 0).unwrap()).unwrap();
    let mut expected_first = c.analyses[1].source_data.clone().unwrap();
    expected_first.set("aede_result_at_ns", "0".into());
    let mut expected_second = c.analyses[0].source_data.clone().unwrap();
    expected_second.set("aede_result_at_ns", "0".into());
    assert_eq!(
        output
            .get("report")
            .unwrap()
            .get("files")
            .unwrap()
            .as_arr()
            .unwrap(),
        &[expected_first, expected_second]
    );
    assert_eq!(
        output.get("report").unwrap().field_str("root").as_deref(),
        Some("/music/album")
    );
}
#[test]
fn refuses_partial_stale_legacy_and_newer_reports_without_inventing_measurements() {
    let mut c = fixture();
    c.analyses.remove(0);
    assert_eq!(
        report(&c, EntityKind::Release, 0),
        Err(ExportError::MissingAnalysis)
    );
    let mut c = fixture();
    c.analyses[0].modified_unix = 1;
    assert_eq!(
        report(&c, EntityKind::Track, 0),
        Err(ExportError::StaleAnalysis)
    );
    let mut c = fixture();
    c.analyses[0].source_data = None;
    assert_eq!(
        report(&c, EntityKind::Track, 0),
        Err(ExportError::IncompleteAnalysis)
    );
    let mut c = fixture();
    c.analyses[0].source_version = 2;
    assert_eq!(
        report(&c, EntityKind::Track, 0),
        Err(ExportError::UnsupportedVersion)
    );
    assert_eq!(
        report(&c, EntityKind::Artist, 0),
        Err(ExportError::InvalidSelection)
    );
    assert_eq!(
        report(&c, EntityKind::Track, 99),
        Err(ExportError::InvalidSelection)
    );
}
#[test]
fn moved_source_entries_retain_their_original_attribution() {
    let mut c = fixture();
    c.analyses[0]
        .source_data
        .as_mut()
        .unwrap()
        .set("path", json::Json::Str("/old-library/0.flac".into()));
    let output = json::parse(&report(&c, EntityKind::Track, 0).unwrap()).unwrap();
    assert_eq!(
        output
            .get("report")
            .unwrap()
            .get("files")
            .unwrap()
            .as_arr()
            .unwrap()[0]
            .field_str("path")
            .as_deref(),
        Some("/old-library/0.flac")
    );
}

#[test]
fn export_budget_and_duplicate_files_are_explicit() {
    let mut c = fixture();
    c.releases[0].track_ids = vec![0, 0];
    assert_eq!(
        crate::analysis::parse_report(&report(&c, EntityKind::Release, 0).unwrap())
            .unwrap()
            .files
            .len(),
        1
    );
    c.releases[0].track_ids = vec![0; 2049];
    assert_eq!(
        report(&c, EntityKind::Release, 0),
        Err(ExportError::TooLarge)
    );
    c.analyses[0]
        .source_data
        .as_mut()
        .unwrap()
        .set("oversize", json::Json::Str("x".repeat(16 * 1024 * 1024)));
    assert_eq!(report(&c, EntityKind::Track, 0), Err(ExportError::TooLarge));
}
