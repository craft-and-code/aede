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
