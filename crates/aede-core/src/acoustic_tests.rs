use super::*;

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
