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
fn extracts_one_track_from_an_album_or_artist_import_without_losing_source_fields() {
    let c = fixture();
    let output = json::parse(&report(&c, EntityKind::Track, 1).unwrap()).unwrap();
    assert_eq!(
        output.field_str("format").as_deref(),
        Some("flaccompagnon-report")
    );
    let inner = output.get("report").unwrap();
    assert_eq!(
        inner.get("files").unwrap().as_arr().unwrap(),
        &[c.analyses[1].source_data.clone().unwrap()]
    );
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
    assert_eq!(
        output
            .get("report")
            .unwrap()
            .get("files")
            .unwrap()
            .as_arr()
            .unwrap(),
        &[
            c.analyses[1].source_data.clone().unwrap(),
            c.analyses[0].source_data.clone().unwrap()
        ]
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
