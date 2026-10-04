use super::*;

use crate::accounts_test_support::{Fixture, http, login};
use crate::playback_test_support::{ServerFrame, Socket, install_wav, upgrade, upgraded_socket};
use crate::test_support::{sample_state, start_server, test_runtime};
use std::io::ErrorKind;

fn send(socket: &mut Socket, frame: serde_json::Value) {
    socket.send_text(&frame.to_string());
}

fn start(socket: &mut Socket, tracks: &[&str], profile: Option<&str>) {
    let mut frame = serde_json::json!({
        "type":"start", "tracks":tracks, "interactive":true,
        "normalize":"off", "sample_rate":8000,
    });
    if let Some(profile) = profile {
        frame["profile"] = profile.into();
    }
    send(socket, frame);
}

fn next_text(socket: &mut Socket, kind: &str) -> serde_json::Value {
    match socket.next() {
        ServerFrame::Text(frame) => {
            assert_eq!(frame["type"], kind, "{frame}");
            frame
        }
        ServerFrame::Binary(_) => panic!("expected {kind}, received PCM"),
        ServerFrame::Close => panic!("closed before {kind}"),
    }
}

fn first_audio(socket: &mut Socket, epoch: u64) -> usize {
    loop {
        match socket.next() {
            ServerFrame::Binary(bytes) => return bytes.len() / 4,
            ServerFrame::Text(frame) => {
                assert_eq!(frame["epoch"], epoch, "{frame}");
                assert_eq!(frame["type"], "track", "{frame}");
            }
            ServerFrame::Close => panic!("closed before PCM"),
        }
    }
}

fn receive_audio(socket: &mut Socket, epoch: u64, minimum_frames: usize) {
    let mut received = 0;
    while received < minimum_frames {
        received += first_audio(socket, epoch);
    }
}

fn await_reset(socket: &mut Socket, epoch: u64) -> serde_json::Value {
    loop {
        match socket.next() {
            ServerFrame::Text(frame) if frame["type"] == "reset" => {
                assert_eq!(frame["epoch"], epoch, "{frame}");
                return frame;
            }
            // Old sent PCM and markers are discarded, never acknowledged as heard.
            ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
            ServerFrame::Binary(_) => {}
            ServerFrame::Close => panic!("closed before reset"),
        }
    }
}

fn finish(socket: &mut Socket, epoch: u64) -> Vec<serde_json::Value> {
    let mut frames = 0_u64;
    let mut records = Vec::new();
    loop {
        match socket.next() {
            ServerFrame::Binary(bytes) => {
                frames += bytes.len() as u64 / 4;
                send(
                    socket,
                    serde_json::json!({"type":"ack","epoch":epoch,"frames":frames}),
                );
            }
            ServerFrame::Text(frame) => match frame["type"].as_str().unwrap() {
                "track" | "track_end" | "format" => assert_eq!(frame["epoch"], epoch),
                "eof" => assert_eq!(frame["frames"], frames),
                "recorded" => records.push(frame),
                "saved" => {}
                _ => panic!("unexpected interactive frame: {frame}"),
            },
            ServerFrame::Close => return records,
        }
    }
}

fn stopped(socket: &mut Socket, epoch: u64, frames: u64) -> Vec<serde_json::Value> {
    send(
        socket,
        serde_json::json!({"type":"stop","epoch":epoch,"frames":frames}),
    );
    let mut records = Vec::new();
    loop {
        match socket.next() {
            ServerFrame::Text(frame) if frame["type"] == "recorded" => records.push(frame),
            ServerFrame::Text(frame) if frame["type"] == "saved" => {}
            ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
            ServerFrame::Binary(_) => {}
            ServerFrame::Close => return records,
        }
    }
}

fn private_data(fixture: &Fixture) -> UserData {
    user::load(&user::user_path(&fixture.0.data_dir))
        .unwrap()
        .unwrap_or_default()
}

fn owner(fixture: &Fixture, name: &str) -> String {
    fixture.accounts().find(name).unwrap().id.clone()
}

#[test]
fn seek_resets_buffers_only_after_client_confirmation_and_counts_only_consumed_audio() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &[&installed.reference], None);
        let queue = next_text(&mut socket, "queue");
        assert_eq!(queue["epoch"], 0);
        assert_eq!(queue["items"][0]["occurrence"], 1);
        assert_eq!(next_text(&mut socket, "format")["position_ms"], 0);
        assert!(first_audio(&mut socket, 0) >= 80);
        send(
            &mut socket,
            serde_json::json!({
                "type":"seek","epoch":0,"frames":80,"position_ms":1250,
            }),
        );
        let reset = await_reset(&mut socket, 1);
        assert_eq!(reset["position_ms"], 1250);
        assert_eq!(reset["current_occurrence"], 1);
        assert!(
            socket.buffered.is_empty(),
            "no new PCM may follow an unconfirmed reset"
        );
        socket
            .stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut byte = [0_u8];
        assert!(matches!(socket.stream.peek(&mut byte), Err(error)
            if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)));
        socket
            .stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        send(
            &mut socket,
            serde_json::json!({"type":"reset_ack","epoch":1}),
        );
        let format = next_text(&mut socket, "format");
        assert_eq!(format["epoch"], 1);
        assert_eq!(format["position_ms"], 1250);
        let records = finish(&mut socket, 1);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["occurrence"], 1);
        assert_eq!(records[0]["ms_played"], 1760);
        assert_eq!(records[0]["completed"], false);
        let data = private_data(&fixture);
        assert_eq!(data.plays.len(), 1, "seeks retain one visit");
        assert_eq!(data.plays[0].ms_played, 1760);
        assert!(!data.plays[0].completed);
        server.abort();
    });
}

#[test]
fn future_queue_edit_discards_buffered_removed_tracks_and_preserves_stable_occurrences() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 4_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(
            &mut socket,
            &[
                &installed.reference,
                &installed.reference,
                &installed.reference,
                &installed.reference,
            ],
            None,
        );
        let queue = next_text(&mut socket, "queue");
        next_text(&mut socket, "format");
        assert!(first_audio(&mut socket, 0) >= 800);
        send(
            &mut socket,
            serde_json::json!({
                "type":"edit_queue", "epoch":0, "frames":800, "revision":queue["revision"],
                "items":[{"occurrence":4},{"occurrence":3},{"track":installed.reference}],
            }),
        );
        let reset = await_reset(&mut socket, 1);
        assert_eq!(reset["position_ms"], 100);
        let ids: Vec<_> = reset["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["occurrence"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [1, 4, 3, 5]);
        assert_eq!(reset["revision"], queue["revision"].as_u64().unwrap() + 1);
        send(
            &mut socket,
            serde_json::json!({"type":"reset_ack","epoch":1}),
        );
        assert_eq!(next_text(&mut socket, "format")["position_ms"], 100);
        let records = finish(&mut socket, 1);
        let ids: Vec<_> = records
            .iter()
            .map(|frame| frame["occurrence"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [1, 4, 3, 5]);
        assert!(
            records
                .iter()
                .all(|frame| frame["ms_played"] == 500 && frame["completed"] == true)
        );
        let data = private_data(&fixture);
        assert_eq!(data.plays.len(), 4);
        assert!(
            data.plays
                .iter()
                .all(|listen| listen.ms_played == 500 && listen.completed)
        );
        assert_eq!(
            data.counts[0].count, 4,
            "removed buffered occurrence never became a listen"
        );
        server.abort();
    });
}

#[test]
fn named_resume_survives_server_state_recreation_and_stays_private_to_its_owner() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(
            &mut socket,
            &[&installed.reference, &installed.reference],
            Some("desktop"),
        );
        next_text(&mut socket, "queue");
        next_text(&mut socket, "format");
        assert!(first_audio(&mut socket, 0) >= 800);
        assert_eq!(stopped(&mut socket, 0, 800)[0]["ms_played"], 100);
        let alice = owner(&fixture, "alice");
        let saved = private_data(&fixture)
            .playback_state(&alice, "desktop")
            .unwrap()
            .clone();
        assert_eq!(saved.position_ms, 100);
        assert_eq!(saved.entries.len(), 2);
        assert_eq!(saved.settings.normalize, Mode::Off);
        assert_eq!(saved.settings.sample_rate, Some(8_000));
        server.abort();
        let mut restarted = sample_state();
        restarted.data_dir = fixture.0.data_dir.clone();
        *restarted.catalog.write().await =
            store::load(&store::catalog_path(&restarted.data_dir)).unwrap();
        let (address, server) = start_server(restarted).await;
        let token = login(address, "bob");
        let mut foreign = upgraded_socket(address, &token);
        send(
            &mut foreign,
            serde_json::json!({"type":"resume","profile":"desktop"}),
        );
        assert_eq!(next_text(&mut foreign, "error")["code"], "state_failed");
        let auditor = login(address, "auditor");
        let (_, response, _) = upgrade(address, &auditor);
        assert!(response.starts_with("HTTP/1.1 403"));
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        send(
            &mut socket,
            serde_json::json!({"type":"resume","profile":"desktop"}),
        );
        let queue = next_text(&mut socket, "queue");
        assert_eq!(queue["position_ms"], 100);
        assert_eq!(queue["revision"], saved.revision + 1);
        assert_eq!(next_text(&mut socket, "format")["position_ms"], 100);
        receive_audio(&mut socket, 0, 400);
        let records = stopped(&mut socket, 0, 400);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["ms_played"], 50);
        assert_eq!(records[0]["completed"], false);
        let data = private_data(&fixture);
        assert_eq!(
            data.playback_state(&alice, "desktop").unwrap().position_ms,
            150
        );
        assert_eq!(
            data.plays.len(),
            2,
            "resuming does not submit the previous listen twice"
        );
        assert_eq!(
            data.plays
                .iter()
                .map(|listen| listen.ms_played)
                .sum::<u64>(),
            150
        );
        assert!(data.plays.iter().all(|listen| listen.owner == alice));
        server.abort();
    });
}

#[test]
fn stale_resume_identity_and_duplicate_live_profile_are_refused_without_overwriting_checkpoint() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &[&installed.reference], Some("desktop"));
        next_text(&mut socket, "queue");
        next_text(&mut socket, "format");
        first_audio(&mut socket, 0);
        let mut duplicate = upgraded_socket(address, &token);
        send(
            &mut duplicate,
            serde_json::json!({"type":"resume","profile":"desktop"}),
        );
        assert_eq!(next_text(&mut duplicate, "error")["code"], "state_conflict");
        stopped(&mut socket, 0, 800);
        let alice = owner(&fixture, "alice");
        let before = private_data(&fixture)
            .playback_state(&alice, "desktop")
            .unwrap()
            .clone();
        std::fs::write(&installed.path, [0_u8; 12]).unwrap();
        let metadata = std::fs::metadata(&installed.path).unwrap();
        {
            let mut guard = fixture.0.catalog.write().await;
            let catalog = guard.as_mut().unwrap();
            let file = &mut catalog.files[0];
            file.size = metadata.len();
            file.mtime = clock::mtime_seconds(&metadata);
            catalog
                .file_mtime_subseconds
                .insert(file.path.clone(), clock::mtime_subseconds(&metadata));
            store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
        }
        let mut socket = upgraded_socket(address, &token);
        send(
            &mut socket,
            serde_json::json!({"type":"resume","profile":"desktop"}),
        );
        assert_eq!(next_text(&mut socket, "error")["code"], "source_changed");
        let after = private_data(&fixture);
        assert_eq!(after.playback_state(&alice, "desktop"), Some(&before));
        assert_eq!(after.plays.len(), 1);
        server.abort();
    });
}

#[test]
fn revoked_session_cannot_publish_a_later_checkpoint_or_listen() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &[&installed.reference], Some("desktop"));
        next_text(&mut socket, "queue");
        next_text(&mut socket, "format");
        receive_audio(&mut socket, 0, 1600);
        assert_eq!(
            private_data(&fixture)
                .playback_state(&owner(&fixture, "alice"), "desktop")
                .unwrap()
                .position_ms,
            0,
            "receipt and buffering never advance the saved cursor"
        );
        tokio::time::sleep(Duration::from_millis(5_200)).await;
        send(
            &mut socket,
            serde_json::json!({"type":"ack","epoch":0,"frames":800}),
        );
        let alice = owner(&fixture, "alice");
        let mut checkpointed = false;
        for _ in 0..120 {
            if private_data(&fixture)
                .playback_state(&alice, "desktop")
                .is_some_and(|state| state.position_ms == 100)
            {
                checkpointed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(checkpointed, "growing ACK is a durable bounded checkpoint");
        let before = private_data(&fixture)
            .playback_state(&alice, "desktop")
            .unwrap()
            .clone();
        assert_eq!(
            http(address, "DELETE", "/api/auth/v1/session", Some(&token), "").0,
            204
        );
        send(
            &mut socket,
            serde_json::json!({"type":"stop","epoch":0,"frames":1600}),
        );
        loop {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "authentication_expired");
                    break;
                }
                ServerFrame::Text(_) | ServerFrame::Binary(_) => {}
                ServerFrame::Close => panic!("closed without authorization error"),
            }
        }
        let after = private_data(&fixture);
        assert_eq!(after.playback_state(&alice, "desktop"), Some(&before));
        assert!(
            after.plays.is_empty(),
            "revocation blocks pending batch publication"
        );
        server.abort();
    });
}

#[test]
fn abandoned_or_invalid_seek_keeps_the_last_consumed_resume_cursor() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let alice = owner(&fixture, "alice");
        for (confirm, profile) in [(false, "abandoned"), (true, "invalid")] {
            let mut socket = upgraded_socket(address, &token);
            start(&mut socket, &[&installed.reference], Some(profile));
            next_text(&mut socket, "queue");
            next_text(&mut socket, "format");
            receive_audio(&mut socket, 0, 80);
            send(
                &mut socket,
                serde_json::json!({
                    "type":"seek","epoch":0,"frames":80,"position_ms":9000,
                }),
            );
            let reset = await_reset(&mut socket, 1);
            assert_eq!(reset["position_ms"], 9000);
            if confirm {
                send(
                    &mut socket,
                    serde_json::json!({"type":"reset_ack","epoch":1}),
                );
                let recorded = next_text(&mut socket, "recorded");
                assert_eq!(recorded["ms_played"], 10);
                assert_eq!(recorded["completed"], false);
                assert_eq!(next_text(&mut socket, "error")["code"], "invalid_seek");
            } else {
                socket.close();
            }
            drop(socket);
            let mut saved = None;
            for _ in 0..120 {
                let data = private_data(&fixture);
                if data.plays.len() == if confirm { 2 } else { 1 } {
                    saved = data.playback_state(&alice, profile).cloned();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let saved =
                saved.expect("failed seeks retain the confirmed bookmark and partial listen");
            assert_eq!(
                saved.position_ms, 10,
                "requested seek is not a verified cursor"
            );
            assert_eq!(saved.current_occurrence, Some(1));
            assert_eq!(saved.revision, reset["revision"].as_u64().unwrap());
        }
        server.abort();
    });
}

#[test]
fn previous_epoch_ack_cannot_complete_a_new_buffer_reset() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 24_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start(&mut socket, &[&installed.reference], None);
        next_text(&mut socket, "queue");
        next_text(&mut socket, "format");
        first_audio(&mut socket, 0);
        send(
            &mut socket,
            serde_json::json!({"type":"seek","epoch":0,"frames":80,"position_ms":500}),
        );
        await_reset(&mut socket, 1);
        send(
            &mut socket,
            serde_json::json!({"type":"reset_ack","epoch":0}),
        );
        let frame = next_text(&mut socket, "recorded");
        assert_eq!(frame["ms_played"], 10);
        assert_eq!(next_text(&mut socket, "error")["code"], "invalid_control");
        server.abort();
    });
}

#[test]
fn malformed_controls_and_start_profiles_cannot_relax_the_public_contract() {
    for input in [
        r#"{"type":"ack","epoch":0,"frames":1,"frames":2}"#,
        r#"{"type":"ack","epoch":0,"frames":1,"extra":true}"#,
        r#"{"type":"seek","epoch":0,"frames":1,"position_ms":-1}"#,
        r#"{"type":"edit_queue","epoch":0,"frames":1,"revision":1,"items":[{"occurrence":2,"extra":true}]}"#,
        r#"{"type":"stop","epoch":null,"frames":0}"#,
        r#"{"type":"pause","epoch":0,"frames":0}"#,
    ] {
        assert!(
            serde_json::from_str::<Frame>(input).is_err(),
            "accepted {input}"
        );
    }
    assert!(
        Start::parse(r#"{"type":"start","track":"track:sample","profile":"desktop"}"#).is_err()
    );
    assert!(Start::parse(r#"{"type":"resume","profile":"desktop","sample_rate":48000}"#).is_err());
}

#[test]
fn invalid_future_edits_leave_queue_identity_and_next_occurrence_counter_unchanged() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 8).await;
        let reference = EntityRef::parse_token(&installed.reference).unwrap();
        let (sources, _) = current_sources(&fixture.0, &[reference]).await.unwrap();
        let original = PlaybackState {
            owner: "local".into(),
            profile: "desktop".into(),
            session_id: "test_session".into(),
            revision: 1,
            entries: (1..=3).map(|id| entry(&sources[0], id)).collect(),
            current_occurrence: Some(1),
            position_ms: 0,
            settings: PlaybackSettings::default(),
            updated_at: 0,
        };
        for (revision, items) in [
            (
                2,
                vec![Item {
                    occurrence: Some(2),
                    track: None,
                }],
            ),
            (
                1,
                vec![Item {
                    occurrence: Some(1),
                    track: None,
                }],
            ),
            (
                1,
                vec![
                    Item {
                        occurrence: Some(2),
                        track: None,
                    },
                    Item {
                        occurrence: Some(2),
                        track: None,
                    },
                ],
            ),
            (
                1,
                vec![Item {
                    occurrence: Some(2),
                    track: Some(installed.reference.clone()),
                }],
            ),
            (
                1,
                vec![Item {
                    occurrence: None,
                    track: None,
                }],
            ),
            (
                1,
                vec![Item {
                    occurrence: None,
                    track: Some("artist:sample".into()),
                }],
            ),
            (
                1,
                (0..MAX_QUEUE_TRACKS)
                    .map(|_| Item {
                        occurrence: None,
                        track: Some(installed.reference.clone()),
                    })
                    .collect(),
            ),
        ] {
            let mut snapshot = original.clone();
            let mut next_id = 3;
            assert_eq!(
                edit(&fixture.0, &mut snapshot, revision, items, &mut next_id)
                    .await
                    .unwrap_err()
                    .code,
                "invalid_control"
            );
            assert_eq!(snapshot, original);
            assert_eq!(next_id, 3);
        }
        let mut snapshot = original.clone();
        let mut next_id = MAX_EXACT_INTEGER;
        assert!(
            edit(
                &fixture.0,
                &mut snapshot,
                1,
                vec![Item {
                    occurrence: None,
                    track: Some(installed.reference)
                }],
                &mut next_id
            )
            .await
            .is_err()
        );
        assert_eq!(snapshot, original);
        assert_eq!(next_id, MAX_EXACT_INTEGER);
        let mut finished = original.clone();
        finished.current_occurrence = None;
        let mut next_id = 3;
        edit(
            &fixture.0,
            &mut finished,
            1,
            vec![Item {
                occurrence: None,
                track: Some(original.entries[0].track.to_token()),
            }],
            &mut next_id,
        )
        .await
        .unwrap();
        assert_eq!(
            finished.current_occurrence,
            Some(4),
            "an appended track after the consumed last boundary must actually become playable"
        );
        assert_eq!(finished.entries.len(), 4);
        assert_eq!(next_id, 4);
    });
}

#[test]
fn short_acknowledged_segments_accumulate_before_one_duration_rounding() {
    let source = TrackSource {
        reference: EntityRef::new(EntityKind::Track, "sample.wav"),
        path: PathBuf::from("sample.wav"),
        file: AudioFile::default(),
        mtime_subseconds: 0,
    };
    let entries = vec![entry(&source, 1)];
    let snapshot = PlaybackState {
        owner: "local".into(),
        profile: "desktop".into(),
        session_id: "test".into(),
        revision: 1,
        entries: entries.clone(),
        current_occurrence: Some(1),
        position_ms: 0,
        settings: PlaybackSettings::default(),
        updated_at: 0,
    };
    let epoch = Epoch {
        number: 0,
        entries,
        base: snapshot,
        sender: None,
        failed: Arc::new(AtomicBool::new(false)),
    };
    let mut visits = BTreeMap::new();
    let sources = vec![source];
    for segment in 1..=8 {
        let mut timeline = ListeningTimeline::default();
        timeline
            .begin(
                &TrackFrame {
                    kind: "track",
                    index: 0,
                    track: "track:sample.wav".into(),
                    start_frame: 0,
                    duration_ms: None,
                    position_ms: Some(0),
                },
                0,
                8000,
                &sources,
            )
            .unwrap();
        assert!(
            timeline.listens(1, &sources).is_empty(),
            "legacy sub-millisecond contract remains unchanged"
        );
        accumulate(
            &mut visits,
            &epoch,
            &timeline,
            1,
            &sources,
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(visits[&1].listen.ms_played, u64::from(segment == 8));
        assert_eq!(visits[&1].frames, segment);
    }
    assert_eq!(visits.len(), 1);
    assert!(!visits[&1].listen.completed);
}
