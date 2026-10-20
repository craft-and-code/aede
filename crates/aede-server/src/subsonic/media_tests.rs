use super::*;

#[test]
fn shutdown_during_catalogue_preparation_cannot_start_an_original_transfer() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut late = Vec::new();
        for bearer in [true, false] {
            let fixture = crate::accounts_test_support::Fixture::new();
            let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
            let identity = if bearer {
                let accounts = fixture.accounts();
                let (principal, _) = crate::auth::start_session(
                    &fixture.0,
                    &accounts,
                    accounts.find("alice").unwrap(),
                )
                .unwrap();
                MediaIdentity::Session(principal)
            } else {
                let token = crate::subsonic::test_support::key(&fixture, "alice");
                let identity = authentication::authenticate(
                    &fixture.0,
                    &super::super::Parameters::from_pairs(&[("apiKey", &token)]),
                )
                .await
                .unwrap();
                MediaIdentity::Key(identity)
            };
            let _shutdown_observer = fixture.0.shutdown.subscribe();
            let catalog = fixture.0.catalog.write().await;
            let mut response = Box::pin(response_for(
                fixture.0.clone(),
                identity,
                EntityRef::parse_token(&installed.reference).unwrap(),
                HeaderMap::new(),
                true,
                None,
            ));
            let (release, worker) = crate::test_support::hold_blocking_worker().await;
            std::future::poll_fn(|context| {
                assert!(response.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
            // Complete admission, then leave the transfer waiting for the catalogue.
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
            std::future::poll_fn(|context| {
                assert!(response.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
            fixture.0.shutdown.send(()).unwrap();
            drop(catalog);
            if let Ok(response) = response.await {
                let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
                    .await
                    .unwrap();
                late.push((bearer, bytes.len()));
            }
            tokio::time::timeout(Duration::from_secs(1), async {
                while fixture.0.playback_slots.available_permits() != crate::MAX_PLAYBACKS {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
        }
        assert!(
            late.is_empty(),
            "transfers started after server shutdown: {late:?}"
        );
    });
}

#[test]
fn cancelled_export_confirmations_retain_bounded_worker_admission() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let accounts = fixture.accounts();
        let (principal, _) =
            crate::auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap())
                .unwrap();
        let identity = MediaIdentity::Session(principal);
        let capacity = fixture.0.parental.read_slots.available_permits();
        let (release, worker) = crate::test_support::hold_blocking_worker().await;
        let mut pending = Vec::new();
        for _ in 0..capacity {
            let mut confirmation = Box::pin(identity.recheck(&fixture.0));
            std::future::poll_fn(|context| {
                assert!(confirmation.as_mut().poll(context).is_pending());
                Poll::Ready(())
            })
            .await;
            pending.push(confirmation);
        }
        // Retiring HTTP waiters must not retire their queued filesystem reads.
        drop(pending);
        let retained = fixture.0.parental.read_slots.available_permits();
        let excess =
            tokio::time::timeout(Duration::from_millis(100), identity.recheck(&fixture.0)).await;
        drop(release);
        worker.await.unwrap();
        tokio::task::spawn_blocking(|| ()).await.unwrap();
        assert!(
            matches!(excess, Ok(Err(_))),
            "excess confirmations must be refused instead of queueing more workers"
        );
        assert_eq!(retained, 0, "cancelled reads must retain their admission");
        assert_eq!(fixture.0.parental.read_slots.available_permits(), capacity);
        assert!(identity.recheck(&fixture.0).await.is_ok());
        assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
    });
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
