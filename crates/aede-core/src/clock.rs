//! The one unit of time the catalog stores: whole seconds since the Unix epoch.
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

/// Modification date of a file, in the same unit.
///
/// Truncated to the second on purpose: it is compared against dates read back
/// from the catalog, and the catalog stores seconds. Keeping more precision
/// here would make every reloaded file look as though it had changed.
pub fn mtime_seconds(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
