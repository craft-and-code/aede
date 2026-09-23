use super::*;

/// First release whose title matches, for tests that expect exactly one.
///
/// Deliberately not on `Catalog`: production code goes through
/// [`Catalog::find_releases`], which never hides the fact that several
/// albums matched. A convenience that picks one silently belongs to the
/// tests that already know there is only one.
pub(crate) fn first_release<'a>(catalog: &'a Catalog, title: &str) -> Option<&'a Release> {
    catalog.find_releases(title).0.into_iter().next()
}

use crate::tags::RawTags;

pub(crate) fn track(path: &str, fields: &[(&str, &str)], duration_ms: u64) -> ScannedFile {
    let mut tags = RawTags::default();
    for (k, v) in fields {
        tags.insert(k, *v);
    }
    tags.properties.duration_ms = Some(duration_ms);
    tags.properties.codec = "flac".into();
    tags.properties.lossless = true;
    ScannedFile {
        path: path.to_string(),
        size: 1_000_000,
        mtime: 0,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

pub(crate) fn example_catalog() -> Catalog {
    build(
        vec![
            track(
                "/m/Metallica/Ride the Lightning/01 Fight Fire with Fire.flac",
                &[
                    ("title", "Fight Fire with Fire"),
                    ("artist", "Metallica"),
                    ("album", "Ride the Lightning"),
                    ("albumartist", "Metallica"),
                    ("date", "1984"),
                    ("genre", "Thrash Metal"),
                    ("tracknumber", "1/8"),
                    ("label", "Megaforce"),
                ],
                100_000,
            ),
            track(
                "/m/Metallica/Ride the Lightning/02 Ride the Lightning.flac",
                &[
                    ("title", "Ride the Lightning"),
                    ("artist", "Metallica"),
                    ("album", "Ride the Lightning"),
                    ("albumartist", "Metallica"),
                    ("date", "1984"),
                    ("tracknumber", "2/8"),
                ],
                120_000,
            ),
            track(
                "/m/Various/Duos/01 Sous le vent.flac",
                &[
                    ("title", "Sous le vent"),
                    ("artist", "Garou feat. Céline Dion"),
                    ("album", "Duos"),
                    ("compilation", "1"),
                    ("date", "2001"),
                ],
                90_000,
            ),
        ],
        vec!["/m".to_string()],
        0,
        &[],
    )
}
