use crate::accounts_test_support::{Fixture, login};
use crate::parental_test_support::{policy, save_policy};
use crate::playback_test_support::{ServerFrame, install_wav, upgraded_socket};
use serde_json::json;

#[tokio::test]
async fn parental_pcm_refuses_hidden_sources_and_exhausted_daily_budget_before_audio() {
    for blocked in [true, false] {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 16000).await;
        let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
        tokio::task::spawn_blocking(move || {
            let admin = login(address, "operator");
            let alice = login(address, "alice");
            let mut rules = policy(if blocked {
                vec!["artist:ac/dc"]
            } else {
                vec![]
            });
            if !blocked {
                for day in rules["days"].as_array_mut().unwrap() {
                    day["limit_seconds"] = json!(0);
                }
            }
            save_policy(address, &admin, "alice", rules);
            let mut socket = upgraded_socket(address, &alice);
            socket.send_text(
                &json!({"type":"start","track":track.reference,"normalize":"off"}).to_string(),
            );
            let ServerFrame::Text(frame) = socket.next() else {
                panic!("policy refusal must precede PCM")
            };
            assert_eq!(frame["type"], "error", "{frame}");
            assert_eq!(
                frame["code"],
                if blocked {
                    "parental_artist_blocked"
                } else {
                    "parental_daily_limit"
                },
                "{frame}"
            );
        })
        .await
        .unwrap();
        server.abort();
    }
}

#[tokio::test]
async fn parental_policy_activation_stops_an_already_open_unrestricted_pcm_stream() {
    let fixture = Fixture::new();
    let track = install_wav(&fixture, 160000).await;
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        let alice = login(address, "alice");
        let mut socket = upgraded_socket(address, &alice);
        socket.send_text(
            &json!({"type":"start","track":track.reference,"normalize":"off"}).to_string(),
        );
        let ServerFrame::Text(format) = socket.next() else {
            panic!("format")
        };
        assert_eq!(format["type"], "format");
        assert!(matches!(socket.next(), ServerFrame::Binary(_)));
        save_policy(address, &admin, "alice", policy(vec!["artist:ac/dc"]));
        for _ in 0..12 {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert!(
                        matches!(
                            frame["code"].as_str(),
                            Some(
                                "parental_conflict"
                                    | "parental_policy_unavailable"
                                    | "authentication_expired"
                            )
                        ),
                        "{frame}"
                    );
                    return;
                }
                ServerFrame::Binary(_) | ServerFrame::Text(_) => {}
                ServerFrame::Close => panic!("policy stop must provide a reason"),
            }
        }
        panic!("policy activation did not stop the open stream");
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn supervised_pcm_cannot_spend_more_audio_than_its_grant_with_immediate_acknowledgements() {
    let fixture = Fixture::new();
    let track = install_wav(&fixture, 48000).await;
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        let alice = login(address, "alice");
        let mut rules = policy(vec![]);
        for day in rules["days"].as_array_mut().unwrap() {
            day["limit_seconds"] = json!(2);
        }
        save_policy(address, &admin, "alice", rules);
        let mut socket = upgraded_socket(address, &alice);
        socket.send_text(
            &json!({"type":"start","track":track.reference,"normalize":"off"}).to_string(),
        );
        let ServerFrame::Text(format) = socket.next() else {
            panic!("format")
        };
        assert_eq!(format["sample_rate"], 8000);
        let began = std::time::Instant::now();
        let mut frames = 0u64;
        for _ in 0..150 {
            match socket.next() {
                ServerFrame::Binary(bytes) => {
                    frames += bytes.len() as u64 / 4;
                    assert!(
                        frames <= 16000,
                        "sent {frames} frames despite a two-second grant"
                    );
                    socket.send_text(&json!({"type":"ack","frames":frames}).to_string());
                }
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert!(frames > 0);
                    assert_eq!(frame["code"], "parental_daily_limit", "{frame}");
                    assert!(began.elapsed() >= std::time::Duration::from_millis(400));
                    return;
                }
                ServerFrame::Text(_) => {}
                ServerFrame::Close => panic!("quota exhaustion must be explicit"),
            }
        }
        panic!("quota did not terminate the controlled stream");
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn parental_pcm_revalidates_catalogue_publication_before_more_audio() {
    let fixture = Fixture::new();
    let track = install_wav(&fixture, 160000).await;
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        let alice = login(address, "alice");
        save_policy(address, &admin, "alice", policy(vec![]));
        let mut socket = upgraded_socket(address, &alice);
        socket.send_text(
            &json!({"type":"start","track":track.reference,"normalize":"off"}).to_string(),
        );
        assert!(matches!(socket.next(), ServerFrame::Text(_)));
        assert!(matches!(socket.next(), ServerFrame::Binary(_)));
        // A replacement catalogue can withdraw a root/source without changing its policy.
        fixture.0.parental.invalidate();
        for _ in 0..12 {
            match socket.next() {
                ServerFrame::Text(frame) if frame["type"] == "error" => {
                    assert_eq!(frame["code"], "parental_policy_unavailable", "{frame}");
                    return;
                }
                ServerFrame::Text(_) | ServerFrame::Binary(_) => {}
                ServerFrame::Close => panic!("a changed source snapshot needs an explicit stop"),
            }
        }
        panic!("catalogue replacement did not invalidate controlled PCM");
    })
    .await
    .unwrap();
    server.abort();
}
