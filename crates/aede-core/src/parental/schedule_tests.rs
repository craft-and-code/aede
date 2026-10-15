use super::super::Window;
use super::*;
fn time(value: &str) -> u64 {
    value.parse::<Timestamp>().unwrap().as_second() as u64
}
#[test]
fn local_calendar_uses_dst_days_and_monday_week() {
    let policy = Policy {
        timezone: "Europe/Paris".into(),
        ..Policy::default()
    };
    let spring = clock(&policy, time("2026-03-29T12:00:00Z")).unwrap();
    assert_eq!(spring.day_end - spring.day_start, 23 * 3600);
    assert_eq!(spring.day, 6);
    let autumn = clock(&policy, time("2026-10-25T12:00:00Z")).unwrap();
    assert_eq!(autumn.day_end - autumn.day_start, 25 * 3600);
    let monday = clock(&policy, time("2026-10-26T12:00:00Z")).unwrap();
    assert_eq!(monday.day, 0);
    assert_eq!(monday.week_start, monday.day_start);
}
#[test]
fn repeated_hours_have_two_actual_window_boundaries() {
    let mut policy = Policy {
        timezone: "Europe/Paris".into(),
        ..Policy::default()
    };
    policy.days[6].windows = vec![Window {
        start_minute: 150,
        end_minute: 165,
    }];
    let first = clock(&policy, time("2026-10-25T00:35:00Z")).unwrap();
    assert!(first.scheduled);
    assert_eq!(first.next, time("2026-10-25T00:45:00Z"));
    let second = clock(&policy, time("2026-10-25T01:35:00Z")).unwrap();
    assert!(second.scheduled);
    assert_eq!(second.next, time("2026-10-25T01:45:00Z"));
}
#[test]
fn missing_clock_hour_never_invents_a_listening_window() {
    let mut policy = Policy {
        timezone: "Europe/Paris".into(),
        ..Policy::default()
    };
    policy.days[6].windows = vec![Window {
        start_minute: 130,
        end_minute: 150,
    }];
    let value = clock(&policy, time("2026-03-29T01:15:00Z")).unwrap();
    assert!(!value.scheduled);
}
