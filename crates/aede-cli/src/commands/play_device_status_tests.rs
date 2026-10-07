use std::sync::atomic::Ordering;

use cpal::ErrorKind;

use super::{DEVICE_CHANGED, DeviceStatus, REALTIME_DENIED};

#[test]
fn a_new_device_has_no_errors_or_xruns() {
    let status = DeviceStatus::default();
    assert!(status.check().is_ok());
    assert_eq!(status.snapshot_xruns(), 0);
    assert!(!status.is_failed());
}

#[test]
fn strict_route_changes_xruns_and_scheduling_failures_are_terminal() {
    for (kind, reason) in [
        (ErrorKind::DeviceChanged, "strict audio route changed"),
        (ErrorKind::Xrun, "strict audio host reported an xrun"),
        (
            ErrorKind::RealtimeDenied,
            "strict audio scheduling was denied",
        ),
    ] {
        let status = DeviceStatus::with_policy(true);
        assert!(!status.is_failed());
        status.record(kind);
        assert!(status.is_failed());
        let first = status
            .check()
            .expect_err("strict host notification is fatal");
        assert!(first.contains(reason));
        status.record(ErrorKind::BackendError);
        assert_eq!(status.check().expect_err("failure remains latched"), first);
        assert_eq!(status.pending_warnings.load(Ordering::Acquire), 0);
        assert_eq!(status.snapshot_xruns(), u64::from(kind == ErrorKind::Xrun));
    }
}

#[test]
fn strict_concurrent_notifications_keep_the_first_failure_and_count_xruns() {
    let status = DeviceStatus::with_policy(true);
    status.record(ErrorKind::Other);
    let first = status.check().expect_err("unknown host error is terminal");
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..100 {
                status.record(ErrorKind::Xrun);
            }
        });
        scope.spawn(|| {
            status.record(ErrorKind::DeviceChanged);
            status.record(ErrorKind::RealtimeDenied);
        });
    });
    assert!(status.is_failed());
    assert_eq!(status.check().expect_err("first cause survives"), first);
    assert_eq!(status.snapshot_xruns(), 100);
    assert_eq!(status.pending_warnings.load(Ordering::Acquire), 0);
}

#[test]
fn recoverable_device_warnings_remain_nonfatal_and_are_reported_once() {
    let status = DeviceStatus::default();
    for _ in 0..3 {
        status.record(ErrorKind::DeviceChanged);
        status.record(ErrorKind::RealtimeDenied);
    }
    assert_eq!(
        status.pending_warnings.load(Ordering::Acquire),
        DEVICE_CHANGED | REALTIME_DENIED
    );
    assert!(status.check().is_ok());
    assert_eq!(status.pending_warnings.load(Ordering::Acquire), 0);

    status.record(ErrorKind::DeviceChanged);
    status.record(ErrorKind::RealtimeDenied);
    assert_eq!(status.pending_warnings.load(Ordering::Acquire), 0);
    assert!(status.check().is_ok());
}

#[test]
fn repeated_xruns_are_counted_without_failing_the_stream() {
    let status = DeviceStatus::default();
    for _ in 0..7 {
        status.record(ErrorKind::Xrun);
    }
    assert_eq!(status.snapshot_xruns(), 7);
    assert!(status.check().is_ok());
    assert_eq!(status.snapshot_xruns(), 7);
    assert_eq!(status.pending_warnings.load(Ordering::Acquire), 0);
}

#[test]
fn the_first_fatal_device_error_survives_later_callbacks_and_checks() {
    let status = DeviceStatus::default();
    status.record(ErrorKind::DeviceNotAvailable);
    let first = status.check().expect_err("unavailable device is fatal");
    assert!(first.contains("device is no longer available"));

    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..100 {
                status.record(ErrorKind::StreamInvalidated);
                status.record(ErrorKind::Xrun);
            }
        });
        scope.spawn(|| {
            status.record(ErrorKind::DeviceChanged);
            status.record(ErrorKind::RealtimeDenied);
            status.record(ErrorKind::BackendError);
        });
    });

    assert_eq!(
        status.check().expect_err("fatal error stays recorded"),
        first
    );
    assert_eq!(
        status.check().expect_err("checks do not clear errors"),
        first
    );
    assert_eq!(status.snapshot_xruns(), 100);
}

#[test]
fn each_fatal_device_category_has_a_driver_side_reason() {
    let cases = [
        (ErrorKind::DeviceBusy, "device is busy"),
        (
            ErrorKind::DeviceNotAvailable,
            "device is no longer available",
        ),
        (ErrorKind::HostUnavailable, "audio host is unavailable"),
        (
            ErrorKind::InvalidInput,
            "invalid audio input or configuration",
        ),
        (
            ErrorKind::PermissionDenied,
            "audio device access was denied",
        ),
        (
            ErrorKind::ResourceExhausted,
            "audio resources were exhausted",
        ),
        (ErrorKind::StreamInvalidated, "audio stream must be rebuilt"),
        (
            ErrorKind::UnsupportedConfig,
            "audio configuration is unsupported",
        ),
        (
            ErrorKind::UnsupportedOperation,
            "audio operation is unsupported",
        ),
        (ErrorKind::BackendError, "audio backend reported an error"),
        (ErrorKind::Other, "unclassified audio device error"),
    ];
    for (kind, reason) in cases {
        let status = DeviceStatus::default();
        status.record(kind);
        let error = status.check().expect_err("category is fatal");
        assert!(error.starts_with("audio device error: "));
        assert!(error.contains(reason), "{kind:?}: {error}");
    }
}
