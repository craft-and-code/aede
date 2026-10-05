//! Scripted test-only receiver with the same bounded Cast envelope as the sender.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Finish,
    Busy,
    Refuse,
    Foreign,
    Wait,
}

pub(super) async fn receiver<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    scenario: Scenario,
    loaded: Option<tokio::sync::oneshot::Sender<()>>,
) -> Vec<String> {
    receiver_with_original(stream, scenario, loaded, None).await
}

pub(super) async fn receiver_with_original<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    scenario: Scenario,
    loaded: Option<tokio::sync::oneshot::Sender<()>>,
    original: Option<Vec<u8>>,
) -> Vec<String> {
    let mut events = Vec::new();
    let mut occurrence = 0;
    let mut loaded = loaded;
    loop {
        let message = match timeout(Duration::from_secs(4), cast_wire::read(&mut stream)).await {
            Ok(Ok(message)) => message,
            Ok(Err(_)) => break,
            Err(_) => panic!("sender stopped making progress: {events:?}"),
        };
        let kind = message.payload["type"].as_str().unwrap().to_owned();
        let id = message.payload["requestId"].clone();
        let sender = message.source.clone();
        let (namespace, source, payload) = match kind.as_str() {
            "CONNECT" => {
                events.push(format!("CONNECT:{}", message.destination));
                continue;
            }
            "PING" => (HEARTBEAT, "receiver-0", json!({"type":"PONG"})),
            "PONG" => continue,
            "GET_STATUS" if message.namespace == RECEIVER => {
                let apps = if matches!(scenario, Scenario::Busy) {
                    json!([{"appId":"OTHER", "transportId":"web-old"}])
                } else {
                    json!([])
                };
                (
                    RECEIVER,
                    "receiver-0",
                    json!({"type":"RECEIVER_STATUS", "requestId":id, "status":{"applications":apps}}),
                )
            }
            "LAUNCH" => {
                assert_eq!(message.payload["appId"], APPLICATION);
                events.push("LAUNCH".into());
                (
                    RECEIVER,
                    "receiver-0",
                    json!({"type":"RECEIVER_STATUS", "requestId":id,
                    "status":{"applications":[{"appId":APPLICATION, "transportId":"web-test"}]}}),
                )
            }
            "LOAD" => {
                assert_eq!(message.destination, "web-test");
                assert_eq!(message.namespace, MEDIA);
                assert_eq!(message.payload["autoplay"], true);
                assert_eq!(message.payload["media"]["contentType"], "audio/flac");
                if let Some(expected) = &original {
                    let url = message.payload["media"]["contentId"].as_str().unwrap();
                    let (address, path) = url
                        .strip_prefix("http://")
                        .unwrap()
                        .split_once('/')
                        .unwrap();
                    let mut http = tokio::net::TcpStream::connect(address).await.unwrap();
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    http.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {address}\r\nOrigin: https://www.gstatic.com\r\n\r\n").as_bytes()).await.unwrap();
                    let mut body = Vec::new();
                    timeout(Duration::from_secs(3), http.read_to_end(&mut body))
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(body.starts_with(b"HTTP/1.1 200"));
                    let at = body
                        .windows(4)
                        .position(|part| part == b"\r\n\r\n")
                        .unwrap()
                        + 4;
                    assert_eq!(&body[at..], expected);
                }
                events.push(format!(
                    "LOAD:{}",
                    message.payload["media"]["contentId"].as_str().unwrap()
                ));
                occurrence += 1;
                if matches!(scenario, Scenario::Refuse) {
                    (
                        MEDIA,
                        "web-test",
                        json!({"type":"LOAD_FAILED", "requestId":id}),
                    )
                } else {
                    let status = json!({"type":"MEDIA_STATUS", "requestId":id,
                        "status":[{"mediaSessionId":occurrence, "playerState":"PLAYING", "media":message.payload["media"]}]});
                    cast_wire::write(
                        &mut stream,
                        &Message {
                            source: "web-test".into(),
                            destination: sender.clone(),
                            namespace: MEDIA.into(),
                            payload: status,
                        },
                    )
                    .await
                    .unwrap();
                    match scenario {
                        Scenario::Wait => continue,
                        Scenario::Foreign => (
                            MEDIA,
                            "web-test",
                            json!({"type":"MEDIA_STATUS", "status":[{"mediaSessionId":999, "playerState":"PLAYING"}]}),
                        ),
                        _ => (
                            MEDIA,
                            "web-test",
                            json!({"type":"MEDIA_STATUS", "status":[{"mediaSessionId":occurrence, "playerState":"IDLE", "idleReason":"FINISHED"}]}),
                        ),
                    }
                }
            }
            "GET_STATUS" => {
                if let Some(signal) = loaded.take() {
                    let _ = signal.send(());
                }
                (
                    MEDIA,
                    "web-test",
                    json!({"type":"MEDIA_STATUS", "requestId":id,
                    "status":[{"mediaSessionId":occurrence, "playerState":"PLAYING"}]}),
                )
            }
            "STOP" => {
                assert_eq!(message.payload["mediaSessionId"], occurrence);
                events.push("STOP".into());
                (
                    MEDIA,
                    "web-test",
                    json!({"type":"MEDIA_STATUS", "requestId":id,
                    "status":[{"mediaSessionId":occurrence, "playerState":"IDLE", "idleReason":"CANCELLED"}]}),
                )
            }
            _ => panic!("unexpected Cast command: {kind}"),
        };
        cast_wire::write(
            &mut stream,
            &Message {
                source: source.into(),
                destination: sender,
                namespace: namespace.into(),
                payload,
            },
        )
        .await
        .unwrap();
    }
    events
}
