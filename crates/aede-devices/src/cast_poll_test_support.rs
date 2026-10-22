//! Protocol-gated Cast status scenarios; request identifiers come from actual commands.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum StatusCase {
    DelayedPreviousPoll,
    CurrentPollForeignSession,
    CurrentPollForeignContent,
    UnsolicitedForeignSession,
    UnsolicitedForeignContent,
    UnknownRequestForeignSession,
    UnknownRequestForeignContent,
    BroadcastWithRetiredPoll,
}

#[derive(Debug, Default)]
pub(super) struct Observation {
    pub(super) loads: Vec<String>,
    pub(super) polls: Vec<(u64, u64)>,
    pub(super) stops: Vec<u64>,
    pub(super) injected_after_confirmation: Option<u64>,
}

impl StatusCase {
    fn crosses_occurrences(self) -> bool {
        matches!(
            self,
            Self::DelayedPreviousPoll | Self::BroadcastWithRetiredPoll
        )
    }

    fn changes_content(self) -> bool {
        matches!(
            self,
            Self::CurrentPollForeignContent
                | Self::UnsolicitedForeignContent
                | Self::UnknownRequestForeignContent
        )
    }
}

async fn media_reply<S: AsyncWrite + Unpin>(
    stream: &mut S,
    sender: &str,
    id: u64,
    session: u64,
    state: &str,
    content: Value,
) -> Result<(), String> {
    cast_wire::write(
        stream,
        &Message {
            source: "web-test".into(),
            destination: sender.into(),
            namespace: MEDIA.into(),
            payload: json!({"type":"MEDIA_STATUS", "requestId":id,
                "status":[{"mediaSessionId":session, "playerState":state,
                    "idleReason":"FINISHED", "media":content}]}),
        },
    )
    .await
}

async fn foreign_then_finish<S: AsyncWrite + Unpin>(
    stream: &mut S,
    sender: &str,
    case: StatusCase,
    id: u64,
    session: u64,
    content: &Value,
) -> Result<(), String> {
    let destination = if matches!(case, StatusCase::BroadcastWithRetiredPoll) {
        "*"
    } else {
        sender
    };
    let foreign_session = if case.changes_content() { session } else { 999 };
    let foreign_content = if case.changes_content() {
        json!({"contentId":"http://127.0.0.1/foreign.flac"})
    } else {
        content.clone()
    };
    media_reply(
        stream,
        destination,
        id,
        foreign_session,
        "PLAYING",
        foreign_content,
    )
    .await?;
    // An implementation that incorrectly ignores the takeover must complete,
    // rather than making a negative assertion depend on a timeout.
    media_reply(stream, sender, 0, session, "IDLE", content.clone()).await
}

async fn receiver<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    case: StatusCase,
) -> Observation {
    let mut observed = Observation::default();
    let mut session = 0;
    let mut content = Value::Null;
    let mut retired = None;
    loop {
        let message = match timeout(Duration::from_secs(6), cast_wire::read(&mut stream)).await {
            Ok(Ok(message)) => message,
            Ok(Err(_)) => break,
            Err(_) => panic!("Cast sender stopped making progress: {case:?}; {observed:?}"),
        };
        let sender = message.source.clone();
        let id = message.payload["requestId"].as_u64().unwrap_or(0);
        let (source, namespace, payload) = match message.payload["type"].as_str().unwrap() {
            "CONNECT" | "PONG" => continue,
            "PING" => ("receiver-0", HEARTBEAT, json!({"type":"PONG"})),
            "GET_STATUS" if message.namespace == RECEIVER => (
                "receiver-0",
                RECEIVER,
                json!({"type":"RECEIVER_STATUS", "requestId":id,
                    "status":{"applications":[]}}),
            ),
            "LAUNCH" => (
                "receiver-0",
                RECEIVER,
                json!({"type":"RECEIVER_STATUS", "requestId":id,
                    "status":{"applications":[{"appId":APPLICATION,"transportId":"web-test"}]}}),
            ),
            "LOAD" => {
                session += 1;
                content = message.payload["media"].clone();
                observed
                    .loads
                    .push(content["contentId"].as_str().unwrap().into());
                media_reply(
                    &mut stream,
                    &sender,
                    id,
                    session,
                    "PLAYING",
                    content.clone(),
                )
                .await
                .unwrap();
                if session == 2 {
                    let (poll, previous_content) = retired.take().unwrap();
                    observed.injected_after_confirmation = Some(session);
                    let result = if matches!(case, StatusCase::DelayedPreviousPoll) {
                        media_reply(&mut stream, &sender, poll, 1, "PLAYING", previous_content)
                            .await
                            .unwrap();
                        media_reply(&mut stream, &sender, 0, session, "IDLE", content.clone()).await
                    } else {
                        foreign_then_finish(&mut stream, &sender, case, poll, session, &content)
                            .await
                    };
                    if result.is_err() {
                        break;
                    }
                    continue;
                }
                if matches!(
                    case,
                    StatusCase::UnsolicitedForeignSession
                        | StatusCase::UnsolicitedForeignContent
                        | StatusCase::UnknownRequestForeignSession
                        | StatusCase::UnknownRequestForeignContent
                ) {
                    let request = if matches!(
                        case,
                        StatusCase::UnknownRequestForeignSession
                            | StatusCase::UnknownRequestForeignContent
                    ) {
                        id + 1000
                    } else {
                        0
                    };
                    observed.injected_after_confirmation = Some(session);
                    if foreign_then_finish(&mut stream, &sender, case, request, session, &content)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                continue;
            }
            "GET_STATUS" => {
                assert_eq!(message.namespace, MEDIA);
                assert_eq!(message.payload["mediaSessionId"], session);
                assert!(id > 0);
                observed.polls.push((session, id));
                if case.crosses_occurrences() {
                    assert_eq!(session, 1);
                    assert!(retired.is_none());
                    retired = Some((id, content.clone()));
                    media_reply(&mut stream, &sender, 0, session, "IDLE", content.clone())
                        .await
                        .unwrap();
                } else {
                    observed.injected_after_confirmation = Some(session);
                    if foreign_then_finish(&mut stream, &sender, case, id, session, &content)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                continue;
            }
            "STOP" => {
                let stopped = message.payload["mediaSessionId"].as_u64().unwrap();
                observed.stops.push(stopped);
                (
                    "web-test",
                    MEDIA,
                    json!({"type":"MEDIA_STATUS", "requestId":id,
                        "status":[{"mediaSessionId":stopped,"playerState":"IDLE"}]}),
                )
            }
            kind => panic!("unexpected Cast command: {kind}"),
        };
        if cast_wire::write(
            &mut stream,
            &Message {
                source: source.into(),
                destination: sender,
                namespace: namespace.into(),
                payload,
            },
        )
        .await
        .is_err()
        {
            break;
        }
    }
    observed
}

pub(super) async fn exercise(case: StatusCase) -> (Result<(), String>, Observation) {
    let (stream, peer) = tokio::io::duplex(4096);
    let receiver = tokio::spawn(receiver(peer, case));
    let count = if case.crosses_occurrences() { 2 } else { 1 };
    let tracks = vec![crate::test_support::track(); count];
    let urls = (0..count)
        .map(|index| format!("http://127.0.0.1/cap/{index}"))
        .collect::<Vec<_>>();
    let (_shutdown, signal) = watch::channel(false);
    let result = timeout(
        Duration::from_secs(12),
        run_stream(stream, &tracks, &urls, false, signal),
    )
    .await
    .unwrap();
    (result, receiver.await.unwrap())
}
