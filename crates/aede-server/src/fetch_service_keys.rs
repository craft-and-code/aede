//! Bounded per-job credentials for trusted loopback administrators, never public task data.
use serde::Deserialize;

/// Explicit client-owned keys override the corresponding server environment only for this job.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FetchServiceKeys {
    /// Fanart.tv application key; absent retains the server's configured key.
    pub fanarttv: Option<String>,
    /// AcoustID application key; absent retains the server's configured key.
    pub acoustid: Option<String>,
}
impl std::fmt::Debug for FetchServiceKeys {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FetchServiceKeys")
            .field("fanarttv_configured", &self.fanarttv.is_some())
            .field("acoustid_configured", &self.acoustid.is_some())
            .finish()
    }
}
impl FetchServiceKeys {
    pub(super) fn valid(&self) -> bool {
        [&self.fanarttv, &self.acoustid].into_iter().all(|value| {
            value.as_ref().is_none_or(|key| {
                !key.trim().is_empty() && key.len() <= 1024 && !key.chars().any(char::is_control)
            })
        })
    }
}
#[cfg(test)]
#[path = "fetch_service_keys_tests.rs"]
mod tests;
