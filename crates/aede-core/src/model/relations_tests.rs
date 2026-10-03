use super::*;
use crate::model::build;
use crate::model::tests::track;

#[test]
fn the_same_album_twice_is_linked_and_qualified() {
    // Same album in two folders: a copy when the encoding matches, another
    // edition when it does not. The model keeps both — they are two sets of
    // files — and says which case it is.
    let make = |folder: &str, codec: &str, rate: u32| {
        let mut f = track(
            &format!("{folder}/01.flac"),
            &[
                ("title", "Brand New God"),
                ("artist", "Danzig"),
                ("albumartist", "Danzig"),
                ("album", "Danzig 4"),
                ("tracknumber", "1"),
            ],
            120_000,
        );
        f.tags.properties.codec = codec.to_string();
        f.tags.properties.sample_rate = Some(rate);
        f.tags.properties.bit_depth = Some(if rate > 48_000 { 24 } else { 16 });
        f
    };
    let c = build(
        vec![
            make("/m/A", "flac", 44_100),
            make("/m/B", "flac", 44_100),
            make("/m/C", "flac", 96_000),
        ],
        vec!["/m".into()],
        0,
        &[],
    );
    assert_eq!(c.releases.len(), 3, "three folders, three releases");
    let a = c.releases[0].id;
    let copies = c.related_releases(a, DUPLICATE);
    assert_eq!(copies.len(), 1, "one identical copy");
    let others = c.related_releases(a, OTHER_EDITION);
    assert_eq!(others.len(), 1, "one differently encoded copy");
    assert_ne!(copies[0], others[0]);
    // Symmetric: the link is navigable from either side.
    assert!(c.related_releases(copies[0], DUPLICATE).contains(&a));
}

#[test]
fn two_editions_in_two_folders_stay_two_releases() {
    // Same title, same artist, different folder: two pressings of one
    // record must not be merged into a single release.
    let fields = [
        ("title", "So What"),
        ("artist", "Miles Davis"),
        ("albumartist", "Miles Davis"),
        ("album", "Kind of Blue"),
    ];
    let c = build(
        vec![
            track("/m/Miles Davis/Kind of Blue/01.flac", &fields, 1000),
            track(
                "/m/Miles Davis/Kind of Blue (2011 remaster)/01.flac",
                &fields,
                1000,
            ),
        ],
        vec!["/m".into()],
        0,
        &[],
    );
    assert_eq!(c.releases.len(), 2, "the folder tells the editions apart");
    assert_eq!(c.artists.len(), 1, "but the artist is shared");
}

#[test]
fn albums_with_different_disc_positions_are_not_duplicate_track_lists() {
    let make = |edition: &str, disc: u32, title: &str| {
        let disc_tag = disc.to_string();
        track(
            &format!("/m/{edition}/Disc {disc}/01.flac"),
            &[
                ("title", title),
                ("artist", "Band"),
                ("albumartist", "Band"),
                ("album", "Record"),
                ("tracknumber", "1"),
                ("discnumber", &disc_tag),
            ],
            60_000,
        )
    };
    let catalog = build(
        vec![
            make("A", 1, "First"),
            make("A", 2, "Second"),
            make("B", 1, "Second"),
            make("B", 2, "First"),
        ],
        vec!["/m".into()],
        0,
        &[],
    );
    assert_eq!(catalog.releases.len(), 2);
    // Loading rules from before disc positions were compared repairs the old
    // false positive without rescanning the files.
    let mut previous = catalog.clone();
    for (source_id, target_id) in [(0, 1), (1, 0)] {
        previous.relations.push(Relation {
            source_kind: EntityKind::Release,
            source_id,
            target_kind: EntityKind::Release,
            target_id,
            kind: DUPLICATE.into(),
            weight: 1,
            source: "tags".into(),
        });
    }
    let mut saved = crate::store::to_json(&previous);
    saved.set("relation_rules", 2u32.into());
    let loaded = crate::store::from_json(&saved).expect("upgrade the inferred relations");
    for catalog in [&catalog, &loaded] {
        for release in &catalog.releases {
            assert!(catalog.related_releases(release.id, DUPLICATE).is_empty());
            assert!(
                catalog
                    .related_releases(release.id, OTHER_EDITION)
                    .is_empty()
            );
        }
    }
}

#[test]
fn encoding_quality_is_compared_per_track_instead_of_per_album_set() {
    let make = |edition: &str, number: u32, codec: &str| {
        let number = number.to_string();
        let mut file = track(
            &format!("/m/{edition}/{number}.{codec}"),
            &[
                ("title", &format!("Song {number}")),
                ("artist", "Band"),
                ("albumartist", "Band"),
                ("album", "Record"),
                ("tracknumber", &number),
            ],
            60_000,
        );
        file.tags.properties.codec = codec.into();
        file.tags.properties.lossless = codec == "flac";
        file.tags.properties.sample_rate = Some(44_100);
        file.tags.properties.bit_depth = (codec == "flac").then_some(16);
        file.tags.properties.bitrate_kbps = (codec == "mp3").then_some(320);
        file
    };
    let catalog = build(
        vec![
            make("A", 1, "flac"),
            make("A", 2, "mp3"),
            make("B", 1, "mp3"),
            make("B", 2, "flac"),
        ],
        vec!["/m".into()],
        0,
        &[],
    );
    assert_eq!(catalog.releases.len(), 2);
    let mut previous = catalog.clone();
    for relation in &mut previous.relations {
        if relation.kind == OTHER_EDITION {
            relation.kind = DUPLICATE.into();
        }
    }
    let mut saved = crate::store::to_json(&previous);
    saved.set("relation_rules", 2u32.into());
    let loaded = crate::store::from_json(&saved).expect("upgrade per-track quality comparisons");
    for catalog in [&catalog, &loaded] {
        for release in &catalog.releases {
            assert!(catalog.related_releases(release.id, DUPLICATE).is_empty());
            assert_eq!(catalog.related_releases(release.id, OTHER_EDITION).len(), 1);
        }
    }
}

#[test]
fn every_canonical_object_link_is_navigable_both_ways() {
    let c = build(
        vec![track(
            "/m/Band/Record/01.flac",
            &[
                ("title", "Song"),
                ("artist", "Band"),
                ("albumartist", "Band"),
                ("album", "Record"),
                ("label", "A Label"),
                ("musicbrainz_recordingid", "recording-id"),
                ("musicbrainz_workid", "work-id"),
                ("musicbrainz_releasegroupid", "group-id"),
                ("performer", "Guest"),
                ("composer", "Writer"),
            ],
            1000,
        )],
        vec!["/m".into()],
        0,
        &[],
    );
    let track = &c.tracks[0];
    let recording = &c.recordings[0];
    let release = &c.releases[0];
    let work = &c.works[0];
    let group = &c.release_groups[0];
    let label = &c.labels[0];
    let band = c.find_artist("Band").expect("band");
    let guest = c.find_artist("Guest").expect("guest");
    let writer = c.find_artist("Writer").expect("writer");

    assert_eq!(
        c.related_entities(
            EntityKind::Track,
            track.id,
            PLACEMENT_OF,
            EntityKind::Recording
        ),
        vec![recording.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Recording,
            recording.id,
            PLACED_AS,
            EntityKind::Track
        ),
        vec![track.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Recording,
            recording.id,
            PERFORMANCE_OF,
            EntityKind::Work
        ),
        vec![work.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Work,
            work.id,
            HAS_RECORDING,
            EntityKind::Recording
        ),
        vec![recording.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Release,
            release.id,
            EDITION_OF,
            EntityKind::ReleaseGroup
        ),
        vec![group.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::ReleaseGroup,
            group.id,
            HAS_EDITION,
            EntityKind::Release
        ),
        vec![release.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Release,
            release.id,
            RELEASED_BY,
            EntityKind::Label
        ),
        vec![label.id]
    );
    assert_eq!(
        c.related_entities(EntityKind::Label, label.id, RELEASED, EntityKind::Release),
        vec![release.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Artist,
            band.id,
            DISCOGRAPHY,
            EntityKind::Release
        ),
        vec![release.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Artist,
            guest.id,
            APPEARS_ON,
            EntityKind::Release
        ),
        vec![release.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Artist,
            writer.id,
            CONTRIBUTED_TO,
            EntityKind::Release
        ),
        vec![release.id]
    );
    assert_eq!(
        c.related_entities(
            EntityKind::Artist,
            writer.id,
            "credit:composer",
            EntityKind::Recording
        ),
        vec![recording.id]
    );
}

#[test]
fn a_compilation_appearance_is_not_a_discography_or_guest_album() {
    let c = build(
        vec![track(
            "/m/Various/Collection/01.flac",
            &[
                ("title", "Song"),
                ("artist", "Guest"),
                ("albumartist", "Various Artists"),
                ("album", "Collection"),
                ("compilation", "1"),
            ],
            1000,
        )],
        vec!["/m".into()],
        0,
        &[],
    );
    let guest = c.find_artist("Guest").expect("guest");
    let release = &c.releases[0];
    assert_eq!(
        c.related_entities(
            EntityKind::Artist,
            guest.id,
            COMPILATION_APPEARANCE,
            EntityKind::Release
        ),
        vec![release.id]
    );
    assert!(
        c.related_entities(
            EntityKind::Artist,
            guest.id,
            APPEARS_ON,
            EntityKind::Release
        )
        .is_empty()
    );
    assert!(
        c.related_entities(
            EntityKind::Artist,
            guest.id,
            DISCOGRAPHY,
            EntityKind::Release
        )
        .is_empty()
    );
}
