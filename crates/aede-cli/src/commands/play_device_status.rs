//! Callback-safe device error capture, reported by the playback driver.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use cpal::ErrorKind;

const DEVICE_CHANGED: u8 = 1;
const REALTIME_DENIED: u8 = 2;

/// Records only fixed-size atomic state on CPAL's error callback thread.
#[derive(Default)]
pub(super) struct DeviceStatus {
    strict: bool,
    seen_warnings: AtomicU8,
    pending_warnings: AtomicU8,
    first_fatal: AtomicU8,
    xruns: AtomicU64,
}

impl DeviceStatus {
    pub(super) fn with_policy(strict: bool) -> Self {
        Self {
            strict,
            ..Self::default()
        }
    }

    pub(super) fn is_failed(&self) -> bool {
        self.first_fatal.load(Ordering::Acquire) != 0
    }

    pub(super) fn record(&self, kind: ErrorKind) {
        if self.strict {
            if kind == ErrorKind::Xrun {
                self.xruns.fetch_add(1, Ordering::Relaxed);
            }
            self.record_fatal(FatalReason::from_kind(kind));
            return;
        }
        match kind {
            ErrorKind::DeviceChanged => self.record_warning(DEVICE_CHANGED),
            ErrorKind::RealtimeDenied => self.record_warning(REALTIME_DENIED),
            ErrorKind::Xrun => {
                self.xruns.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.record_fatal(FatalReason::from_kind(kind));
            }
        }
    }

    fn record_fatal(&self, reason: FatalReason) {
        // Preserve the first cause even when later callbacks also fail.
        let _ = self.first_fatal.compare_exchange(
            0,
            reason as u8,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    fn record_warning(&self, flag: u8) {
        if self.seen_warnings.fetch_or(flag, Ordering::Relaxed) & flag == 0 {
            self.pending_warnings.fetch_or(flag, Ordering::Release);
        }
    }

    /// Called outside the callback; reporting and String allocation happen here.
    pub(super) fn check(&self) -> Result<(), String> {
        let warnings = self.pending_warnings.swap(0, Ordering::AcqRel);
        if warnings & DEVICE_CHANGED != 0 {
            eprintln!("audio device warning: audio route changed; playback remains active");
        }
        if warnings & REALTIME_DENIED != 0 {
            eprintln!("audio device warning: real-time scheduling was denied; playback continues");
        }
        let fatal = self.first_fatal.load(Ordering::Acquire);
        if fatal == 0 {
            Ok(())
        } else {
            Err(format!(
                "audio device error: {}",
                FatalReason::message(fatal)
            ))
        }
    }

    pub(super) fn snapshot_xruns(&self) -> u64 {
        self.xruns.load(Ordering::Relaxed)
    }
}

#[repr(u8)]
enum FatalReason {
    DeviceBusy = 1,
    DeviceNotAvailable,
    HostUnavailable,
    InvalidInput,
    PermissionDenied,
    ResourceExhausted,
    StreamInvalidated,
    UnsupportedConfig,
    UnsupportedOperation,
    BackendError,
    Unclassified,
    RouteChanged,
    Xrun,
    RealtimeDenied,
}

impl FatalReason {
    fn from_kind(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::DeviceChanged => Self::RouteChanged,
            ErrorKind::Xrun => Self::Xrun,
            ErrorKind::RealtimeDenied => Self::RealtimeDenied,
            ErrorKind::DeviceBusy => Self::DeviceBusy,
            ErrorKind::DeviceNotAvailable => Self::DeviceNotAvailable,
            ErrorKind::HostUnavailable => Self::HostUnavailable,
            ErrorKind::InvalidInput => Self::InvalidInput,
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            ErrorKind::ResourceExhausted => Self::ResourceExhausted,
            ErrorKind::StreamInvalidated => Self::StreamInvalidated,
            ErrorKind::UnsupportedConfig => Self::UnsupportedConfig,
            ErrorKind::UnsupportedOperation => Self::UnsupportedOperation,
            ErrorKind::BackendError => Self::BackendError,
            _ => Self::Unclassified,
        }
    }

    fn message(code: u8) -> &'static str {
        match code {
            value if value == Self::RouteChanged as u8 => "strict audio route changed",
            value if value == Self::Xrun as u8 => "strict audio host reported an xrun",
            value if value == Self::RealtimeDenied as u8 => "strict audio scheduling was denied",
            value if value == Self::DeviceBusy as u8 => "device is busy",
            value if value == Self::DeviceNotAvailable as u8 => "device is no longer available",
            value if value == Self::HostUnavailable as u8 => "audio host is unavailable",
            value if value == Self::InvalidInput as u8 => "invalid audio input or configuration",
            value if value == Self::PermissionDenied as u8 => "audio device access was denied",
            value if value == Self::ResourceExhausted as u8 => "audio resources were exhausted",
            value if value == Self::StreamInvalidated as u8 => "audio stream must be rebuilt",
            value if value == Self::UnsupportedConfig as u8 => "audio configuration is unsupported",
            value if value == Self::UnsupportedOperation as u8 => "audio operation is unsupported",
            value if value == Self::BackendError as u8 => "audio backend reported an error",
            _ => "unclassified audio device error",
        }
    }
}

#[cfg(test)]
#[path = "play_device_status_tests.rs"]
mod tests;
