//! Tests for [`super`], the things every command shares.
//!
//! Declared there with `#[path]`, so this is still that module's own child and
//! still reaches its private items through `use super::*`.

use super::*;

fn window(offset: usize, limit: usize) -> Window {
    Window { offset, limit }
}

/// Everything shown means nothing said.
#[test]
fn a_listing_that_held_nothing_back_says_nothing() {
    assert_eq!(window_note(window(0, 50), 12, "issue"), None);
    assert_eq!(window_note(window(0, 12), 12, "issue"), None);
    assert_eq!(window_note(window(0, usize::MAX), 12, "issue"), None);
}

#[test]
fn a_page_is_offered_only_when_there_is_one() {
    // The fault, reported from a real run: `aede doctor --offset=25 --all`
    // answered "26–52 of 52 issues — --offset=52 for the next page". The
    // command had done exactly what it was asked — every issue after the
    // twenty-fifth — and the sentence under it named the one number guaranteed
    // to answer "starts past the end".
    let said = window_note(window(25, usize::MAX), 52, "issue").expect("a note");
    assert!(said.starts_with("26–52 of 52 issues"), "{said}");
    assert!(
        !said.contains("--offset=52"),
        "no next page is offered, because there is none: {said}"
    );
    assert!(
        said.contains("these are the last"),
        "and the reader is told where they are: {said}"
    );

    // A page that does exist is still offered, with the number to type.
    let said = window_note(window(0, 25), 52, "issue").expect("a note");
    assert!(said.starts_with("1–25 of 52 issues"), "{said}");
    assert!(said.contains("--offset=25 for the next page"), "{said}");
}

#[test]
fn all_is_advised_only_while_it_would_change_something() {
    // Given already, the rows are unbounded and what shaped the screen was
    // `--offset` alone: telling a reader to type an option they have typed is
    // how they learn to stop reading these lines.
    let with_limit = window_note(window(0, 25), 52, "issue").expect("a note");
    assert!(with_limit.contains("--all for every row"), "{with_limit}");

    let unbounded = window_note(window(25, usize::MAX), 52, "issue").expect("a note");
    assert!(!unbounded.contains("--all"), "{unbounded}");

    // And on the last screen it is not advice either, whatever the limit: what
    // cut this one is the offset, and lifting the limit would show the same
    // rows again. `aede artists --offset=25` on sixty artists shows 26–60 with
    // the default limit and shows 26–60 with `--all`.
    let last_page = window_note(window(45, 25), 52, "issue").expect("a note");
    assert!(last_page.starts_with("46–52 of 52 issues"), "{last_page}");
    assert!(!last_page.contains("--offset=52"), "{last_page}");
    assert!(
        !last_page.contains("--all"),
        "the only move left is back to the start: {last_page}"
    );
    assert!(last_page.contains("drop --offset"), "{last_page}");
}

#[test]
fn an_empty_screen_names_the_emptiness_it_actually_is() {
    // Two different emptinesses, and naming the wrong one sends the reader
    // looking for a page that was never there.
    let nothing = window_note(window(0, 50), 0, "issue").expect("a note");
    assert!(nothing.contains("no issue to show"), "{nothing}");
    assert!(
        !nothing.contains("--offset"),
        "a listing that matched nothing is not a paging accident: {nothing}"
    );

    let past = window_note(window(99, 50), 52, "issue").expect("a note");
    assert!(past.contains("52 issues in all"), "{past}");
    assert!(past.contains("--offset=99 starts past the end"), "{past}");
}
