use super::*;

fn split(words: &[&str]) -> (String, Vec<String>) {
    let rest: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
    let (name, labels) = split_tags(&rest);
    (name.join(" "), labels)
}

#[test]
fn a_comma_is_what_says_where_the_name_stops() {
    // The three shapes a user may reasonably type, and they must all mean
    // the same thing. Tested here rather than through the binary because
    // the fixture library cannot tell them apart: every one of them ends up
    // tagging *something*, and only the split says what was named and what
    // was labelled.
    let expected = vec!["music".to_string(), "ep".to_string(), "record".to_string()];
    assert_eq!(
        split(&["Scream", "music,ep,record"]),
        ("Scream".into(), expected.clone()),
        "commas inside one word"
    );
    assert_eq!(
        split(&["Scream", "music,", "ep,", "record"]),
        ("Scream".into(), expected.clone()),
        "a space after each comma"
    );
    assert_eq!(
        split(&["Scream", "music", ",", "ep", ",", "record"]),
        ("Scream".into(), expected),
        "a comma standing on its own"
    );
}

#[test]
fn the_shape_that_already_worked_still_means_what_it_did() {
    // `aede tag album Kind of Blue jazz` predates the list entirely. A new
    // reading that changed an old one would be a regression dressed as a
    // feature.
    assert_eq!(
        split(&["Kind", "of", "Blue", "jazz"]),
        ("Kind of Blue".into(), vec!["jazz".to_string()])
    );
    assert_eq!(
        split(&["Legion", "vinyl"]),
        ("Legion".into(), vec!["vinyl".to_string()])
    );
}

#[test]
fn a_name_alone_leaves_nothing_before_the_labels() {
    // Nothing stands before "Scream", and a target is mandatory — so the
    // caller reads the tail as the name rather than as a label with nothing
    // to put it on. That is what makes `--remove` with only a name mean
    // "every tag", and it is decided in the caller, not here: this function
    // reports the split it found and does not guess at intent.
    assert_eq!(split(&["Scream"]), ("".into(), vec!["Scream".to_string()]));
    assert_eq!(split(&[]), ("".into(), Vec::new()));
}

#[test]
fn a_trailing_comma_does_not_invent_an_empty_tag() {
    // A list typed with a trailing separator is a slip, not a request for a
    // tag whose name is nothing — and an empty tag would be invisible on
    // every screen that shows it while still matching `tag:` in a query.
    assert_eq!(
        split(&["Scream", "music,", "ep,"]),
        ("Scream".into(), vec!["music".to_string(), "ep".to_string()])
    );
    assert_eq!(
        split(&["Scream", "music,,ep"]),
        ("Scream".into(), vec!["music".to_string(), "ep".to_string()])
    );
}

#[test]
fn a_label_may_hold_a_space() {
    // "to rip again" is the example in the model's own doc comment, so it
    // had better survive a list.
    assert_eq!(
        split(&["Scream", "to rip again,", "vinyl"]),
        (
            "Scream".into(),
            vec!["to rip again".to_string(), "vinyl".to_string()]
        )
    );
}

#[test]
fn a_list_is_read_out_the_way_it_is_said() {
    assert_eq!(quoted(&["a".to_string()]), "\"a\"");
    assert_eq!(
        quoted(&["a".to_string(), "b".to_string()]),
        "\"a\" and \"b\""
    );
    assert_eq!(
        quoted(&["a".to_string(), "b".to_string(), "c".to_string()]),
        "\"a\", \"b\" and \"c\""
    );
}
