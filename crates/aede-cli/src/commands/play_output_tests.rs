#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_callback_preserves_samples_across_buffer_boundaries() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;

    let (sender, receiver) = mpsc::sync_channel(2);
    sender.send(vec![0.1, 0.2, 0.3]).unwrap();
    sender.send(vec![0.4, 0.5]).unwrap();
    let mut pending = super::native::PendingSamples::new(receiver);
    let consumed = AtomicU64::new(0);
    let mut first = [0.0; 2];
    let mut second = [0.0; 3];
    pending.render(&mut first, &consumed);
    pending.render(&mut second, &consumed);
    assert_eq!(first, [0.1, 0.2]);
    assert_eq!(second, [0.3, 0.4, 0.5]);
    assert_eq!(consumed.load(Ordering::Acquire), 5);
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_output_keeps_source_rate_or_chooses_nearest_compatible_rate() {
    let ranges = [(48_000, 48_000), (44_100, 96_000)];
    assert_eq!(
        super::native::select_rate(44_100, &ranges),
        Some((1, 44_100))
    );
    assert_eq!(
        super::native::select_rate(32_000, &ranges),
        Some((1, 44_100))
    );
    assert_eq!(
        super::native::select_rate(192_000, &ranges),
        Some((1, 96_000))
    );
    assert_eq!(
        super::native::select_rate(48_000, &[(48_000, 48_000)]),
        Some((0, 48_000))
    );
    assert_eq!(super::native::select_rate(48_000, &[]), None);
}
