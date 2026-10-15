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
fn renewing_one_voice_does_not_remove_another_expiring_voices_identity() {
    let mut store = controlled(100);
    let at = now();
    for character in ['a', 'b'] {
        store
            .reserve("listener", &id(character), Media::Radio, 10, at)
            .unwrap();
    }
    store
        .renew("listener", &id('a'), 10_000, 10, at + 10)
        .unwrap();
    let grant = store
        .renew("listener", &id('b'), 10_000, 10, at + 10)
        .expect("another voice's renewal must not invalidate this admitted lease");
    assert_eq!(grant.expires_at, at + 20);
    assert_eq!(
        store
            .status("listener", at + 10, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        20
    );
}

#[test]
fn expired_lease_slots_are_reusable_without_refunding_their_listening() {
    let mut store = controlled(100);
    let at = now();
    for number in 0..8 {
        store
            .reserve("listener", &format!("{number:064x}"), Media::Radio, 10, at)
            .unwrap();
    }
    for number in 8..16 {
        store
            .reserve(
                "listener",
                &format!("{number:064x}"),
                Media::Radio,
                10,
                at + 10,
            )
            .unwrap();
    }
    assert!(
        store
            .reserve("listener", &id('f'), Media::Radio, 10, at + 10)
            .is_err()
    );
    assert_eq!(
        store
            .status("listener", at + 10, &Media::Radio)
            .unwrap()
            .day_used_seconds,
        20
    );
}

#[test]
fn expired_lease_identities_outside_usage_retention_are_forgotten() {
    let mut store = controlled(100);
    let at = now();
    store
        .reserve("listener", &id('a'), Media::Radio, 10, at)
        .unwrap();
    let later = at + 33 * 86_400;
    store
        .reserve("listener", &id('b'), Media::Radio, 10, later)
        .unwrap();
    assert!(store.renew("listener", &id('a'), 0, 10, later).is_err());
    assert_eq!(
        store
            .status("listener", later, &Media::Radio)
            .unwrap()
            .week_used_seconds,
        10
    );
}

#[test]
fn a_full_retention_window_keeps_a_transmitted_prebuffer_when_the_lease_closes() {
    let mut store = ParentalStore::default();
    store
        .set_policy(
            "listener",
            Policy {
                enabled: true,
                ..Policy::default()
            },
            0,
        )
        .unwrap();
    let at = now();
    let now_ms = at * 1000;
    let start_ms = now_ms - super::super::storage::RETAIN_MS;
    store.owners.get_mut("listener").unwrap().settled = vec![Interval {
        start_ms,
        end_ms: now_ms,
    }];
    store.validate().unwrap();
    for transmitted_ms in [1000, 10_000] {
        let mut active = store.clone();
        active
            .reserve("listener", &id('a'), Media::Radio, 10, at)
            .unwrap();
        active
            .settle("listener", &id('a'), transmitted_ms, at, true)
            .unwrap();
        let retained = &active.owners["listener"].settled;
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].start_ms, start_ms + transmitted_ms);
        assert_eq!(retained[0].end_ms, now_ms + transmitted_ms);
        assert_eq!(
            retained[0].end_ms - retained[0].start_ms,
            super::super::storage::RETAIN_MS
        );
        let restored = super::super::from_json(&super::super::to_json(&active)).unwrap();
        assert_eq!(
            restored
                .status("listener", at, &Media::Radio)
                .unwrap()
                .day_used_seconds,
            12 * 3600 + transmitted_ms / 1000
        );
    }
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
