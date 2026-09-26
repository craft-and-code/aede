use super::*;

#[test]
fn synthetic_tracks_form_distinct_recordings_and_twelve_track_albums() {
    let catalog = build(
        (0..25).map(synthetic_file).collect(),
        vec!["/aede-benchmark".into()],
        1_700_000_000,
        &[],
    );
    assert_eq!(catalog.files.len(), 25);
    assert_eq!(catalog.tracks.len(), 25);
    assert_eq!(catalog.recordings.len(), 25);
    assert_eq!(catalog.releases.len(), 3);
    assert_eq!(catalog.releases[0].track_ids.len(), 12);
    assert_eq!(catalog.releases[1].track_ids.len(), 12);
    assert_eq!(catalog.releases[2].track_ids.len(), 1);
    assert_eq!(catalog.artists.len(), 1);
}
