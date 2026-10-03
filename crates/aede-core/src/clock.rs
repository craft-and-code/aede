//! Unix timestamps shared by catalog and conclusion stores.
//!
//! Small enough to be tempting to rewrite on the spot — which is exactly what
//! had happened, in three places, each with its own answer to "and if the clock
//! is before 1970?". A scan date, an integrity date and an import date have to
//! be comparable, so they come from here.

use std::time::{SystemTime, UNIX_EPOCH};

/// Now, in seconds since the Unix epoch.
///
/// A clock set before 1970 yields `0` rather than an error: nothing in the
/// catalog can act on that failure, and a date of zero reads as "unknown"
/// everywhere a date is shown.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Now with subsecond precision, for ordering analysis results produced in
/// quick succession. File identity stores seconds and the fractional part
/// separately so both integers survive the JSON representation exactly.
pub fn now_nanoseconds() -> u64 {
    nanoseconds(SystemTime::now())
}

/// A report file's modification time, with the same precision as new results.
pub fn mtime_nanoseconds(meta: &std::fs::Metadata) -> u64 {
    meta.modified().map(nanoseconds).unwrap_or(0)
}

fn nanoseconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

/// Modification date of a file, in seconds since the Unix epoch.
///
/// The scanner also compares [`mtime_subseconds`]. Keeping the parts separate
/// avoids rounding epoch nanoseconds through JSON's floating-point numbers.
pub fn mtime_seconds(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Nanosecond fraction of a file's modification date, or `0` when unavailable.
pub fn mtime_subseconds(meta: &std::fs::Metadata) -> u32 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0)
}
