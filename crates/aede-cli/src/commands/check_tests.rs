//! Tests for [`super`], split out of `check.rs`.
//!
//! About the wording of the interruption hint, which is where the previous
//! message went wrong: it claimed a continuous save without naming the batch
//! size, which is misleading — an interrupt mid-batch loses that whole batch,
//! not "at most the current file". The end-to-end behaviour (a batch actually
//! surviving a restart) is proved against the real binary elsewhere; this
//! guards the sentence a reader decides whether to trust.

use super::*;

#[test]
fn interruption_hint_names_the_batch_size() {
    let hint = interruption_hint();
    assert!(
        hint.contains(&SAVE_EVERY.to_string()),
        "the hint must name the actual batch size, not just claim safety: {hint}"
    );
}

#[test]
fn interruption_hint_does_not_overclaim_continuous_saving() {
    // The old wording ("saved as the run goes") read as per-file safety, which
    // is not what the code does: a batch is committed as a unit.
    let hint = interruption_hint();
    assert!(
        !hint.contains("as the run goes"),
        "must not suggest saving happens continuously rather than per batch: {hint}"
    );
}
