use super::*;
use crate::playback_test_support::{ServerFrame, socket_pair};
use crate::test_support::test_runtime;

#[test]
fn decode_failure_waits_for_the_peer_close_with_an_ack_in_flight() {
    test_runtime().block_on(async {
        let shutdown = crate::shutdown::Sender::default();
        let mut receiver = shutdown.subscribe();
        let (mut socket, mut client, server) = socket_pair("close-test").await;
        let (received_close, close_received) = tokio::sync::oneshot::channel();
        let (release_client, released_client) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            client.send_text(r#"{"type":"ack","frames":1}"#);
            match client.next() {
                ServerFrame::Text(error) => assert_eq!(error["code"], "decode_failed"),
                _ => panic!("decode failure must precede the close"),
            }
            assert!(matches!(client.next(), ServerFrame::Close));
            if received_close.send(()).is_err() {
                return;
            }
            if released_client.recv().is_ok() {
                client.close();
            }
        });
        let mut closing = Box::pin(send_failure(
            &mut socket,
            StreamFailure::DECODE_FAILED,
            &mut receiver,
        ));
        tokio::select! {
            _ = &mut closing => panic!("server dropped the connection before the peer close"),
            result = close_received => result.unwrap(),
        }
        std::future::poll_fn(|context| {
            assert!(closing.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        release_client.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), closing)
            .await
            .expect("peer close releases the connection");
        client.join().unwrap();
        server.abort();
    });
}

#[test]
fn close_reply_deadline_releases_a_silent_peer() {
    test_runtime().block_on(async {
        let shutdown = crate::shutdown::Sender::default();
        let mut receiver = shutdown.subscribe();
        let (mut socket, _client, server) = socket_pair("close-test").await;
        events::send_message(&mut socket, Message::Close(None))
            .await
            .unwrap();
        let result =
            finish_failure_close(&mut socket, &mut receiver, Duration::from_millis(30)).await;
        assert_eq!(result, Err("close reply deadline"));
        server.abort();
    });
}

#[test]
fn queued_late_acks_do_not_extend_the_close_input_message_budget() {
    test_runtime().block_on(async {
        let shutdown = crate::shutdown::Sender::default();
        let mut receiver = shutdown.subscribe();
        let (mut socket, mut client, server) = socket_pair("close-test").await;
        for _ in 0..MAX_CLOSE_MESSAGES {
            client.send_text(r#"{"type":"ack","frames":1}"#);
        }
        events::send_message(&mut socket, Message::Close(None))
            .await
            .unwrap();
        assert_eq!(
            finish_failure_close(&mut socket, &mut receiver, CLOSE_TIMEOUT).await,
            Err("close input message limit")
        );
        server.abort();
    });
}

#[test]
fn close_input_bytes_are_bounded_separately_from_the_message_count() {
    test_runtime().block_on(async {
        let shutdown = crate::shutdown::Sender::default();
        let mut receiver = shutdown.subscribe();
        let (mut socket, mut client, server) = socket_pair("close-test").await;
        let message = "x".repeat(MAX_CONTROL_MESSAGE_BYTES);
        for _ in 0..=MAX_CLOSE_BYTES / message.len() {
            client.send_text(&message);
        }
        events::send_message(&mut socket, Message::Close(None))
            .await
            .unwrap();
        assert_eq!(
            finish_failure_close(&mut socket, &mut receiver, CLOSE_TIMEOUT).await,
            Err("close input byte limit")
        );
        server.abort();
    });
}

#[test]
fn disconnected_peer_does_not_retain_the_failure_close() {
    test_runtime().block_on(async {
        let shutdown = crate::shutdown::Sender::default();
        let mut receiver = shutdown.subscribe();
        let (mut socket, client, server) = socket_pair("close-test").await;
        events::send_message(&mut socket, Message::Close(None))
            .await
            .unwrap();
        drop(client);
        let result = finish_failure_close(&mut socket, &mut receiver, CLOSE_TIMEOUT).await;
        assert!(matches!(
            result,
            Err("transport failed" | "peer disconnected before close reply")
        ));
        server.abort();
    });
}

#[test]
fn server_shutdown_interrupts_a_pending_close_and_is_retained_for_late_waiters() {
    test_runtime().block_on(async {
        for before_wait in [false, true] {
            let shutdown = crate::shutdown::Sender::default();
            let mut receiver = shutdown.subscribe();
            let (mut socket, _client, server) = socket_pair("close-test").await;
            events::send_message(&mut socket, Message::Close(None))
                .await
                .unwrap();
            if before_wait {
                shutdown.send(()).unwrap();
            }
            let mut closing = Box::pin(finish_failure_close(
                &mut socket,
                &mut receiver,
                CLOSE_TIMEOUT,
            ));
            if !before_wait {
                std::future::poll_fn(|context| {
                    assert!(closing.as_mut().poll(context).is_pending());
                    std::task::Poll::Ready(())
                })
                .await;
                shutdown.send(()).unwrap();
            }
            assert_eq!(
                tokio::time::timeout(Duration::from_millis(100), closing)
                    .await
                    .expect("shutdown must not wait for the peer close"),
                Err("server shutdown")
            );
            server.abort();
        }
    });
}
