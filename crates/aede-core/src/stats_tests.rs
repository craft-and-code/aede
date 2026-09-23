use super::*;
use crate::model::{self, ScannedFile};
use crate::tags::RawTags;

// A verbose test constructor reads better than a dedicated struct for six
// fields.
#[allow(clippy::too_many_arguments)]
fn track(
    path: &str,
    artist: &str,
    album: &str,
    year: &str,
    codec: &str,
    lossless: bool,
    bits: u16,
    rate: u32,
) -> ScannedFile {
    let mut tags = RawTags::default();
    tags.insert("title", path.rsplit('/').next().unwrap_or("x"));
    tags.insert("artist", artist);
    tags.insert("album", album);
    tags.insert("albumartist", artist);
    tags.insert("date", year);
    tags.insert("genre", "Jazz");
    tags.properties.codec = codec.into();
    tags.properties.lossless = lossless;
    tags.properties.bit_depth = Some(bits);
    tags.properties.sample_rate = Some(rate);
    tags.properties.duration_ms = Some(300_000);
    if !lossless {
        tags.properties.bitrate_kbps = Some(320);
    }
    ScannedFile {
        path: path.into(),
        size: 10_000_000,
        mtime: 0,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

fn catalog() -> Catalog {
    model::build(
        vec![
            track(
                "/m/a/Blue/01.flac",
                "Alpha",
                "Blue",
                "1975",
                "flac",
                true,
                16,
                44_100,
            ),
            track(
                "/m/a/Blue/02.flac",
                "Alpha",
                "Blue",
                "1975",
                "flac",
                true,
                16,
                44_100,
            ),
            track(
                "/m/b/Red/01.flac",
                "Beta",
                "Red",
                "1988",
                "flac",
                true,
                24,
                96_000,
            ),
            track(
                "/m/c/Green/01.mp3",
                "Gamma",
                "Green",
                "2003",
                "mp3",
                false,
                0,
                44_100,
            ),
        ],
        vec!["/m".into()],
        0,
        &[],
    )
}

#[test]
fn basic_counts() {
    let s = compute(&catalog());
    assert_eq!(s.files, 4);
    assert_eq!(s.tracks, 4);
    assert_eq!(s.releases, 3);
    assert_eq!(s.artists, 3);
    assert_eq!(s.album_artists, 3);
    assert_eq!(s.genres, 1);
    assert_eq!(s.total_duration_ms, 1_200_000);
    assert_eq!(s.total_bytes, 40_000_000);
    assert_eq!(s.orphan_tracks, 0);
}

#[test]
fn breakdown_by_codec() {
    let s = compute(&catalog());
    assert_eq!(s.by_codec[0].label, "FLAC");
    assert_eq!(s.by_codec[0].count, 3);
    assert_eq!(s.by_codec[1].label, "MP3");
    assert_eq!(s.by_codec[1].count, 1);
}

#[test]
fn quality_classes() {
    let s = compute(&catalog());
    let hires = s
        .by_quality
        .iter()
        .find(|b| b.label.contains("Hi-res"))
        .unwrap();
    assert_eq!(hires.count, 1);
    let lossless = s
        .by_quality
        .iter()
        .find(|b| b.label.contains("CD"))
        .unwrap();
    assert_eq!(lossless.count, 2);
}

#[test]
fn breakdown_by_decade() {
    let s = compute(&catalog());
    let labels: Vec<&str> = s.by_decade.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["1970", "1980", "2000"]);
}

#[test]
fn completeness_rates() {
    let s = compute(&catalog());
    assert_eq!(s.year_ratio, 1.0);
    assert_eq!(s.genre_ratio, 1.0);
    assert_eq!(s.mbid_ratio, 0.0);
    assert_eq!(s.cover_ratio, 0.0);
}

#[test]
fn rankings() {
    let c = catalog();
    let top = top_artists(&c, 10);
    assert_eq!(top[0].1, 2, "Alpha has two tracks");
    assert_eq!(c.artist(top[0].0).unwrap().name, "Alpha");
    assert_eq!(top_genres(&c, 10)[0].1, 4);
}

#[test]
fn empty_catalog_does_not_divide_by_zero() {
    let s = compute(&Catalog::default());
    assert_eq!(s.files, 0);
    assert_eq!(s.cover_ratio, 0.0);
    assert!(s.by_codec.is_empty());
}
