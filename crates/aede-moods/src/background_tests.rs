use super::*;
#[test]
fn foreground_and_stop_preempt_idle_compute_even_when_budget_remains() {
    assert_eq!(admission(false, false, Duration::ZERO), Admission::Work);
    assert_eq!(
        admission(false, true, Duration::ZERO),
        Admission::Foreground
    );
    assert_eq!(admission(true, true, Duration::ZERO), Admission::Stop);
    assert_eq!(
        admission(false, false, Duration::from_millis(5)),
        Admission::Yield
    );
}
