//! IANA civil boundaries; durations remain UTC intervals across clock changes.
use super::Policy;
use jiff::{Timestamp, civil::Date, tz::TimeZone};

pub(super) struct Clock {
    pub(super) day: usize,
    pub(super) day_start: u64,
    pub(super) day_end: u64,
    pub(super) week_start: u64,
    pub(super) week_end: u64,
    pub(super) scheduled: bool,
    pub(super) next: u64,
}
fn stamp(value: Timestamp) -> Result<u64, String> {
    u64::try_from(value.as_second())
        .map_err(|_| "parental calendar is outside supported dates".into())
}
fn midnight(date: Date, zone: &TimeZone) -> Result<u64, String> {
    stamp(
        date.to_zoned(zone.clone())
            .map_err(|_| "invalid parental civil date")?
            .timestamp(),
    )
}
fn scheduled(policy: &Policy, now: u64, zone: &TimeZone) -> Result<bool, String> {
    let time = Timestamp::from_second(i64::try_from(now).map_err(|_| "invalid parental clock")?)
        .map_err(|_| "invalid parental clock")?
        .to_zoned(zone.clone());
    let minute = u16::try_from(time.hour()).map_err(|_| "invalid parental hour")? * 60
        + u16::try_from(time.minute()).map_err(|_| "invalid parental minute")?;
    Ok(policy.days[time.weekday().to_monday_zero_offset() as usize]
        .windows
        .iter()
        .any(|window| window.start_minute <= minute && minute < window.end_minute))
}
pub(super) fn clock(policy: &Policy, now: u64) -> Result<Clock, String> {
    let zone = TimeZone::get(&policy.timezone).map_err(|_| "invalid parental time zone")?;
    let timestamp =
        Timestamp::from_second(i64::try_from(now).map_err(|_| "invalid parental clock")?)
            .map_err(|_| "invalid parental clock")?;
    let local = timestamp.to_zoned(zone.clone());
    let date = local.date();
    let day = local.weekday().to_monday_zero_offset() as usize;
    let day_start = midnight(date, &zone)?;
    let day_end = midnight(
        date.tomorrow().map_err(|_| "invalid parental civil date")?,
        &zone,
    )?;
    let mut monday = date;
    for _ in 0..day {
        monday = monday.yesterday().map_err(|_| "invalid parental week")?;
    }
    let week_start = midnight(monday, &zone)?;
    for _ in 0..7 {
        monday = monday.tomorrow().map_err(|_| "invalid parental week")?;
    }
    let week_end = midnight(monday, &zone)?;
    let mut next = day_end;
    // Both sides of a fold matter. A missing wall-clock boundary is only
    // admitted if actual clock membership changes there; zone transitions
    // independently cover jumps into/out of a window in the middle of a gap.
    for window in &policy.days[day].windows {
        for minute in [window.start_minute, window.end_minute] {
            let (date, minute) = if minute == 1440 {
                (date.tomorrow().map_err(|_| "invalid parental day")?, 0)
            } else {
                (date, minute)
            };
            let value = date.at((minute / 60) as i8, (minute % 60) as i8, 0, 0);
            for candidate in [
                zone.to_ambiguous_timestamp(value).earlier(),
                zone.to_ambiguous_timestamp(value).later(),
            ]
            .into_iter()
            .flatten()
            {
                let candidate = stamp(candidate)?;
                if now < candidate
                    && candidate < next
                    && scheduled(policy, candidate - 1, &zone)?
                        != scheduled(policy, candidate, &zone)?
                {
                    next = candidate;
                }
            }
        }
    }
    if let Some(transition) = zone.following(timestamp).next() {
        let candidate = stamp(transition.timestamp())?;
        if now < candidate
            && candidate < next
            && scheduled(policy, candidate - 1, &zone)? != scheduled(policy, candidate, &zone)?
        {
            next = candidate;
        }
    }
    Ok(Clock {
        day,
        day_start,
        day_end,
        week_start,
        week_end,
        scheduled: scheduled(policy, now, &zone)?,
        next,
    })
}
#[cfg(test)]
#[path = "schedule_tests.rs"]
mod tests;
