use super::*;
use std::time::Duration;

#[tokio::test]
async fn shutdown_without_receivers_is_retained_for_late_subscribers() {
    let sender = Sender::default();
    assert!(sender.send(()).is_err());
    let mut late = sender.subscribe();
    assert_eq!(late.try_recv(), Ok(()));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), late.recv())
            .await
            .unwrap()
            .is_ok()
    );
}

#[tokio::test]
async fn shutdown_is_terminal_for_current_and_late_listeners_after_cancelled_waits() {
    let sender = Sender::default();
    let mut first = sender.subscribe();
    let mut second = sender.subscribe();
    assert_eq!(first.try_recv(), Err(broadcast::error::TryRecvError::Empty));
    assert!(
        tokio::time::timeout(Duration::from_millis(5), first.recv())
            .await
            .is_err()
    );
    assert_eq!(sender.clone().send(()).unwrap(), 2);
    first.recv().await.unwrap();
    second.recv().await.unwrap();
    assert_eq!(first.try_recv(), Ok(()));
    assert_eq!(sender.send(()).unwrap(), 2);
    let mut late = sender.subscribe();
    late.recv().await.unwrap();
    // Consuming the notification does not revive a stopped server.
    late.recv().await.unwrap();
    assert_eq!(late.try_recv(), Ok(()));
}

#[tokio::test]
async fn losing_the_last_sender_unblocks_an_idle_listener() {
    let sender = Sender::default();
    let mut receiver = sender.subscribe();
    drop(sender);
    assert_eq!(
        receiver.try_recv(),
        Err(broadcast::error::TryRecvError::Closed)
    );
    assert!(receiver.recv().await.is_err());
}
