use super::super::{Policy, Window};
use super::*;
fn now() -> u64 {
    "2026-10-12T12:00:00Z"
        .parse::<jiff::Timestamp>()
        .unwrap()
        .as_second() as u64
}
fn id(character: char) -> String {
    character.to_string().repeat(64)
}
fn controlled(limit: u32) -> ParentalStore {
    let mut store = ParentalStore::default();
    let mut policy = Policy {
        enabled: true,
        week_limit_seconds: Some(30),
        ..Policy::default()
    };
    policy.days[0].limit_seconds = Some(limit);
    store.set_policy("listener", policy, 0).unwrap();
    store
}
#[test]
fn concurrent_fade_voices_share_quota_and_refund_only_their_own_coverage() {
    let mut store = controlled(15);
    let at = now();
    store
        .reserve("listener", &id('a'), Media::Radio, 10, at)
        .unwrap();
    store
        .reserve("listener", &id('b'), Media::Radio, 10, at)
        .unwrap();
    assert_eq!(
        store
            .status("listener", at, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        10
    );
    store
        .settle("listener", &id('a'), 4000, at + 4, true)
        .unwrap();
    assert_eq!(
        store
            .status("listener", at + 4, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        10
    );
    store
        .settle("listener", &id('b'), 6000, at + 6, true)
        .unwrap();
    assert_eq!(
        store
            .status("listener", at + 6, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        6
    );
    let grant = store
        .reserve("listener", &id('c'), Media::Radio, 10, at + 6)
        .unwrap();
    assert_eq!(grant.granted_seconds, 9);
    assert_eq!(
        store
            .status("listener", at + 6, &Media::Radio)
            .unwrap()
            .reason
            .as_deref(),
        Some("daily_limit")
    );
    assert!(
        store
            .check_schedule("listener", at + 6, &Media::Radio)
            .is_ok()
    );
}
#[test]
fn renewal_refunds_unused_tail_before_reserving_the_next_slice() {
    let mut store = controlled(20);
    let at = now();
    store
        .reserve("listener", &id('a'), Media::Radio, 10, at)
        .unwrap();
    let grant = store.renew("listener", &id('a'), 8000, 10, at + 8).unwrap();
    assert_eq!(grant.expires_at, at + 18);
    store
        .settle("listener", &id('a'), 10_000, at + 18, true)
        .unwrap();
    assert_eq!(
        store
            .status("listener", at + 18, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        18
    );
    assert_eq!(
        store
            .reserve("listener", &id('a'), Media::Radio, 10, at + 18)
            .unwrap()
            .granted_seconds,
        2
    );
}
#[test]
fn lease_caps_monotone_counters_and_category_policy_are_enforced() {
    let mut store = controlled(100);
    let at = now();
    for character in 'a'..='h' {
        let id = if character > 'f' {
            format!(
                "{}{}",
                "f".repeat(63),
                if character == 'g' { '0' } else { '1' }
            )
        } else {
            id(character)
        };
        store
            .reserve("listener", &id, Media::Radio, 10, at)
            .unwrap();
    }
    assert!(
        store
            .reserve("listener", &id('0'), Media::Radio, 10, at)
            .is_err()
    );
    store
        .settle("listener", &id('a'), 1000, at + 1, false)
        .unwrap();
    assert!(store.settle("listener", &id('a'), 0, at + 1, true).is_err());
    assert!(
        store
            .settle("listener", &id('a'), 11_000, at + 1, true)
            .is_err()
    );
    let mut policy = store.policy("listener").policy;
    policy.radio_allowed = false;
    policy.blocked_podcasts = vec!["https://feeds.example.test/show".into()];
    store.set_policy("listener", policy, 1).unwrap();
    assert_eq!(
        store
            .check_schedule("listener", at, &Media::Radio)
            .unwrap_err(),
        "radio_blocked"
    );
    assert_eq!(
        store
            .check_schedule(
                "listener",
                at,
                &Media::Podcast {
                    feed: "HTTPS://FEEDS.EXAMPLE.TEST:443/show#episode".into()
                }
            )
            .unwrap_err(),
        "podcast_blocked"
    );
    assert!(
        store
            .check_schedule(
                "listener",
                at,
                &Media::Podcast {
                    feed: "https://feeds.example.test/another".into()
                }
            )
            .is_ok()
    );
}
#[test]
fn grants_stop_at_window_end_and_weekly_limits_survive_a_new_day() {
    let mut store = controlled(100);
    let at = now() + 57;
    let mut policy = store.policy("listener").policy;
    policy.days[0].windows = vec![Window {
        start_minute: 720,
        end_minute: 721,
    }];
    policy.week_limit_seconds = Some(3);
    store.set_policy("listener", policy, 1).unwrap();
    assert_eq!(
        store
            .reserve("listener", &id('a'), Media::Radio, 10, at)
            .unwrap()
            .granted_seconds,
        3
    );
    store
        .settle("listener", &id('a'), 3000, at + 3, true)
        .unwrap();
    assert_eq!(
        store
            .status("listener", at + 86_400, &Media::Radio)
            .unwrap()
            .reason
            .as_deref(),
        Some("weekly_limit")
    );
    assert_eq!(
        store
            .status("listener", now() + 7 * 86_400, &Media::Radio)
            .unwrap()
            .week_used_seconds,
        0
    );
}
#[test]
fn closing_with_a_false_zero_counter_cannot_refund_elapsed_activity() {
    let mut store = controlled(15);
    let at = now();
    store
        .reserve("listener", &id('a'), Media::Radio, 10, at)
        .unwrap();
    store.settle("listener", &id('a'), 0, at + 8, true).unwrap();
    assert_eq!(
        store
            .status("listener", at + 8, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        8
    );
    assert_eq!(
        store
            .reserve("listener", &id('b'), Media::Radio, 10, at + 8)
            .unwrap()
            .granted_seconds,
        7
    );
}
