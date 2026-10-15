//! Short durable reservations count overlapping transports once per account.
use super::intervals::{Interval, duration, union};
use super::storage::{Owner, Reservation, valid_lease, valid_owner};
use super::{Media, ParentalStore, Status};
use serde::Serialize;

/// A bounded server-generated listening reservation, never an authentication token.
#[derive(Debug, Clone, Serialize)]
pub struct LeaseGrant {
    /// Opaque identifier supplied by the trusted server's secure generator.
    pub lease_id: String,
    /// Policy generation used for this grant.
    pub revision: u64,
    /// Exclusive expiry in Unix seconds.
    pub expires_at: u64,
    /// Granted duration, between one and ten seconds.
    pub granted_seconds: u32,
}
fn milliseconds(now: u64) -> Result<u64, String> {
    now.checked_mul(1000)
        .ok_or_else(|| "invalid parental clock".into())
}
fn category(policy: &super::Policy, media: &Media) -> Result<Option<&'static str>, String> {
    match media {
        Media::Radio if !policy.radio_allowed => Ok(Some("radio_blocked")),
        Media::Podcast { .. } if !policy.podcasts_allowed => Ok(Some("podcast_blocked")),
        Media::Podcast { feed } => {
            let feed = super::canonical_feed(feed)?;
            Ok(policy
                .blocked_podcasts
                .iter()
                .any(|blocked| super::canonical_feed(blocked).is_ok_and(|value| value == feed))
                .then_some("podcast_blocked"))
        }
        Media::Music { track: Some(token) } => {
            let reference = crate::user::EntityRef::parse_token(token)
                .ok_or("invalid parental audio reference")?;
            if reference.kind != crate::model::EntityKind::Track
                || reference.key.is_empty()
                || token.len() > 16_384
                || token.chars().any(char::is_control)
            {
                return Err("invalid parental audio reference".into());
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}
impl ParentalStore {
    /// Recheck current category and clock windows without spending an existing grant twice.
    pub fn check_schedule(&self, owner: &str, now: u64, media: &Media) -> Result<(), String> {
        if !valid_owner(owner) {
            return Err("invalid parental owner".into());
        }
        let fallback = Owner::default();
        let row = self.owners.get(owner).unwrap_or(&fallback);
        if !row.policy.enabled {
            return Ok(());
        }
        if let Some(reason) = category(&row.policy, media)? {
            return Err(reason.into());
        }
        if !super::schedule::clock(&row.policy, now)?.scheduled {
            return Err("outside_schedule".into());
        }
        Ok(())
    }

    /// Inspect only one owner's policy and reserved/settled local calendar budgets.
    pub fn status(&self, owner: &str, now: u64, media: &Media) -> Result<Status, String> {
        if !valid_owner(owner) {
            return Err("invalid parental owner".into());
        }
        let fallback = Owner::default();
        let row = self.owners.get(owner).unwrap_or(&fallback);
        let clock = super::schedule::clock(&row.policy, now)?;
        let intervals = row.intervals();
        let day_used = duration(
            &intervals,
            milliseconds(clock.day_start)?,
            milliseconds(clock.day_end)?,
        );
        let week_used = duration(
            &intervals,
            milliseconds(clock.week_start)?,
            milliseconds(clock.week_end)?,
        );
        let day_remaining = row.policy.days[clock.day].limit_seconds.map(|limit| {
            u64::from(limit)
                .saturating_mul(1000)
                .saturating_sub(day_used)
        });
        let week_remaining = row.policy.week_limit_seconds.map(|limit| {
            u64::from(limit)
                .saturating_mul(1000)
                .saturating_sub(week_used)
        });
        let reason = if !row.policy.enabled {
            None
        } else if let Some(reason) = category(&row.policy, media)? {
            Some(reason)
        } else if !clock.scheduled {
            Some("outside_schedule")
        } else if day_remaining == Some(0) {
            Some("daily_limit")
        } else if week_remaining == Some(0) {
            Some("weekly_limit")
        } else {
            None
        };
        let remaining = match (day_remaining, week_remaining) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        Ok(Status {
            owner: owner.into(),
            revision: row.revision,
            policy: row.policy.clone(),
            day_used_seconds: day_used.div_ceil(1000),
            week_used_seconds: week_used.div_ceil(1000),
            remaining_seconds: if row.policy.enabled {
                remaining.map(|value| value / 1000)
            } else {
                None
            },
            allowed: reason.is_none(),
            reason: reason.map(str::to_owned),
            next_change_at: row.policy.enabled.then_some(clock.next),
        })
    }
    /// Reserve at most ten seconds before audio is admitted. The caller saves before granting.
    /// Overlapping transports share temporal coverage rather than spending their duration twice.
    pub fn reserve(
        &mut self,
        owner: &str,
        lease_id: &str,
        media: Media,
        requested_seconds: u32,
        now: u64,
    ) -> Result<LeaseGrant, String> {
        if !valid_owner(owner) || !valid_lease(lease_id) || !(1..=10).contains(&requested_seconds) {
            return Err("invalid parental listening reservation".into());
        }
        let mut candidate = self.clone();
        let row = candidate.owners.entry(owner.into()).or_default();
        let now_ms = milliseconds(now)?;
        row.prune(now_ms);
        if row.reservations.iter().any(|value| value.id == lease_id) {
            return Err("parental listening leases are occupied".into());
        }
        row.make_room(now_ms);
        if row.reservations.len() >= 8 {
            return Err("parental listening leases are occupied".into());
        }
        let clock = super::schedule::clock(&row.policy, now)?;
        if row.policy.enabled {
            if let Some(reason) = category(&row.policy, &media)? {
                return Err(reason.into());
            }
            if !clock.scheduled {
                return Err("outside_schedule".into());
            }
        }
        let available_seconds = if row.policy.enabled {
            requested_seconds.min(u32::try_from(clock.next.saturating_sub(now)).unwrap_or(u32::MAX))
        } else {
            requested_seconds
        };
        let start_ms = milliseconds(now)?;
        let held = row.intervals();
        let day_used = duration(
            &held,
            milliseconds(clock.day_start)?,
            milliseconds(clock.day_end)?,
        );
        let week_used = duration(
            &held,
            milliseconds(clock.week_start)?,
            milliseconds(clock.week_end)?,
        );
        let mut granted = 0;
        let mut refusal = "daily_limit";
        for seconds in 1..=available_seconds {
            let end_ms = start_ms
                .checked_add(u64::from(seconds) * 1000)
                .ok_or("invalid parental reservation clock")?;
            // The candidate is inside the current day. Count only coverage
            // not already reserved/settled by another voice; no repeated
            // allocation or sorting of the retained interval ledger is needed.
            let extra = u64::from(seconds) * 1000 - duration(&held, start_ms, end_ms);
            if row.policy.enabled
                && row.policy.days[clock.day]
                    .limit_seconds
                    .is_some_and(|limit| day_used + extra > u64::from(limit) * 1000)
            {
                refusal = "daily_limit";
                break;
            }
            if row.policy.enabled
                && row
                    .policy
                    .week_limit_seconds
                    .is_some_and(|limit| week_used + extra > u64::from(limit) * 1000)
            {
                refusal = "weekly_limit";
                break;
            }
            granted = seconds;
        }
        if granted == 0 {
            return Err(refusal.into());
        }
        let grant = LeaseGrant {
            lease_id: lease_id.into(),
            revision: row.revision,
            expires_at: now
                .checked_add(u64::from(granted))
                .ok_or("invalid parental expiry")?,
            granted_seconds: granted,
        };
        row.reservations.push(Reservation {
            id: lease_id.into(),
            media,
            interval: Interval {
                start_ms,
                end_ms: milliseconds(grant.expires_at)?,
            },
            consumed_ms: 0,
        });
        candidate.validate()?;
        *self = candidate;
        Ok(grant)
    }
    /// Confirm cumulative consumed milliseconds and close/refund only future activity.
    /// Already elapsed time under an active grant is never refunded by a client's counter.
    /// Expired reservations stay fully charged; a crash or late client cannot regain their budget.
    pub fn settle(
        &mut self,
        owner: &str,
        lease_id: &str,
        consumed_ms: u64,
        now: u64,
        close: bool,
    ) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate.settle_inner(owner, lease_id, consumed_ms, now, close)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    fn settle_inner(
        &mut self,
        owner: &str,
        lease_id: &str,
        consumed_ms: u64,
        now: u64,
        close: bool,
    ) -> Result<(), String> {
        let row = self
            .owners
            .get_mut(owner)
            .ok_or("parental listening reservation is unavailable")?;
        let index = row
            .reservations
            .iter()
            .position(|value| value.id == lease_id)
            .ok_or("parental listening reservation is unavailable")?;
        let reservation = &mut row.reservations[index];
        if consumed_ms < reservation.consumed_ms
            || consumed_ms > reservation.interval.end_ms - reservation.interval.start_ms
        {
            return Err("invalid parental consumption counter".into());
        }
        let now_ms = milliseconds(now)?;
        if now_ms < reservation.interval.start_ms {
            return Err("parental clock moved backwards".into());
        }
        reservation.consumed_ms = consumed_ms;
        if close {
            let value = row.reservations.remove(index);
            let end_ms = if now_ms > value.interval.end_ms {
                value.interval.end_ms
            } else {
                // A client cannot refund time after enjoying an active grant
                // merely by reporting zero consumption. Only future time is refundable.
                value.interval.start_ms + consumed_ms.max(now_ms - value.interval.start_ms)
            };
            if end_ms > value.interval.start_ms {
                row.settled.push(Interval {
                    start_ms: value.interval.start_ms,
                    end_ms,
                });
            }
            row.settled = union(std::mem::take(&mut row.settled));
        }
        row.prune(now_ms);
        self.validate()
    }
    /// Atomically settle a previous grant and reserve the next slice under the same identifier.
    /// Consumption counters restart at zero for the returned grant.
    pub fn renew(
        &mut self,
        owner: &str,
        lease_id: &str,
        consumed_ms: u64,
        requested_seconds: u32,
        now: u64,
    ) -> Result<LeaseGrant, String> {
        let media = self
            .owners
            .get(owner)
            .and_then(|row| row.reservations.iter().find(|value| value.id == lease_id))
            .map(|value| value.media.clone())
            .ok_or("parental listening reservation is unavailable")?;
        let mut candidate = self.clone();
        candidate.settle(owner, lease_id, consumed_ms, now, true)?;
        let grant = candidate.reserve(owner, lease_id, media, requested_seconds, now)?;
        *self = candidate;
        Ok(grant)
    }
}
#[cfg(test)]
#[path = "leases_tests.rs"]
mod tests;
