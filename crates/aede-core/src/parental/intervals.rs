//! Sorted UTC interval unions, shared by overlapping PCM/radio/podcast leases.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Interval {
    pub(super) start_ms: u64,
    pub(super) end_ms: u64,
}
pub(super) fn union(mut intervals: Vec<Interval>) -> Vec<Interval> {
    intervals.retain(|interval| interval.start_ms < interval.end_ms);
    intervals.sort_by_key(|interval| (interval.start_ms, interval.end_ms));
    let mut result: Vec<Interval> = Vec::with_capacity(intervals.len());
    for interval in intervals {
        if let Some(last) = result.last_mut()
            && interval.start_ms <= last.end_ms
        {
            last.end_ms = last.end_ms.max(interval.end_ms);
        } else {
            result.push(interval);
        }
    }
    result
}
pub(super) fn duration(intervals: &[Interval], start_ms: u64, end_ms: u64) -> u64 {
    intervals
        .iter()
        .map(|interval| {
            interval
                .end_ms
                .min(end_ms)
                .saturating_sub(interval.start_ms.max(start_ms))
        })
        .sum()
}
#[cfg(test)]
#[path = "intervals_tests.rs"]
mod tests;
