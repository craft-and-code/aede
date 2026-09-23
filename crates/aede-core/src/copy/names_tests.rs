use super::*;

#[test]
fn a_name_a_player_accepts_is_left_alone() {
    // Adapting a name that did not need it would make the copy differ from
    // the library for no reason, and `None` is what tells the caller there
    // is nothing to report.
    for name in [
        "01 Crazy Train.flac",
        "Blizzard of Ozz",
        "Café Bleu",
        "AC-DC",
        "folder.jpg",
    ] {
        assert_eq!(adapt(name), None, "{name} needed no change");
    }
}

#[test]
fn the_punctuation_a_card_refuses_is_replaced() {
    assert_eq!(
        adapt("Where Is My Mind?.flac").as_deref(),
        Some("Where Is My Mind_.flac")
    );
    assert_eq!(
        adapt("Symphony No. 5: Allegro.flac").as_deref(),
        Some("Symphony No. 5_ Allegro.flac")
    );
    assert_eq!(adapt("AC/DC"), None, "the separator never reaches here");
    assert_eq!(
        adapt("a\"b<c>d|e*f.flac").as_deref(),
        Some("a_b_c_d_e_f.flac")
    );
}

#[test]
fn a_trailing_dot_or_space_is_taken_off_rather_than_replaced() {
    // The call succeeds and the name comes back without it, so the copy
    // can never find the file it just wrote. Trimmed, because a trailing
    // space carries nothing anyone wants to keep.
    assert_eq!(adapt("Vol. 2 ").as_deref(), Some("Vol. 2"));
    assert_eq!(adapt("Greatest Hits.").as_deref(), Some("Greatest Hits"));
    assert_eq!(adapt("...").as_deref(), Some("_"));
}

#[test]
fn a_device_name_is_refused_whatever_the_extension() {
    // `NUL.flac` cannot be created on a FAT volume: the reserved word is
    // the stem, not the whole name.
    assert_eq!(adapt("NUL.flac").as_deref(), Some("NUL_.flac"));
    assert_eq!(adapt("con.mp3").as_deref(), Some("con_.mp3"));
    assert_eq!(adapt("COM1").as_deref(), Some("COM1_"));
    assert_eq!(adapt("Nullify.flac"), None, "only the exact word");
}

#[test]
fn an_overlong_name_keeps_its_extension() {
    // Cutting the tail would take the extension with it, and a player that
    // sorts by extension would lose the track entirely.
    let long = format!("{}.flac", "a".repeat(300));
    let adapted = adapt(&long).expect("too long to write");
    assert!(adapted.len() <= MAX_COMPONENT, "{}", adapted.len());
    assert!(adapted.ends_with(".flac"), "{adapted}");
}

#[test]
fn a_name_is_cut_on_a_character_and_not_in_the_middle_of_one() {
    // Every "é" is two bytes: a limit counted in bytes lands inside one
    // every other time, and the result would not be a string at all.
    for extra in 0..4 {
        let long = format!("{}{}.flac", "é".repeat(200), "x".repeat(extra));
        let adapted = adapt(&long).expect("too long");
        assert!(adapted.len() <= MAX_COMPONENT);
        assert!(adapted.ends_with(".flac"));
    }
}

#[test]
fn not_every_dot_ends_a_name() {
    // `Vol. 1: Live` holds a dot that introduces no extension. Taking the
    // last one regardless made the stem `Vol` and the extension ` 1_ Live`,
    // and the counter landed inside the title: `Vol (2). 1_ Live`.
    assert_eq!(split_extension("Vol. 1_ Live"), ("Vol. 1_ Live", ""));
    assert_eq!(
        split_extension("01 Crazy Train.flac"),
        ("01 Crazy Train", ".flac")
    );
    assert_eq!(split_extension("no dot at all"), ("no dot at all", ""));
    assert_eq!(split_extension(".hidden"), (".hidden", ""));
    // A long run after the dot is a sentence, not an extension.
    assert_eq!(
        split_extension("Symphony No. 5 in C minor"),
        ("Symphony No. 5 in C minor", "")
    );
}

#[test]
fn two_names_that_became_one_are_told_apart() {
    // "Vol. 1: Live" and "Vol. 1? Live" both adapt to the same string, and
    // a copy that merged two albums into one folder would be worse than
    // one that refused.
    let mut taken = BTreeSet::new();
    assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live");
    assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (2)");
    assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (3)");
    // The counter goes before the extension, so the file stays playable.
    assert_eq!(make_unique("a.flac", &mut taken), "a.flac");
    assert_eq!(make_unique("a.flac", &mut taken), "a (2).flac");
    // And a title whose own dot is not an extension keeps its shape: the
    // counter goes at the end, not inside it. Three are already taken
    // above, so this one is the fourth.
    assert_eq!(make_unique("Vol. 1_ Live", &mut taken), "Vol. 1_ Live (4)");
}
