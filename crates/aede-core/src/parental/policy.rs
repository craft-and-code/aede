//! Strict, bounded parental policy independent of HTTP and account credentials.
use super::feeds::canonical_feed;
use crate::model::EntityKind;
use crate::user::EntityRef;
use serde::{Deserialize, Serialize};

/// One half-open local-clock window, split at midnight when necessary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    /// Inclusive minute after midnight, from 0 through 1439.
    pub start_minute: u16,
    /// Exclusive minute after midnight, from 1 through 1440.
    pub end_minute: u16,
}
/// Listening windows and optional budget for one weekday.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayPolicy {
    /// At most eight non-overlapping windows; empty means no listening.
    pub windows: Vec<Window>,
    /// Optional maximum seconds; zero means no listening.
    pub limit_seconds: Option<u32>,
}
impl Default for DayPolicy {
    fn default() -> Self {
        Self {
            windows: vec![Window {
                start_minute: 0,
                end_minute: 1440,
            }],
            limit_seconds: None,
        }
    }
}
/// Administrator-owned restrictions for one stable account owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Disabled policies retain their settings but do not constrain listening.
    pub enabled: bool,
    /// IANA time-zone name used for civil dates, weeks and clock windows.
    pub timezone: String,
    /// Canonical, unfederated artist references, retained across catalogue scans.
    pub blocked_artists: Vec<String>,
    /// Whether independent radio listening is permitted.
    pub radio_allowed: bool,
    /// Whether podcasts are available; older policies retain their allowed default.
    #[serde(default = "allow_podcasts")]
    pub podcasts_allowed: bool,
    /// Canonical RSS addresses whose episodes are refused.
    pub blocked_podcasts: Vec<String>,
    /// Monday first, Sunday last.
    pub days: [DayPolicy; 7],
    /// Optional maximum seconds in the local Monday-to-Monday week.
    pub week_limit_seconds: Option<u32>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            timezone: "UTC".into(),
            blocked_artists: Vec::new(),
            radio_allowed: true,
            podcasts_allowed: true,
            blocked_podcasts: Vec::new(),
            days: std::array::from_fn(|_| DayPolicy::default()),
            week_limit_seconds: None,
        }
    }
}
fn allow_podcasts() -> bool {
    true
}
impl Policy {
    /// Validate every setting without accepting an unknown zone or excess budget.
    pub fn validate(&self) -> Result<(), String> {
        if self.timezone.len() > 128 || jiff::tz::TimeZone::get(&self.timezone).is_err() {
            return Err("invalid parental time zone".into());
        }
        if self.blocked_artists.len() > 256
            || self.blocked_podcasts.len() > 128
            || self.week_limit_seconds.is_some_and(|limit| limit > 604_800)
        {
            return Err("parental policy exceeds its bounds".into());
        }
        for token in &self.blocked_artists {
            let reference =
                EntityRef::parse_token(token).ok_or("invalid blocked artist reference")?;
            if reference.kind != EntityKind::Artist
                || reference.key.is_empty()
                || token.len() > 16_384
                || reference.key.starts_with("@phemios/")
                || token.chars().any(char::is_control)
            {
                return Err("invalid blocked artist reference".into());
            }
        }
        for feed in &self.blocked_podcasts {
            canonical_feed(feed)?;
        }
        for day in &self.days {
            if day.windows.len() > 8 || day.limit_seconds.is_some_and(|limit| limit > 86_400) {
                return Err("parental day exceeds its bounds".into());
            }
            let mut windows = day.windows.iter().collect::<Vec<_>>();
            windows.sort_by_key(|window| window.start_minute);
            let mut previous_end = 0;
            for window in windows {
                if window.start_minute >= window.end_minute
                    || window.end_minute > 1440
                    || window.start_minute < previous_end
                {
                    return Err(
                        "parental windows must be ordered, non-overlapping intervals".into(),
                    );
                }
                previous_end = window.end_minute;
            }
        }
        Ok(())
    }
    /// Canonicalize equivalent addresses and deterministic order after validation.
    pub fn canonicalize(mut self) -> Result<Self, String> {
        self.validate()?;
        for feed in &mut self.blocked_podcasts {
            *feed = canonical_feed(feed)?;
        }
        for day in &mut self.days {
            day.windows.sort_by_key(|window| window.start_minute);
        }
        self.blocked_artists.sort();
        self.blocked_artists.dedup();
        self.blocked_podcasts.sort();
        self.blocked_podcasts.dedup();
        Ok(self)
    }
}
/// The source category captured by an admitted audio transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Media {
    /// Catalogue audio; reference admission is additionally checked on its visible graph.
    Music {
        /// Optional unfederated current track reference.
        track: Option<String>,
    },
    /// An independent radio stream.
    Radio,
    /// An episode belonging to this original RSS subscription address.
    Podcast {
        /// RSS address, never the unrelated enclosure URL.
        feed: String,
    },
}
/// Revisioned policy projection without credentials or another owner's usage.
#[derive(Debug, Clone, Serialize)]
pub struct PolicySnapshot {
    /// Immutable account owner.
    pub owner: String,
    /// Monotone policy generation; absent policies start at zero.
    pub revision: u64,
    /// Stable supervising administrator; absent after the rules are disabled.
    pub locked_by: Option<String>,
    /// Complete editable policy.
    pub policy: Policy,
}
/// Current admission and conservative reserved listening budget.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// Immutable account owner.
    pub owner: String,
    /// Current policy generation.
    pub revision: u64,
    /// Current complete policy.
    pub policy: Policy,
    /// Union of settled and reserved audio in the local day, rounded upwards.
    pub day_used_seconds: u64,
    /// Union of settled and reserved audio in the local week, rounded upwards.
    pub week_used_seconds: u64,
    /// Smallest remaining day/week budget, or no limit.
    pub remaining_seconds: Option<u64>,
    /// Whether the supplied media category may start now.
    pub allowed: bool,
    /// Stable refusal code, absent on success.
    pub reason: Option<String>,
    /// Next local schedule/day boundary as Unix seconds, if relevant.
    pub next_change_at: Option<u64>,
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
