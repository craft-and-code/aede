//! Original-bit WebSocket transport, independent of device output or PCM decoding.
use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::playback_test_support::{InstalledTrack, ServerFrame, install_wav, upgraded_socket};
use crate::test_support::{start_server, test_runtime};

async fn install_dsd(
    fixture: &Fixture,
    name: &str,
    bytes: &[u8],
    additional: bool,
) -> InstalledTrack {
    let path = fixture.0.data_dir.join(name);
    fs::write(&path, bytes).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    let properties = aede_core::tags::read(&path).unwrap().properties;
    let text = path.to_string_lossy().into_owned();
    let mut held = fixture.0.catalog.write().await;
    let catalog = held.as_mut().unwrap();
    let id = if additional {
        let mut file = catalog.files[0].clone();
        file.id = catalog.files.len() as u32;
        let mut track = catalog.tracks[0].clone();
        track.id = catalog.tracks.len() as u32;
        track.file_id = file.id;
        if let Some(release) = track.release_id {
            catalog.releases[release as usize].track_ids.push(track.id);
        }
        catalog.recordings[track.recording_id as usize]
            .track_ids
            .push(track.id);
        let id = track.id;
        catalog.files.push(file);
        catalog.tracks.push(track);
        id
    } else {
        0
    };
    let file_id = catalog.tracks[id as usize].file_id;
    let file = &mut catalog.files[file_id as usize];
    let old = std::mem::replace(&mut file.path, text.clone());
    file.size = metadata.len();
    file.mtime = clock::mtime_seconds(&metadata);
    file.properties = properties;
    if !additional {
        catalog.file_mtime_subseconds.remove(&old);
    }
    catalog
        .file_mtime_subseconds
        .insert(text, clock::mtime_subseconds(&metadata));
    let reference = EntityRef::of(catalog, EntityKind::Track, id)
        .unwrap()
        .to_token();
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
    InstalledTrack { reference, path }
}

#[test]
fn original_dsd_transport_is_explicit_and_refuses_pcm_processing_or_saved_profiles() {
    let request = serde_json::json!({"type":"start","track":"track:source","normalize":"off","dsd_output":"dop"});
    assert!(Start::parse(&request.to_string()).is_ok());
    for (key, value) in [
        ("normalize", serde_json::json!("track")),
        ("normalize", serde_json::json!("album")),
        ("sample_rate", serde_json::json!(176400)),
        ("bass", serde_json::json!(1)),
        ("treble", serde_json::json!(-1)),
        ("profile", serde_json::json!("retained")),
        ("dsd_output", serde_json::json!("native")),
        ("dsd_output", serde_json::Value::Null),
    ] {
        let mut changed = request.clone();
        changed[key] = value;
        assert!(
            Start::parse(&changed.to_string()).is_err(),
            "{key}: {changed}"
        );
    }
    let mut profile = request.clone();
    profile["interactive"] = true.into();
    profile["profile"] = "retained".into();
    assert!(Start::parse(&profile.to_string()).is_err());
    assert!(Start::parse(r#"{"type":"resume","profile":"retained","dsd_output":"dop"}"#).is_err());
    assert!(Start::parse(r#"{"type":"start","track":"track:source"}"#).is_ok());
}

#[test]
fn original_dsd_words_and_exact_source_facts_cross_the_authenticated_transport() {
    test_runtime().block_on(async {
        let mut partial = include_bytes!("../../aede-audio/tests/fixtures/tone.dsf").to_vec();
        // Carrier padding crosses23ms; only22wholemilliseconds are original.
        partial[64..72].copy_from_slice(&64_913u64.to_le_bytes());
        for (name, source) in [
            ("tone.dsf", include_bytes!("../../aede-audio/tests/fixtures/tone.dsf").as_slice()),
            ("tone.dff", include_bytes!("../../aede-audio/tests/fixtures/tone.dff").as_slice()),
            ("stereo128.dsf", include_bytes!("../../aede-audio/tests/fixtures/stereo128.dsf").as_slice()),
            ("stereo128.dff", include_bytes!("../../aede-audio/tests/fixtures/stereo128.dff").as_slice()),
            ("partial-tail.dsf", partial.as_slice()),
        ] {
            let fixture = Fixture::new();
            let installed = install_dsd(&fixture, name, source, false).await;
            let mut reader = aede_audio::dop::Reader::open(&installed.path).unwrap();
            let info = reader.info();
            let mut expected = Vec::new();
            while let Some(block) = reader.next_chunk(|| Ok(())).unwrap() {
                for word in block.payload {
                    expected.extend_from_slice(&word.0.to_le_bytes());
                }
            }
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = upgraded_socket(address, &token);
            socket.send_text(&serde_json::json!({"type":"start","track":installed.reference,"normalize":"off","dsd_output":"dop"}).to_string());
            let ServerFrame::Text(format) = socket.next() else { panic!("format") };
            assert_eq!(format["type"], "format", "{format}");
            assert_eq!(format["encoding"], "dsd16le");
            assert_eq!(format["source_rate"], info.sample_rate);
            assert_eq!(format["source_samples"], info.samples);
            assert_eq!(format["position_samples"], 0);
            assert_eq!(format["sample_rate"], info.sample_rate / 16);
            assert_eq!(format["channels"], info.channels);
            let mut captured = Vec::new();
            let mut frames = 0u64;
            let mut saw_eof = false;
            loop {
                match socket.next() {
                    ServerFrame::Binary(bytes) => {
                        assert!(bytes.len() <= MAX_AUDIO_BYTES);
                        assert!(bytes.len().is_multiple_of(2 * info.channels));
                        frames += (bytes.len() / (2 * info.channels)) as u64;
                        captured.extend_from_slice(&bytes);
                        socket.send_text(&serde_json::json!({"type":"ack","frames":frames}).to_string());
                    }
                    ServerFrame::Text(value) if value["type"] == "eof" => {
                        assert_eq!(value["frames"], info.samples.div_ceil(16));
                        saw_eof = true;
                    }
                    ServerFrame::Text(value) if value["type"] == "recorded" => {
                        assert!(saw_eof);
                        assert_eq!(value["ms_played"], info.samples * 1000 / u64::from(info.sample_rate));
                        assert_eq!(value["completed"], true);
                        break;
                    }
                    ServerFrame::Text(value) => panic!("unexpected {value}"),
                    ServerFrame::Close => panic!("unexpected close"),
                }
            }
            assert_eq!(captured, expected, "{name}");
            let events = dsd_producer_events(
                &fixture, &[&installed.reference], Some(info.samples), false,
            ).await;
            assert!(events.iter().all(|event| !matches!(event, ProducerEvent::Audio {..} | ProducerEvent::Error(_))));
            assert!(events.iter().any(|event| matches!(event, ProducerEvent::Eof {frames: 0})));
            assert_eq!(fs::read(&installed.path).unwrap(), source);
            server.abort();
        }
    });
}

#[test]
fn original_dsd_mode_never_falls_back_to_pcm_for_an_ordinary_track() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 400).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        socket.send_text(&serde_json::json!({"type":"start","track":installed.reference,"normalize":"off","dsd_output":"dop"}).to_string());
        let ServerFrame::Text(failure) = socket.next() else { panic!("explicit refusal") };
        assert_eq!(failure["type"], "error");
        assert_eq!(failure["code"], "unsupported_dsd_output");
        server.abort();
    });
}

async fn dsd_producer_events(
    fixture: &Fixture,
    references: &[&str],
    position_samples: Option<u64>,
    cancelled: bool,
) -> Vec<ProducerEvent> {
    let start = Start::parse(&serde_json::json!({"type":"start","tracks":references,"normalize":"off","dsd_output":"dop","interactive":true}).to_string()).unwrap();
    let (sources, catalog) = current_sources(&fixture.0, &start.references)
        .await
        .unwrap();
    let mut options = settings(catalog, fixture.0.data_dir.clone(), &start);
    options.first_position_samples = position_samples;
    let (mut receive, worker) =
        spawn_producer(sources, options, Arc::new(AtomicBool::new(cancelled)));
    let mut events = Vec::new();
    while let Some(event) = receive.recv().await {
        events.push(event);
    }
    worker.await.unwrap();
    events
}

#[test]
fn original_dsd_queue_preflight_refuses_mixed_clocks_and_cancellation_before_publication() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_dsd(
            &fixture,
            "first.dsf",
            include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
            false,
        )
        .await;
        let next = install_dsd(
            &fixture,
            "next.dsf",
            include_bytes!("../../aede-audio/tests/fixtures/stereo128.dsf"),
            true,
        )
        .await;
        let events =
            dsd_producer_events(&fixture, &[&first.reference, &next.reference], None, false).await;
        assert_eq!(
            events.len(),
            1,
            "no format or audio may escape a refused queue"
        );
        assert!(
            matches!(&events[0],ProducerEvent::Error(error) if error.code=="unsupported_dsd_output")
        );
        assert!(
            dsd_producer_events(&fixture, &[&first.reference], None, true)
                .await
                .is_empty()
        );
    });
}

#[test]
fn original_dsd_queues_keep_channel_changes_and_exact_bit_seek_bounds() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_dsd(
            &fixture,
            "first.dsf",
            include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
            false,
        )
        .await;
        let mut stereo = include_bytes!("../../aede-audio/tests/fixtures/stereo128.dsf").to_vec();
        stereo[56..60].copy_from_slice(&2_822_400u32.to_le_bytes());
        let next = install_dsd(&fixture, "stereo64.dsf", &stereo, true).await;
        let events = dsd_producer_events(
            &fixture,
            &[&first.reference, &next.reference],
            Some(1968),
            false,
        )
        .await;
        let mut formats = Vec::new();
        let mut starts = Vec::new();
        let mut sent = 0;
        for event in events {
            match event {
                ProducerEvent::Format(format) => {
                    assert_eq!(format.encoding, "dsd16le");
                    assert_eq!(format.sample_rate, 176400);
                    let original = format.original.unwrap();
                    formats.push((format.channels, original.position_samples));
                }
                ProducerEvent::Audio { bytes, frames } => {
                    assert!(bytes.len() <= 8192);
                    sent += frames
                }
                ProducerEvent::Track(track) => starts.push(track.start_frame),
                ProducerEvent::TrackEnd(_) => {}
                ProducerEvent::Eof { frames } => assert_eq!(frames, sent),
                ProducerEvent::Error(error) => panic!("{}", error.code),
            }
        }
        assert_eq!(formats, [(1, 1968), (2, 0)]);
        assert_eq!(starts, [0, 4096 - 123]);
        for invalid in [1, 65537, u64::MAX] {
            let events =
                dsd_producer_events(&fixture, &[&first.reference], Some(invalid), false).await;
            assert!(
                matches!(&events[..],[ProducerEvent::Error(error)] if error.code=="invalid_seek"),
                "{invalid}"
            );
        }
    });
}

#[test]
fn original_dsd_source_changes_and_truncation_cannot_publish_audio() {
    test_runtime().block_on(async {
        let fixture=Fixture::new();
        let source=include_bytes!("../../aede-audio/tests/fixtures/tone.dsf");
        let installed=install_dsd(&fixture,"source.dsf",source,false).await;
        let start=Start::parse(&serde_json::json!({"type":"start","track":installed.reference,"normalize":"off","dsd_output":"dop"}).to_string()).unwrap();
        let (sources,catalog)=current_sources(&fixture.0,&start.references).await.unwrap();
        fs::write(&installed.path,&source[..source.len()-1]).unwrap();
        let (mut receive,worker)=spawn_producer(sources,settings(catalog,fixture.0.data_dir.clone(),&start),Arc::new(AtomicBool::new(false)));
        let event=receive.recv().await.unwrap();
        assert!(matches!(event,ProducerEvent::Error(error) if error.code=="source_changed"));
        assert!(receive.recv().await.is_none());worker.await.unwrap();
        let truncated=install_dsd(&fixture,"truncated.dsf",source,false).await;
        fs::write(&truncated.path,&source[..source.len()-1]).unwrap();
        let metadata=fs::metadata(&truncated.path).unwrap();
        {
            // Keep catalog hints to exercise source-content admission separately.
            let mut held=fixture.0.catalog.write().await;
            let catalog=held.as_mut().unwrap();
            let file=&mut catalog.files[0];
            file.size=metadata.len();
            file.mtime=clock::mtime_seconds(&metadata);
            catalog.file_mtime_subseconds.insert(
                truncated.path.to_string_lossy().into_owned(),
                clock::mtime_subseconds(&metadata),
            );
        }
        let events=dsd_producer_events(&fixture,&[&truncated.reference],None,false).await;
        assert!(matches!(&events[..],[ProducerEvent::Error(error)] if error.code=="unsupported_dsd_output"));
    });
}

fn next_dsd_text(
    socket: &mut crate::playback_test_support::Socket,
    kind: &str,
) -> serde_json::Value {
    loop {
        match socket.next() {
            ServerFrame::Text(frame) if frame["type"] == kind => return frame,
            ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
            // Stale unconsumed epoch bytes are discarded, never acknowledged.
            ServerFrame::Binary(_) => {}
            ServerFrame::Close => panic!("closed before {kind}"),
        }
    }
}

async fn restarted_dsd_keeps_original_bits(edit: bool) {
    let fixture = Fixture::new();
    let installed = install_dsd(
        &fixture,
        "source.dsf",
        include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
        false,
    )
    .await;
    let mut reader = aede_audio::dop::Reader::open(&installed.path).unwrap();
    let mut original = Vec::new();
    while let Some(block) = reader.next_chunk(|| Ok(())).unwrap() {
        for word in block.payload {
            original.extend_from_slice(&word.0.to_le_bytes());
        }
    }
    let (address, server) = start_server(fixture.0.clone()).await;
    let token = login(address, "alice");
    let mut socket = upgraded_socket(address, &token);
    let start = serde_json::json!({
        "type":"start", "track":installed.reference, "normalize":"off",
        "dsd_output":"dop", "interactive":true,
    });
    socket.send_text(&start.to_string());
    let queue = next_dsd_text(&mut socket, "queue");
    assert_eq!(queue["position_samples"], 0);
    let initial = next_dsd_text(&mut socket, "format");
    assert_eq!(initial["epoch"], 0);
    if edit {
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_eq!(&bytes[..123 * 2], &original[..123 * 2]);
                    break;
                }
                ServerFrame::Text(frame) => assert_eq!(frame["type"], "track"),
                ServerFrame::Close => panic!("closed before original words"),
            }
        }
        // Only123carrierframes have actually been consumed; the rest of that
        // packet is abandoned on reset and must be delivered exactly once later.
        let control = serde_json::json!({
            "type":"edit_queue", "epoch":0, "frames":123,
            "revision":queue["revision"], "items":[],
        });
        socket.send_text(&control.to_string());
    } else {
        socket.send_text(r#"{"type":"seek","epoch":0,"frames":0,"position_samples":1968}"#);
    }
    let reset = next_dsd_text(&mut socket, "reset");
    assert_eq!(reset["epoch"], 1);
    assert_eq!(reset["position_samples"], 1968);
    assert_eq!(reset["position_ms"], 0);
    socket.send_text(r#"{"type":"reset_ack","epoch":1}"#);
    let format = next_dsd_text(&mut socket, "format");
    assert_eq!(format["position_samples"], 1968);
    assert_eq!(format["position_ms"], 0);
    let mut captured = Vec::new();
    let mut sent = 0;
    let mut recorded = 0;
    loop {
        match socket.next() {
            ServerFrame::Binary(bytes) => {
                sent += bytes.len() / 2;
                captured.extend_from_slice(&bytes);
                socket.send_text(
                    &serde_json::json!({
                        "type":"ack", "epoch":1, "frames":sent,
                    })
                    .to_string(),
                );
            }
            ServerFrame::Text(frame) if frame["type"] == "recorded" => {
                recorded += 1;
                assert_eq!(frame["completed"], edit);
                assert_eq!(frame["ms_played"], if edit { 23 } else { 22 });
            }
            ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
            ServerFrame::Close => break,
        }
    }
    assert_eq!(recorded, 1);
    assert_eq!(captured, original[123 * 2..]);
    assert_eq!(sent, 4096 - 123);
    server.abort();
}

#[test]
fn interactive_original_dsd_restarts_preserve_exact_bits_for_seek_and_queue_edits() {
    test_runtime().block_on(async {
        for edit in [false, true] {
            restarted_dsd_keeps_original_bits(edit).await;
        }
    });
}

#[test]
fn original_dsd_seek_during_final_native_drain_keeps_the_current_track() {
    test_runtime().block_on(async {
        for (exact, queue_phase) in [
            (true, 0),
            (false, 0),
            (true, 1),
            (false, 1),
            (true, 2),
            (false, 2),
        ] {
            let fixture = Fixture::new();
            let installed = install_dsd(
                &fixture,
                "source.dsf",
                include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
                false,
            )
            .await;
            let next = if queue_phase > 0 {
                Some(
                    install_dsd(
                        &fixture,
                        "next.dsf",
                        include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
                        true,
                    )
                    .await,
                )
            } else {
                None
            };
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = upgraded_socket(address, &token);
            let mut start = serde_json::json!({
                "type":"start",
                "normalize":"off", "dsd_output":"dop", "interactive":true,
            });
            if let Some(next) = &next {
                start["tracks"] = serde_json::json!([installed.reference, next.reference]);
            } else {
                start["track"] = installed.reference.clone().into();
            }
            socket.send_text(&start.to_string());
            next_dsd_text(&mut socket, "queue");
            let mut format = next_dsd_text(&mut socket, "format");
            let end = next_dsd_text(
                &mut socket,
                if queue_phase > 0 { "track_end" } else { "eof" },
            );
            let frames = end[if queue_phase > 0 {
                "end_frame"
            } else {
                "frames"
            }]
            .clone();
            if queue_phase == 2 {
                socket.send_text(
                    &serde_json::json!({"type":"ack", "epoch":0, "frames":frames}).to_string(),
                );
                format = next_dsd_text(&mut socket, "format");
            }
            // Native handoff can reach the last word while the DAC still
            // drains. A seek retires that output and reports its true count;
            // it does not certify completion or move to a nonexistent track.
            let mut seek = serde_json::json!({
                "type":"seek", "epoch":0, "frames":frames,
            });
            seek[if exact {
                "position_samples"
            } else {
                "position_ms"
            }] = 0.into();
            socket.send_text(&seek.to_string());
            let reset = next_dsd_text(&mut socket, "reset");
            assert_eq!(
                reset["current_occurrence"],
                if queue_phase == 2 { 2 } else { 1 }
            );
            assert_eq!(reset["position_samples"], 0);
            socket.send_text(r#"{"type":"reset_ack","epoch":1}"#);
            let resumed = next_dsd_text(&mut socket, "format");
            assert_eq!(resumed["track"], format["track"]);
            assert_eq!(resumed["position_samples"], 0);
            socket.send_text(r#"{"type":"stop","epoch":1,"frames":0}"#);
            let recorded = next_dsd_text(&mut socket, "recorded");
            assert_eq!(recorded["ms_played"], 23);
            assert_eq!(recorded["completed"], false);
            server.abort();
        }
    });
}

#[test]
fn original_dsd_millisecond_seeks_publish_the_actual_aligned_reset_position() {
    test_runtime().block_on(async {
        for (milliseconds, bits, actual_ms) in
            [(0, 0, 0), (1, 2816, 0), (5, 14112, 5), (23, 64912, 22)]
        {
            let fixture = Fixture::new();
            let installed = install_dsd(
                &fixture,
                "source.dsf",
                include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
                false,
            )
            .await;
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = upgraded_socket(address, &token);
            socket.send_text(
                &serde_json::json!({
                    "type":"start", "track":installed.reference,
                    "normalize":"off", "dsd_output":"dop", "interactive":true,
                })
                .to_string(),
            );
            next_dsd_text(&mut socket, "queue");
            next_dsd_text(&mut socket, "format");
            socket.send_text(
                &serde_json::json!({
                    "type":"seek", "epoch":0, "frames":0, "position_ms":milliseconds,
                })
                .to_string(),
            );
            let reset = next_dsd_text(&mut socket, "reset");
            assert_eq!(reset["position_samples"], bits, "{milliseconds}ms: {reset}");
            assert_eq!(reset["position_ms"], actual_ms);
            socket.send_text(r#"{"type":"reset_ack","epoch":1}"#);
            let format = next_dsd_text(&mut socket, "format");
            assert_eq!(format["position_samples"], bits);
            assert_eq!(format["position_ms"], actual_ms);
            socket.send_text(r#"{"type":"stop","epoch":1,"frames":0}"#);
            server.abort();
        }
    });
}

#[test]
fn original_dsd_full_stop_ack_preserves_duration_without_certifying_completion() {
    test_runtime().block_on(async {
        for (dop, stop) in [(true, true), (true, false), (false, true)] {
            let fixture = Fixture::new();
            let installed = if dop {
                install_dsd(
                    &fixture,
                    "source.dsf",
                    include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
                    false,
                )
                .await
            } else {
                install_wav(&fixture, 400).await
            };
            let (address, server) = start_server(fixture.0.clone()).await;
            let token = login(address, "alice");
            let mut socket = upgraded_socket(address, &token);
            let mut start = serde_json::json!({
                "type":"start", "track":installed.reference,
                "normalize":"off", "interactive":true,
            });
            if dop {
                start["dsd_output"] = "dop".into();
            }
            socket.send_text(&start.to_string());
            next_dsd_text(&mut socket, "queue");
            let format = next_dsd_text(&mut socket, "format");
            let channels = format["channels"].as_u64().unwrap();
            let mut frames = 0;
            loop {
                match socket.next() {
                    ServerFrame::Binary(bytes) => {
                        frames += bytes.len() as u64 / (channels * if dop { 2 } else { 4 });
                    }
                    ServerFrame::Text(frame) if frame["type"] == "eof" => {
                        assert_eq!(frame["frames"], frames);
                        break;
                    }
                    ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
                    ServerFrame::Close => panic!("closed before EOF"),
                }
            }
            // A backend may consume every frame before a subsequent device
            // drain/restoration fails. Stop carries the true count but is not
            // the successful final-ACK/EOF completion handshake for DoP.
            socket.send_text(
                &serde_json::json!({
                    "type":if stop {"stop"} else {"ack"}, "epoch":0, "frames":frames,
                })
                .to_string(),
            );
            let recorded = next_dsd_text(&mut socket, "recorded");
            assert_eq!(recorded["ms_played"], if dop { 23 } else { 50 });
            assert_eq!(recorded["completed"], !dop || !stop, "{dop}: {stop}");
            let saved = user::load(&user::user_path(&fixture.0.data_dir))
                .unwrap()
                .unwrap();
            assert_eq!(saved.plays.len(), 1);
            assert_eq!(saved.plays[0].completed, !dop || !stop);
            server.abort();
        }
    });
}

#[test]
fn abandoned_original_dsd_queue_cannot_certify_a_fully_acknowledged_prior_track() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_dsd(
            &fixture,
            "source.dsf",
            include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),
            false,
        )
        .await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        socket.send_text(
            &serde_json::json!({
                "type":"start", "tracks":[installed.reference,installed.reference],
                "normalize":"off", "dsd_output":"dop",
            })
            .to_string(),
        );
        let end = next_dsd_text(&mut socket, "track_end");
        assert_eq!(end["index"], 0);
        socket.send_text(&serde_json::json!({"type":"ack","frames":end["end_frame"]}).to_string());
        socket.close();
        for _ in 0..80 {
            if let Some(saved) = user::load(&user::user_path(&fixture.0.data_dir)).unwrap()
                && !saved.plays.is_empty()
            {
                assert_eq!(saved.plays.len(), 1);
                assert_eq!(saved.plays[0].ms_played, 23);
                assert!(!saved.plays[0].completed);
                server.abort();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("acknowledged DoP listen was not saved");
    });
}

#[test]
fn status_advertises_original_dsd_transport_separately_from_device_support() {
    test_runtime().block_on(async {
        let status = crate::catalog::status(State(crate::test_support::sample_state())).await;
        let value = serde_json::to_value(status.0).unwrap();
        assert!(
            value["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "dsd_original_playback")
        );
    });
}

#[test]
fn interactive_original_bit_seeks_refuse_ambiguity_and_never_widen_pcm_controls() {
    test_runtime().block_on(async {
        for (dop,fields) in [
            (true,serde_json::json!({"position_ms":0,"position_samples":16})),
            (true,serde_json::json!({"position_samples":null})),
            (true,serde_json::json!({"position_samples":1})),
            (true,serde_json::json!({"position_samples":65537})),
            (false,serde_json::json!({"position_samples":16})),
        ] {
            let fixture=Fixture::new();
            let installed=if dop {
                install_dsd(&fixture,"source.dsf",include_bytes!("../../aede-audio/tests/fixtures/tone.dsf"),false).await
            } else { install_wav(&fixture,400).await };
            let (address,server)=start_server(fixture.0.clone()).await;
            let token=login(address,"alice");let mut socket=upgraded_socket(address,&token);
            let mut start=serde_json::json!({"type":"start","track":installed.reference,"normalize":"off","interactive":true});
            if dop {start["dsd_output"]="dop".into()}
            socket.send_text(&start.to_string());
            next_dsd_text(&mut socket,"queue");
            let format=next_dsd_text(&mut socket,"format");
            if !dop { for key in ["source_rate","source_samples","position_samples"] {assert!(format.get(key).is_none(),"{format}");} }
            let mut control=serde_json::json!({"type":"seek","epoch":0,"frames":0});
            for (key,value) in fields.as_object().unwrap() {control[key]=value.clone();}
            socket.send_text(&control.to_string());
            let mut failed=false;
            loop {
                match socket.next() {
                    ServerFrame::Text(frame) if frame["type"]=="error"=>{assert!(matches!(frame["code"].as_str(),Some("invalid_seek"|"invalid_control")));failed=true;break},
                    ServerFrame::Text(frame)=>assert_ne!(frame["type"],"reset","invalid seek reset the stream"),
                    ServerFrame::Binary(_)=>{},
                    ServerFrame::Close=>break,
                }
            }
            assert!(failed,"{dop}: {fields}");
            drop(socket);server.abort();
        }
    });
}
