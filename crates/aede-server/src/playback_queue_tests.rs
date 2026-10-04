use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::playback_test_support::{
    ServerFrame, Socket, assert_guarded_pcm, install_wav, upgraded_socket, wav_bytes,
};
use crate::test_support::{start_server, test_runtime};
use aede_core::playback::session::PcmSession;
use aede_dsp::{PcmFormat, ToneControls};
use std::io::{ErrorKind, Read};

fn acknowledge(socket: &mut Socket, frames: u64) {
    socket.send_text(&serde_json::json!({"type": "ack", "frames": frames}).to_string());
}

fn start_queue(socket: &mut Socket, references: &[&str], rate: u32) {
    socket.send_text(
        &serde_json::json!({
            "type": "start", "tracks": references, "normalize": "off",
            "sample_rate": rate, "bass": 6, "treble": -3,
        })
        .to_string(),
    );
}

async fn history_with_plays(fixture: &Fixture, expected: usize) -> UserData {
    for _ in 0..120 {
        if let Ok(Some(data)) = user::load(&user::user_path(&fixture.0.data_dir))
            && data.plays.len() == expected
        {
            return data;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("expected {expected} separately attributed queue listens");
}

async fn additional_wav(fixture: &Fixture, frames: u32, channels: u16) -> String {
    let path = fixture.0.data_dir.join(format!("queue-{channels}.wav"));
    let source = wav_bytes(frames);
    let mut bytes = source[..44].to_vec();
    let size = frames * u32::from(channels) * 2;
    bytes[4..8].copy_from_slice(&(36 + size).to_le_bytes());
    bytes[22..24].copy_from_slice(&channels.to_le_bytes());
    bytes[28..32].copy_from_slice(&(8_000 * u32::from(channels) * 2).to_le_bytes());
    bytes[32..34].copy_from_slice(&(channels * 2).to_le_bytes());
    bytes[40..44].copy_from_slice(&size.to_le_bytes());
    for sample in source[44..].as_chunks::<2>().0 {
        for _ in 0..channels {
            bytes.extend_from_slice(sample);
        }
    }
    std::fs::write(&path, bytes).unwrap();
    let metadata = std::fs::metadata(&path).unwrap();
    let path_text = path.to_string_lossy().into_owned();
    let mut guard = fixture.0.catalog.write().await;
    let catalog = guard.as_mut().unwrap();
    let mut file = catalog.files[0].clone();
    file.id = catalog.files.len() as u32;
    file.path = path_text.clone();
    file.size = metadata.len();
    file.mtime = clock::mtime_seconds(&metadata);
    file.properties.channels = Some(channels);
    file.properties.duration_ms = Some(u64::from(frames) / 8);
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
    catalog
        .file_mtime_subseconds
        .insert(path_text, clock::mtime_subseconds(&metadata));
    let reference = EntityRef::of(catalog, EntityKind::Track, id)
        .unwrap()
        .to_token();
    store::save_catalog_only(catalog, &store::catalog_path(&fixture.0.data_dir)).unwrap();
    reference
}

#[test]
fn repeated_queue_occurrences_share_exact_processed_pcm_and_keep_separate_history() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 401).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start_queue(
            &mut socket,
            &[&installed.reference, &installed.reference],
            11_025,
        );
        let mut received = Vec::new();
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut recorded = Vec::new();
        let mut frames = 0;
        let mut eof = false;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_guarded_pcm(&bytes);
                    frames += bytes.len() as u64 / 4;
                    received.extend_from_slice(&bytes);
                    acknowledge(&mut socket, frames);
                }
                ServerFrame::Text(frame) => match frame["type"].as_str().unwrap() {
                    "format" => {
                        assert_eq!(frame["sample_rate"], 11_025);
                        assert_eq!(frame["channels"], 1);
                    }
                    "track" => starts.push(frame),
                    "track_end" => ends.push(frame),
                    "eof" => {
                        assert_eq!(frame["frames"], frames);
                        eof = true;
                    }
                    "recorded" => {
                        assert!(eof, "confirmation follows final queue consumption");
                        recorded.push(frame);
                    }
                    _ => panic!("unexpected queue frame {frame}"),
                },
                ServerFrame::Close => break,
            }
        }
        // Compare with one uninterrupted source programme, rather than with
        // independent per-track processors that would hide filter/SRC resets.
        let mut source: Vec<f32> = wav_bytes(401)[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|sample| f32::from(i16::from_le_bytes(*sample)) / 32_768.0)
            .collect();
        source.extend_from_within(..);
        let tone = ToneControls::new(6.0, -3.0).unwrap();
        let mut whole = PcmSession::new(PcmFormat::new(8_000, 1).unwrap(), 11_025, tone).unwrap();
        whole.begin_track(0, tone.safe_preamp_db()).unwrap();
        let mut expected = whole.push_source(&source).unwrap().f32le.to_vec();
        expected.extend_from_slice(whole.end_track().unwrap().f32le);
        expected.extend_from_slice(whole.finish().unwrap().f32le);
        assert_eq!(
            received, expected,
            "no reset, inserted silence or omitted SRC tail"
        );
        assert_eq!(frames, 1_106);
        assert_eq!(starts.len(), 2);
        assert_eq!(ends.len(), 2);
        assert_eq!(starts[0]["start_frame"], 0);
        assert_eq!(starts[1]["start_frame"], 553);
        assert_eq!(ends[0]["end_frame"], 553);
        assert_eq!(ends[1]["end_frame"], 1_106);
        assert_eq!(recorded.len(), 2);
        for (index, frame) in recorded.iter().enumerate() {
            assert_eq!(frame["index"], index);
            assert_eq!(frame["track"], installed.reference);
            assert_eq!(frame["completed"], true);
            assert_eq!(frame["ms_played"], 50);
        }
        let history = history_with_plays(&fixture, 2).await;
        assert!(
            history
                .plays
                .iter()
                .all(|play| play.completed && play.ms_played == 50)
        );
        assert_eq!(history.counts[0].count, 2);
        server.abort();
    });
}

#[test]
fn closing_in_the_second_queue_occurrence_preserves_first_complete_and_second_partial() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let installed = install_wav(&fixture, 4_000).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start_queue(
            &mut socket,
            &[&installed.reference, &installed.reference],
            8_000,
        );
        let mut frames = 0;
        let mut second = false;
        loop {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "track" => {
                    second = frame["index"] == 1;
                }
                ServerFrame::Binary(bytes) => {
                    if second {
                        acknowledge(&mut socket, frames + 123);
                        socket.close();
                        break;
                    }
                    frames += bytes.len() as u64 / 4;
                    acknowledge(&mut socket, frames);
                }
                ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
                ServerFrame::Close => panic!("closed before second listen"),
            }
        }
        // Keep TCP alive until the server has read the ordered ACK/Close
        // frames; dropping with unread incoming PCM can reset the connection.
        let history = history_with_plays(&fixture, 2).await;
        assert!(history.plays[0].completed);
        assert_eq!(history.plays[0].ms_played, 500);
        assert!(!history.plays[1].completed);
        assert_eq!(history.plays[1].ms_played, 15);
        assert_eq!(history.counts[0].count, 2);
        server.abort();
    });
}

#[test]
fn queue_channel_change_waits_for_consumption_and_preserves_cumulative_frames() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let mono = install_wav(&fixture, 400).await;
        let stereo = additional_wav(&fixture, 800, 2).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start_queue(&mut socket, &[&mono.reference, &stereo], 8_000);
        let mut channels = 1;
        let mut frames = 0;
        let mut formats = 0;
        let mut records = 0;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_guarded_pcm(&bytes);
                    assert_eq!(bytes.len() % (channels * 4), 0);
                    frames += bytes.len() as u64 / (channels as u64 * 4);
                    if formats > 1 {
                        acknowledge(&mut socket, frames);
                    }
                }
                ServerFrame::Text(frame) if frame["type"] == "format" => {
                    formats += 1;
                    channels = frame["channels"].as_u64().unwrap() as usize;
                    if formats == 2 {
                        assert_eq!(frames, 400);
                        assert_eq!(channels, 2);
                        assert_eq!(frame["start_frame"], 400);
                    }
                }
                ServerFrame::Text(frame) if frame["type"] == "track_end" && formats == 1 => {
                    assert_eq!(frame["end_frame"], 400);
                    assert_eq!(frames, 400);
                    assert!(
                        socket.buffered.is_empty(),
                        "new format cannot bypass consumed old PCM"
                    );
                    socket
                        .stream
                        .set_read_timeout(Some(Duration::from_millis(250)))
                        .unwrap();
                    let failure = socket
                        .stream
                        .read(&mut [0_u8; 1])
                        .expect_err("wait for old PCM ACK");
                    assert!(matches!(
                        failure.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut
                    ));
                    socket
                        .stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    acknowledge(&mut socket, frames);
                }
                ServerFrame::Text(frame) if frame["type"] == "recorded" => records += 1,
                ServerFrame::Text(frame) if frame["type"] == "eof" => {
                    assert_eq!(frame["frames"], 1_200)
                }
                ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
                ServerFrame::Close => break,
            }
        }
        assert_eq!(formats, 2);
        assert_eq!(frames, 1_200);
        assert_eq!(records, 2);
        let history = history_with_plays(&fixture, 2).await;
        assert!(history.plays.iter().all(|play| play.completed));
        assert_eq!(history.plays[0].ms_played, 50);
        assert_eq!(history.plays[1].ms_played, 100);
        server.abort();
    });
}

#[test]
fn queue_a_b_a_keeps_order_and_counts_only_the_authenticated_owner() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_wav(&fixture, 400).await;
        let second = additional_wav(&fixture, 800, 1).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let owner = fixture.accounts().find("alice").unwrap().id.clone();
        let mut socket = upgraded_socket(address, &token);
        start_queue(
            &mut socket,
            &[&first.reference, &second, &first.reference],
            8_000,
        );
        let mut frames = 0;
        let mut starts = Vec::new();
        let mut records = 0;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    frames += bytes.len() as u64 / 4;
                    acknowledge(&mut socket, frames);
                }
                ServerFrame::Text(frame) if frame["type"] == "track" => starts.push(frame),
                ServerFrame::Text(frame) if frame["type"] == "recorded" => records += 1,
                ServerFrame::Text(frame) => assert_ne!(frame["type"], "error", "{frame}"),
                ServerFrame::Close => break,
            }
        }
        assert_eq!(records, 3);
        let expected = [&first.reference, &second, &first.reference];
        for (index, (frame, reference)) in starts.iter().zip(expected).enumerate() {
            assert_eq!(frame["index"], index);
            assert_eq!(frame["track"], *reference);
        }
        assert_eq!(starts.len(), 3);
        let history = history_with_plays(&fixture, 3).await;
        assert!(
            history
                .plays
                .iter()
                .all(|play| play.owner == owner && play.completed)
        );
        assert_eq!(
            history.play_count(&owner, &EntityRef::parse_token(&first.reference).unwrap()),
            2
        );
        assert_eq!(
            history.play_count(&owner, &EntityRef::parse_token(&second).unwrap()),
            1
        );
        assert_eq!(
            history
                .plays
                .iter()
                .map(|play| play.ms_played)
                .collect::<Vec<_>>(),
            [50, 100, 50]
        );
        server.abort();
    });
}

#[test]
fn invalidating_one_queued_source_before_publication_preserves_other_valid_listens() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_wav(&fixture, 4_000).await;
        let second = additional_wav(&fixture, 4_000, 1).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        start_queue(&mut socket, &[&first.reference, &second], 8_000);
        let mut frames = 0;
        let mut records = Vec::new();
        let mut failed = false;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => frames += bytes.len() as u64 / 4,
                ServerFrame::Text(frame) if frame["type"] == "eof" => {
                    // Publication cannot start until this final ACK. A changed
                    // first source must not erase the unrelated second listen.
                    assert_eq!(frames, 8_000);
                    std::fs::write(&first.path, wav_bytes(4_001)).unwrap();
                    acknowledge(&mut socket, frames);
                }
                ServerFrame::Text(frame) if frame["type"] == "recorded" => records.push(frame),
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "history_failed");
                    failed = true;
                }
                ServerFrame::Text(_) => {}
                ServerFrame::Close => break,
            }
        }
        assert!(failed);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["index"], 1);
        assert_eq!(records[0]["track"], second);
        let history = history_with_plays(&fixture, 1).await;
        assert_eq!(history.plays[0].track.to_token(), second);
        assert!(history.plays[0].completed);
        assert_eq!(history.plays[0].ms_played, 500);
        server.abort();
    });
}
