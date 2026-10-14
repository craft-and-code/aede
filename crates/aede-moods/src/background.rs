//! Cooperative work admission; audio drivers never wait on the background worker.
use std::time::Duration;
/// One checkpoint action, evaluated before decoding and transforming another bounded packet.
#[derive(Debug, PartialEq, Eq)]
pub enum Admission {
    /// Continue the low-priority work quantum.
    Work,
    /// Park while foreground audio/transfers own their independent permits.
    Foreground,
    /// Yield after five milliseconds of active work, limiting one idle worker's duty cycle.
    Yield,
    /// Retire the source without publishing partial features.
    Stop,
}
/// The caller supplies captured cancellation/shutdown and foreground state; no task sleeps in audio callbacks.
pub fn admission(cancelled: bool, foreground: bool, active: Duration) -> Admission {
    if cancelled {
        Admission::Stop
    } else if foreground {
        Admission::Foreground
    } else if active >= Duration::from_millis(5) {
        Admission::Yield
    } else {
        Admission::Work
    }
}
#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
