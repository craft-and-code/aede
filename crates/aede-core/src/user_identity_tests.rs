use super::*;

#[test]
fn indexed_resolution_keeps_canonical_and_legacy_reference_behaviour() {
    let mut catalog = super::super::test_support::library(&["/m/Legion/Disc 2/02.flac"]);
    catalog.recordings[0].mbid = Some("recording-id".into());
    let kinds = [
        EntityKind::Track,
        EntityKind::Release,
        EntityKind::Artist,
        EntityKind::Recording,
        EntityKind::Work,
        EntityKind::ReleaseGroup,
        EntityKind::Label,
        EntityKind::Genre,
    ]
    .into_iter()
    .collect();
    let index = ReferenceIndex::new(&catalog, kinds);
    for reference in [
        EntityRef::of(&catalog, EntityKind::Track, 0).unwrap(),
        EntityRef::of(&catalog, EntityKind::Release, 0).unwrap(),
        EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap(),
        EntityRef::of(&catalog, EntityKind::Recording, 0).unwrap(),
        EntityRef::new(EntityKind::Release, "deicide|legion|/m/Legion/Disc 1"),
        EntityRef::new(EntityKind::Recording, "local:/m/Legion/Disc 2/02.flac"),
        EntityRef::new(EntityKind::Release, "deicide|legion|/another/Disc 1"),
        EntityRef::new(EntityKind::Artist, "missing"),
    ] {
        assert_eq!(
            index.resolve(&reference),
            reference.resolve(&catalog),
            "{}",
            reference.to_token()
        );
    }
}

#[test]
fn identity_with_a_fingerprint_requires_the_same_known_fingerprint() {
    let mut file = AudioFile {
        path: "/old/a.flac".into(),
        size: 100,
        ..Default::default()
    };
    file.fingerprint = Some(crate::fingerprint::Fingerprint {
        data: "first".into(),
        seconds: 1,
    });
    let identity = TrackIdentity::of(&file);
    file.path = "/new/a.flac".into();
    assert!(identity.matches(&file));
    file.fingerprint = None;
    assert!(!identity.matches(&file));
    file.fingerprint = Some(crate::fingerprint::Fingerprint {
        data: "other".into(),
        seconds: 1,
    });
    assert!(!identity.matches(&file));
    assert_eq!(
        TrackIdentity::from_json(&identity.to_json()),
        Some(identity)
    );
}
