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

#[test]
fn relocation_requires_the_same_plural_artist_evidence() {
    let library = |path: &str, field: &str, value: &str| {
        crate::model::build(
            vec![crate::model::tests::track(
                path,
                &[
                    ("title", "Song"),
                    ("artist", "Fallback Artist"),
                    ("albumartist", "Fallback Album Artist"),
                    ("album", "Album"),
                    (field, value),
                ],
                1000,
            )],
            vec!["/m".into()],
            0,
            &[],
        )
    };
    let from = EntityRef::new(EntityKind::Track, "/old/song.flac");
    let to = EntityRef::new(EntityKind::Track, "/new/song.flac");
    for field in ["artists", "albumartists"] {
        let old = library(&from.key, field, "First Artist");
        let matching = library(&to.key, field, "First Artist");
        let different = library(&to.key, field, "Different Artist");
        let mut data = UserData::default();
        for owner in [LOCAL_USER, "other-listener"] {
            data.entry(owner, &from, 1).note = Some(format!("{owner}'s original note"));
        }
        reconcile(&mut data, &old);

        let mut moved = data.clone();
        assert_eq!(reconcile(&mut moved, &matching).moved, 2, "{field}");
        for owner in [LOCAL_USER, "other-listener"] {
            assert!(moved.find(owner, &from).is_none());
            assert_eq!(
                moved.find(owner, &to).unwrap().note,
                data.find(owner, &from).unwrap().note,
                "{field} preserves each owner's note"
            );
        }

        let mut legacy = data.clone();
        for identity in &mut legacy.track_identities {
            identity.metadata.remove(field);
        }
        let mut waiting = data.clone();
        for (records, catalog) in [(&mut waiting, &different), (&mut legacy, &matching)] {
            let report = reconcile(records, catalog);
            assert_eq!((report.moved, report.waiting), (0, 2), "{field}");
            for owner in [LOCAL_USER, "other-listener"] {
                assert_eq!(records.find(owner, &from), data.find(owner, &from));
                assert!(records.find(owner, &to).is_none());
            }
        }

        reconcile(&mut legacy, &old);
        assert_eq!(reconcile(&mut legacy, &matching).moved, 2, "{field}");
    }
}
