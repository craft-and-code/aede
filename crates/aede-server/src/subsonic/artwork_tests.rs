use super::test_support as pictures;
use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn cover_ids_are_distinct_from_entity_ids_and_omit_unsupported_or_escaping_paths() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (id, path) = pictures::install(&fixture, pictures::PNG).await;
        let mut catalog = fixture.0.catalog.read().await.as_ref().unwrap().clone();
        assert!(id.starts_with("cover-"));
        assert_ne!(
            id,
            super::super::opaque_id(&catalog, EntityKind::Release, 0).unwrap()
        );
        assert_eq!(source(&catalog, &id).unwrap().path, path);
        assert!(
            source(
                &catalog,
                &super::super::opaque_id(&catalog, EntityKind::Release, 0).unwrap()
            )
            .is_err()
        );
        for candidate in [
            None,
            Some("relative.png".into()),
            Some(
                fixture
                    .0
                    .data_dir
                    .join("outside.png")
                    .to_str()
                    .unwrap()
                    .into(),
            ),
            Some(
                fixture
                    .0
                    .data_dir
                    .join("album/../outside.png")
                    .to_str()
                    .unwrap()
                    .into(),
            ),
            Some(path.with_extension("webp").to_str().unwrap().into()),
        ] {
            catalog.releases[0].cover_path = candidate;
            assert!(cover_id(&catalog, &catalog.releases[0]).unwrap().is_none());
        }
    });
}

#[test]
fn auditors_get_verified_originals_and_response_only_thumbnails_without_writes() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let identity = pictures::identity(&fixture, "auditor").await;
        for (bytes, mime) in [(pictures::PNG, "image/png"), (pictures::JPEG, "image/jpeg")] {
            let (id, path) = pictures::install(&fixture, bytes).await;
            for size in [None, Some(1), Some(2048)] {
                let response = response(
                    fixture.0.clone(),
                    identity.clone(),
                    pictures::parameters(&id, size),
                )
                .await
                .unwrap();
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                assert_eq!(
                    response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                    "nosniff"
                );
                if size != Some(1) {
                    assert_eq!(response.headers()[header::CONTENT_TYPE], mime);
                }
                let length = response.headers()[header::CONTENT_LENGTH]
                    .to_str()
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                let received = axum::body::to_bytes(response.into_body(), MAX_BYTES as usize)
                    .await
                    .unwrap();
                assert_eq!(received.len(), length);
                if size == Some(1) {
                    assert_eq!(coverart::image_kind(&received), Some("png"));
                } else {
                    assert_eq!(&received[..], bytes);
                }
                coverart::render_image(&received, None).unwrap();
                assert_eq!(fs::read(&path).unwrap(), bytes);
            }
        }
        assert!(
            aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn invalid_oversized_and_linked_sources_never_become_image_responses() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (id, path) = pictures::install(&fixture, pictures::PNG).await;
        let source = source(fixture.0.catalog.read().await.as_ref().unwrap(), &id).unwrap();
        for invalid in [
            b"<html>private file</html>".as_slice(),
            &pictures::PNG[..pictures::PNG.len() - 1],
        ] {
            fs::write(&path, invalid).unwrap();
            assert_eq!(read_image(source.clone(), None).unwrap_err().code, 70);
        }
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(read_image(source.clone(), None).is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(read_image(source.clone(), None).is_err());
        fs::remove_dir(&path).unwrap();
        #[cfg(unix)]
        {
            let private = fixture.0.data_dir.join("outside.png");
            fs::write(&private, pictures::PNG).unwrap();
            std::os::unix::fs::symlink(&private, &path).unwrap();
            assert!(read_image(source, None).is_err());
            assert_eq!(fs::read(private).unwrap(), pictures::PNG);
        }
    });
}

#[test]
fn image_transfer_admission_and_revocation_keep_bounded_consumer_lifetimes() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (id, path) = pictures::install(&fixture, pictures::PNG).await;
        let identity = pictures::identity(&fixture, "alice").await;
        let permits = (0..crate::MAX_PLAYBACKS)
            .map(|_| {
                fixture
                    .0
                    .playback_slots
                    .clone()
                    .try_acquire_owned()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let busy = response(
            fixture.0.clone(),
            identity.clone(),
            pictures::parameters(&id, None),
        )
        .await
        .unwrap_err();
        assert_eq!(busy.code, 0);
        drop(permits);
        let response = response(
            fixture.0.clone(),
            identity.clone(),
            pictures::parameters(&id, None),
        )
        .await
        .unwrap();
        assert_eq!(
            fixture.0.playback_slots.available_permits(),
            crate::MAX_PLAYBACKS - 1
        );
        let mut accounts = fixture.accounts();
        accounts.revoke_api_key("alice", &identity.key_id).unwrap();
        aede_core::accounts::save(
            &accounts,
            &aede_core::accounts::accounts_path(&fixture.0.data_dir),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(
            axum::body::to_bytes(response.into_body(), MAX_BYTES as usize)
                .await
                .is_err()
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while fixture.0.playback_slots.available_permits() != crate::MAX_PLAYBACKS {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), pictures::PNG);
    });
}
