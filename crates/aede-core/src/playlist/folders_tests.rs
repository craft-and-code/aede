use super::*;
#[test]
fn artists_sharing_a_parent_folder_do_not_overwrite_each_others_playlists() {
    let files = [("One", "A1"), ("One", "A2"), ("Two", "B1"), ("Two", "B2")]
        .into_iter()
        .map(|(artist, album)| crate::model::ScannedFile {
            path: format!("/music/Mixed/{album}/01.flac"),
            size: 1,
            mtime: 0,
            tags: crate::tags::RawTags {
                fields: [
                    ("album".into(), vec![album.into()]),
                    ("artist".into(), vec![artist.into()]),
                    ("albumartist".into(), vec![artist.into()]),
                ]
                .into(),
                ..Default::default()
            },
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        })
        .collect();
    let catalog = crate::model::build(files, vec!["/music".into()], 0, &[]);
    let wanted = render_folders(&catalog, discographies(&catalog, &[]), Style::Simple).unwrap();
    assert_eq!(
        wanted.len(),
        1,
        "one physical folder has only one output path"
    );
    assert_eq!(wanted[0].1.lines().count(), 4);
}

#[test]
fn releases_in_the_same_folder_share_one_complete_playlist() {
    let catalog = crate::model::build(
        vec![
            crate::model::ScannedFile {
                path: "/music/Album/01.flac".into(),
                size: 1,
                mtime: 1,
                tags: crate::tags::RawTags {
                    fields: [("album".into(), vec!["First album".into()])].into(),
                    ..Default::default()
                },
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            },
            crate::model::ScannedFile {
                path: "/music/Album/02.flac".into(),
                size: 1,
                mtime: 1,
                tags: crate::tags::RawTags {
                    fields: [("album".into(), vec!["Second album".into()])].into(),
                    ..Default::default()
                },
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            },
        ],
        vec!["/music".into()],
        0,
        &[],
    );
    assert_eq!(catalog.releases.len(), 2);
    let wanted = render_folders(&catalog, albums(&catalog, &[]), Style::Simple).unwrap();
    assert_eq!(wanted.len(), 1);
    assert_eq!(
        wanted[0].1.lines().collect::<Vec<_>>(),
        ["01.flac", "02.flac"]
    );
}
