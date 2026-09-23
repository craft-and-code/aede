use super::*;
use aede_core::model::Track;

fn track(disc: Option<u32>, number: Option<u32>) -> Track {
    Track {
        disc_no: disc,
        track_no: number,
        ..Default::default()
    }
}

#[test]
fn a_single_disc_album_shows_a_plain_number() {
    // A column of "1-01, 1-02" on every album in a library is noise: the
    // disc is worth saying only where there is more than one.
    assert_eq!(track_number(&track(Some(1), Some(7)), 1), "7");
    assert_eq!(track_number(&track(None, Some(7)), 1), "7");
    assert_eq!(track_number(&track(None, None), 1), "—");
}

#[test]
fn a_box_set_says_which_disc() {
    // Numbered 1, 2, 3, 1, 2, 3 with nothing saying which disc, the page
    // cannot be read against the object on the shelf.
    assert_eq!(track_number(&track(Some(2), Some(7)), 3), "2-07");
    // Zero-padded so the column lines up: "2-7" beside "2-11" reads as two
    // different widths of the same thing.
    assert_eq!(track_number(&track(Some(2), Some(11)), 3), "2-11");
    // A track whose disc the tags forgot belongs to the first, which is
    // where the model orders it.
    assert_eq!(track_number(&track(None, Some(3)), 2), "1-03");
    assert_eq!(track_number(&track(Some(2), None), 2), "—");
}

#[test]
fn what_was_verified_is_said_in_a_fraction_only_when_it_needs_one() {
    let reading = |seen, good, bad, stale| Reading {
        seen,
        good,
        bad,
        stale,
    };
    // The whole album, and nothing went wrong: no denominator, because
    // "12 of 12" on every page is a fraction nobody reads twice.
    assert_eq!(
        wording(&reading(12, 12, 0, 0), 12, "intact", "damaged"),
        "12 intact"
    );
    // Part of it, which is exactly the case the denominator exists for.
    assert_eq!(
        wording(&reading(9, 9, 0, 0), 12, "intact", "damaged"),
        "9 of 12 intact"
    );
    // A failure is never folded into the good count.
    assert_eq!(
        wording(&reading(12, 11, 1, 0), 12, "MD5 matches", "MD5 failed"),
        "11 MD5 matches, 1 MD5 failed"
    );
    // An answer about bytes that have changed since is neither good nor
    // bad — it is void, and says so.
    assert_eq!(
        wording(&reading(12, 10, 0, 2), 12, "MD5 matches", "MD5 failed"),
        "10 MD5 matches, 2 stale"
    );
    // A report that never opened the checksum has said nothing about
    // matching, and "0 of 12 MD5 matches" would read as twelve failures.
    assert_eq!(
        wording(&reading(12, 0, 0, 0), 12, "MD5 matches", "MD5 failed"),
        "12 analysed"
    );
    // And a count of zero is never printed: "0 MD5 matches, 1 failed" says
    // the same thing twice and buries the half that matters.
    assert_eq!(
        wording(&reading(1, 0, 1, 0), 1, "MD5 matches", "MD5 failed"),
        "1 MD5 failed"
    );
}

#[test]
fn the_summary_counts_the_discs_of_a_box_set_and_only_of_a_box_set() {
    // 4 discs is the answer to "is my rip complete?", and counting the
    // "4-xx" rows by hand to get it is work the page should have done.
    assert_eq!(
        summary(4, 85, 16_451_000, 1_500_000_000),
        "4 discs · 85 tracks · 4:34:11 · 1.5 GB"
    );
    // On a single disc the word carries nothing, and a line that always
    // reads the same stops being read at all.
    assert_eq!(
        summary(1, 9, 2_700_000, 300_000_000),
        "9 tracks · 45:00 · 300.0 MB"
    );
}
