//! Tests for [`super`], split out of `backup.rs`.
//!
//! The two commands themselves print and write, and are covered end to end
//! where that can be observed. What is worth pinning here is the handful of
//! decisions they make before anything is written: where the file comes from,
//! what an unreadable store on disk does to the rest, and what a restore is
//! about to do to each one.

use super::*;
use aede_core::user::UserData;

fn args(words: &[&str]) -> Args {
    let mut raw = vec!["backup".to_string()];
    raw.extend(words.iter().map(|w| w.to_string()));
    Args::parse(raw)
}

#[test]
fn the_file_is_named_or_the_command_refuses_and_shows_the_form() {
    // A positional rather than `--output`, because the file is not a detail of
    // this command: it is what the command is about. And a missing one is
    // refused rather than defaulted — a backup written somewhere the reader did
    // not choose is a backup they will not find.
    let given =
        named(&args(&["/tmp/aede-somewhere.json"]), "aede backup <file>").expect("the path given");
    assert_eq!(given, std::path::PathBuf::from("/tmp/aede-somewhere.json"));

    let refused = named(&args(&[]), "aede backup <file>").expect_err("no file");
    assert!(
        refused.to_string().contains("aede backup <file>"),
        "the refusal shows the form that works: {refused}"
    );
    // A quoted empty word is not a file name, and treating it as one would
    // write a backup to a path nobody can type again.
    assert!(named(&args(&["   "]), "aede backup <file>").is_err());
}

#[test]
fn a_store_that_is_not_there_and_one_that_will_not_read_are_two_things() {
    // The distinction that decides what gets written and what gets said. A data
    // folder holding notes and no catalog is perfectly ordinary; a catalog this
    // build refuses is not, and must be reported now rather than discovered by
    // whoever restores the file a year from now.
    let absent: Part<UserData> = part(Ok(None));
    assert!(matches!(absent, Part::Empty));

    let there: Part<UserData> = part(Ok(Some(UserData::default())));
    assert!(there.held().is_some());

    let broken: Part<UserData> = part(Err(store::StoreError::Version {
        found: 99,
        expected: 1,
    }));
    match &broken {
        Part::Unreadable(why) => assert!(why.contains("99"), "it keeps what was said: {why}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(
        broken.held().is_none(),
        "and it is never mistaken for something to write"
    );
}

#[test]
fn a_restore_writes_only_what_the_backup_holds_and_deletes_nothing() {
    // The promise the command makes before asking for confirmation. A backup
    // made before anything was fetched carries no `sources.json`, and treating
    // that as "there should be none" would silently throw away a layer that
    // took twenty minutes of polite requests to build.
    let held: Part<UserData> = Part::Held(UserData {
        set_aside: vec![aede_core::user::SetAside {
            owner: aede_core::user::LOCAL_USER.to_string(),
            release_group: "g2".to_string(),
            title: "Bitches Brew".to_string(),
            created_at: 1,
        }],
        ..Default::default()
    });
    match state(&held, user_of, "not in this backup") {
        // The zeros are left out: a reader checking that their notes are in
        // the file should not have to find the one number that is not a zero.
        Doing::Write(what) => assert_eq!(what, "1 record set aside"),
        Doing::Skip(why) => panic!("there is something to write: {why}"),
    }
    assert_eq!(user_of(&UserData::default()), "nothing written yet");

    let empty: Part<UserData> = Part::Empty;
    match state(&empty, user_of, "not in this backup") {
        Doing::Skip(why) => assert_eq!(why, "not in this backup"),
        Doing::Write(_) => panic!("nothing to write"),
    }

    // Never silent: a store that will not read is the one thing here somebody
    // must be told about now rather than discover at the moment they most need
    // the file.
    let broken: Part<UserData> = Part::Unreadable("catalog in version 99".to_string());
    match state(&broken, user_of, "not in this backup") {
        Doing::Skip(why) => assert!(why.contains("version 99"), "{why}"),
        Doing::Write(_) => panic!("this build cannot write what it cannot read"),
    }
}

#[test]
fn both_commands_describe_one_store_in_one_set_of_words() {
    // Written twice, the two lists would have drifted the first time a field
    // was added to one of the stores — the same reason the role vocabulary is
    // one table read in both directions. Only the words for "there is none"
    // differ, because they mean different things on the way out and on the way
    // in.
    let held = Backup {
        made_at: 1,
        made_by: "0.1.0".to_string(),
        catalog: Part::Empty,
        user: Part::Held(UserData::default()),
        sources: Part::Empty,
    };
    let out = summarise(&held, "nothing here to save");
    let back = summarise(&held, "not in this backup");

    let names: Vec<&str> = out.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, vec!["catalog", "what you said", "what sources said"]);
    assert_eq!(
        names,
        back.iter().map(|(name, _)| *name).collect::<Vec<&str>>(),
        "the same three, in the same order"
    );

    match (&out[1].1, &back[1].1) {
        (Doing::Write(one), Doing::Write(two)) => assert_eq!(one, two, "and described alike"),
        _ => panic!("the store that is held is written either way"),
    }
    match (&out[0].1, &back[0].1) {
        (Doing::Skip(one), Doing::Skip(two)) => assert_ne!(
            one, two,
            "and the absence is worded for the direction it is absent in"
        ),
        _ => panic!("an empty store is written by neither"),
    }
}
