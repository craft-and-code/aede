use super::*;

#[test]
fn byte_ranges_select_exact_original_regions_and_refuse_multiple_or_invalid_ranges() {
    for (value, start, length) in [
        ("bytes=0-0", 0, 1),
        ("bytes=2-4", 2, 3),
        ("bytes=7-99", 7, 3),
        ("bytes=4-", 4, 6),
        ("bytes=-3", 7, 3),
        ("bytes=-99", 0, 10),
    ] {
        assert_eq!(
            parse_range(value, 10),
            Some(ByteRange {
                start,
                length,
                partial: true
            }),
            "{value}"
        );
    }
    for value in [
        "bytes=",
        "bytes=10-",
        "bytes=-0",
        "bytes=4-2",
        "bytes=0-1,3-4",
        "bytes=0-18446744073709551616",
        "bytes=+1-2",
        "bytes=1 -2",
        "items=0-1",
    ] {
        assert!(parse_range(value, 10).is_none(), "{value}");
    }
    assert!(parse_range("bytes=0-", 0).is_none());
    assert_eq!(
        parse_range("bytes=0-18446744073709551615", u64::MAX)
            .unwrap()
            .length,
        u64::MAX
    );
}

#[test]
fn unknown_if_range_returns_the_whole_entity_and_duplicate_headers_are_refused() {
    let mut headers = HeaderMap::new();
    headers.insert(header::RANGE, "bytes=3-5".parse().unwrap());
    assert_eq!(requested_range(&headers, 10).unwrap().unwrap().length, 3);
    headers.insert(header::IF_RANGE, "\"earlier-entity\"".parse().unwrap());
    assert_eq!(
        requested_range(&headers, 10).unwrap(),
        Some(ByteRange {
            start: 0,
            length: 10,
            partial: false
        })
    );
    headers.append(header::IF_RANGE, "\"other-entity\"".parse().unwrap());
    assert!(requested_range(&headers, 10).is_err());
    headers.remove(header::IF_RANGE);
    headers.append(header::RANGE, "bytes=0-1".parse().unwrap());
    assert!(requested_range(&headers, 10).is_err());
}

#[test]
fn media_types_describe_original_containers_instead_of_requesting_transcoding() {
    for (container, codec, expected_suffix, expected_type) in [
        ("wav", "pcm", "wav", "audio/wav"),
        ("mp4", "alac", "m4a", "audio/mp4"),
        ("ogg", "opus", "ogg", "audio/ogg"),
        ("", "flac", "flac", "audio/flac"),
        ("unknown", "unknown", "bin", "application/octet-stream"),
    ] {
        let properties = AudioProperties {
            container: container.into(),
            codec: codec.into(),
            ..AudioProperties::default()
        };
        assert_eq!(suffix(&properties), expected_suffix);
        assert_eq!(content_type(&properties), expected_type);
    }
}

#[test]
fn revocation_and_shutdown_stop_full_queues_without_releasing_the_slot_early() {
    crate::test_support::test_runtime().block_on(async {
        for (revoke, frames) in [(true, 400), (true, 131_072), (false, 131_072)] {
            let fixture = crate::accounts_test_support::Fixture::new();
            let installed = crate::playback_test_support::install_wav(&fixture, frames).await;
            let token = crate::subsonic::test_support::key(&fixture, "alice");
            let accounts = fixture.accounts();
            let account = accounts.find("alice").unwrap();
            let identity = Identity {
                owner: account.id.clone(),
                username: account.username.clone(),
                role: account.role,
                key_id: token.split_once('.').unwrap().0.into(),
                epoch: accounts.epoch().into(),
                revision: account.revision(),
            };
            let source = {
                let catalog = fixture.0.catalog.read().await;
                source_from_catalog(
                    catalog.as_ref().unwrap(),
                    &EntityRef::parse_token(&installed.reference).unwrap(),
                )
                .unwrap()
            };
            let size = source.file.size;
            let queued = usize::try_from(size.div_ceil(CHUNK_BYTES as u64))
                .unwrap()
                .min(QUEUED_CHUNKS);
            let before = std::fs::read(&source.path).unwrap();
            let permit = fixture
                .0
                .playback_slots
                .clone()
                .try_acquire_owned()
                .unwrap();
            let opened = open_source(source, permit).unwrap();
            let body = original_body(
                opened,
                ByteRange {
                    start: 0,
                    length: size,
                    partial: false,
                },
                fixture.0.clone(),
                identity.clone(),
                fixture.0.shutdown.subscribe(),
            );
            tokio::time::timeout(Duration::from_secs(1), async {
                while body.receiver.len() != queued {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the bounded HTTP queue must fill while its consumer pauses");
            assert_eq!(
                fixture.0.playback_slots.available_permits(),
                crate::MAX_PLAYBACKS - 1
            );
            if revoke {
                let mut accounts = fixture.accounts();
                accounts.revoke_api_key("alice", &identity.key_id).unwrap();
                aede_core::accounts::save(
                    &accounts,
                    &aede_core::accounts::accounts_path(&fixture.0.data_dir),
                )
                .unwrap();
            } else {
                fixture.0.shutdown.send(()).unwrap();
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                while !body.cancelled.load(Ordering::Acquire) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("revocation and shutdown must be observed even with a full queue");
            assert_eq!(
                fixture.0.playback_slots.available_permits(),
                crate::MAX_PLAYBACKS - 1,
                "buffered response lifetime keeps its admission slot"
            );
            assert!(
                axum::body::to_bytes(Body::new(body), 1_000_000)
                    .await
                    .is_err(),
                "queued original bytes must not be released after access stops"
            );
            tokio::time::timeout(Duration::from_secs(2), async {
                while fixture.0.playback_slots.available_permits() != crate::MAX_PLAYBACKS {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("the cancelled producer must release its slot after it exits");
            assert_eq!(std::fs::read(&installed.path).unwrap(), before);
            assert!(
                aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
                    .unwrap()
                    .is_none()
            );
        }
    });
}

#[cfg(unix)]
#[test]
fn replacing_the_path_during_a_transfer_is_detected_even_with_matching_size_and_mtime() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
        let source = {
            let catalog = fixture.0.catalog.read().await;
            source_from_catalog(
                catalog.as_ref().unwrap(),
                &EntityRef::parse_token(&installed.reference).unwrap(),
            )
            .unwrap()
        };
        let size = source.file.size;
        let permit = fixture
            .0
            .playback_slots
            .clone()
            .try_acquire_owned()
            .unwrap();
        let mut opened = open_source(source, permit).unwrap();
        let bytes = std::fs::read(&installed.path).unwrap();
        let modified = opened.file.metadata().unwrap().modified().unwrap();
        std::fs::rename(&installed.path, fixture.0.data_dir.join("original.wav")).unwrap();
        std::fs::write(&installed.path, bytes).unwrap();
        File::options()
            .write(true)
            .open(&installed.path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(validate_source(&opened.source).is_ok());
        let (sender, mut receiver) = mpsc::channel(1);
        let result = tokio::task::spawn_blocking(move || {
            read_original(
                &mut opened,
                ByteRange {
                    start: 0,
                    length: size,
                    partial: false,
                },
                sender,
                Arc::new(AtomicBool::new(false)),
            )
        })
        .await
        .unwrap();
        assert!(
            result.is_err(),
            "the original descriptor must still identify the catalogued path"
        );
        assert!(receiver.recv().await.is_none());
    });
}

#[test]
fn opening_an_original_refuses_changed_files_and_source_links() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
        let source = {
            let catalog = fixture.0.catalog.read().await;
            source_from_catalog(
                catalog.as_ref().unwrap(),
                &EntityRef::parse_token(&installed.reference).unwrap(),
            )
            .unwrap()
        };
        let reserve = || {
            fixture
                .0
                .playback_slots
                .clone()
                .try_acquire_owned()
                .unwrap()
        };
        assert!(open_source(source.clone(), reserve()).is_ok());
        let original = std::fs::read(&installed.path).unwrap();
        std::fs::write(&installed.path, b"changed").unwrap();
        assert!(open_source(source.clone(), reserve()).is_err());
        #[cfg(unix)]
        {
            let replacement = fixture.0.data_dir.join("replacement.wav");
            std::fs::write(&replacement, original).unwrap();
            std::fs::remove_file(&installed.path).unwrap();
            std::os::unix::fs::symlink(&replacement, &installed.path).unwrap();
            assert!(open_source(source, reserve()).is_err());
        }
        #[cfg(not(unix))]
        let _ = original;
        assert_eq!(
            fixture.0.playback_slots.available_permits(),
            crate::MAX_PLAYBACKS
        );
    });
}
