use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::playback_test_support::{
    ServerFrame, Socket, additional_flac, assert_guarded_pcm, install_stereo_wav, upgraded_socket,
};
use crate::test_support::{start_server, test_runtime};

fn acknowledge(socket: &mut Socket, frames: u64) -> bool {
    match socket.try_send_text(&serde_json::json!({"type": "ack", "frames": frames}).to_string()) {
        Ok(()) => true,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
            ) =>
        {
            // Failure can close the socket while earlier PCM is still buffered
            // at the client. Such a late ACK cannot become listening history.
            false
        }
        Err(error) => panic!("unexpected acknowledgement failure: {error}"),
    }
}

async fn saved_history(fixture: &Fixture) -> UserData {
    for _ in 0..120 {
        if let Some(data) = user::load(&user::user_path(&fixture.0.data_dir)).unwrap() {
            return data;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("acknowledged audio must be retained in history");
}

#[test]
fn flac_md5_failure_has_no_successful_eof_and_acknowledged_audio_stays_incomplete() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let invalid = additional_flac(&fixture, "wrong-audio-md5.flac", true).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        socket.send_text(
            &serde_json::json!({
                "type": "start", "track": invalid.reference,
                "sample_rate": 192_000, "normalize": "off",
            })
            .to_string(),
        );
        let mut frames = 0_u64;
        let mut sent_acknowledgement = 0_u64;
        let mut acknowledgements_closed = false;
        let mut channels = 0_u64;
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_guarded_pcm(&bytes);
                    frames += bytes.len() as u64 / (channels * 4);
                    if !acknowledgements_closed {
                        if acknowledge(&mut socket, frames) {
                            sent_acknowledgement = frames;
                        } else {
                            acknowledgements_closed = true;
                        }
                    }
                }
                ServerFrame::Text(frame) if frame["type"] == "format" => {
                    assert_eq!(frame["sample_rate"], 192_000);
                    channels = frame["channels"].as_u64().unwrap();
                    assert_eq!(channels, 1);
                    assert_eq!(frame["max_unacknowledged_frames"], 192_000);
                }
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "decode_failed");
                    break;
                }
                ServerFrame::Text(frame) => {
                    panic!("no successful EOF/history confirmation: {frame}")
                }
                ServerFrame::Close => panic!("closed before decode failure"),
            }
        }
        assert!(
            frames > 192_000,
            "the source exceeds the window and forces an accepted ACK before failure"
        );
        let history = saved_history(&fixture).await;
        assert_eq!(history.plays.len(), 1);
        let play = &history.plays[0];
        assert_eq!(play.track.to_token(), invalid.reference);
        assert!(!play.completed);
        assert!(play.ms_played > 0 && play.ms_played <= sent_acknowledgement * 1000 / 192_000);
        // An ACK can still be in flight when decoding fails. Only acknowledgements
        // accepted by the server, rather than every received byte, enter history.
        assert_eq!(history.counts[0].count, 1);
        server.abort();
    });
}

#[test]
fn a_flac_md5_failure_keeps_the_consumed_preceding_track_and_never_starts_later_queue_entries() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let first = install_stereo_wav(&fixture, 16_000).await;
        let invalid = additional_flac(&fixture, "wrong-audio-md5.flac", true).await;
        let later = additional_flac(&fixture, "unvisited.flac", false).await;
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        let mut socket = upgraded_socket(address, &token);
        socket.send_text(
            &serde_json::json!({
                "type": "start", "tracks": [first.reference, invalid.reference, later.reference],
                "sample_rate": 192_000, "normalize": "off",
            })
            .to_string(),
        );
        let mut frames = 0_u64;
        let mut sent_acknowledgement = 0_u64;
        let mut acknowledgements_closed = false;
        let mut channels = 0_u64;
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        loop {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    assert_guarded_pcm(&bytes);
                    frames += bytes.len() as u64 / (channels * 4);
                    if !acknowledgements_closed {
                        if acknowledge(&mut socket, frames) {
                            sent_acknowledgement = frames;
                        } else {
                            acknowledgements_closed = true;
                        }
                    }
                }
                ServerFrame::Text(frame) => match frame["type"].as_str().unwrap() {
                    "format" => channels = frame["channels"].as_u64().unwrap(),
                    "track" => starts.push(frame["track"].as_str().unwrap().to_owned()),
                    "track_end" => ends.push(frame["track"].as_str().unwrap().to_owned()),
                    "error" => {
                        assert_eq!(frame["code"], "decode_failed");
                        break;
                    }
                    _ => panic!("no successful queue EOF/history confirmation: {frame}"),
                },
                ServerFrame::Close => panic!("closed before decode failure"),
            }
        }
        assert_eq!(starts, [first.reference.clone(), invalid.reference.clone()]);
        assert_eq!(ends, std::slice::from_ref(&first.reference));
        let history = saved_history(&fixture).await;
        assert_eq!(history.plays.len(), 2);
        assert_eq!(history.plays[0].track.to_token(), first.reference);
        assert!(history.plays[0].completed);
        assert_eq!(history.plays[0].ms_played, 2000);
        assert_eq!(history.plays[1].track.to_token(), invalid.reference);
        assert!(!history.plays[1].completed);
        assert!(history.plays[1].ms_played > 0 && history.plays[1].ms_played <= 2000);
        assert!(
            history.plays.iter().map(|play| play.ms_played).sum::<u64>()
                <= sent_acknowledgement * 1000 / 192_000
        );
        assert_eq!(history.counts.len(), 2);
        assert!(
            history
                .plays
                .iter()
                .all(|play| play.track.to_token() != later.reference)
        );
        server.abort();
    });
}
