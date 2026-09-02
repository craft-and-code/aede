//! Tests for [`super`], split out of `user.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model;

fn library(paths: &[&str]) -> Catalog {
    let files: Vec<_> = paths
        .iter()
        .map(|p| {
            model::tests::track(
                p,
                &[
                    ("title", "T"),
                    ("artist", "Deicide"),
                    ("album", "Legion"),
                    ("date", "1992"),
                ],
                1000,
            )
        })
        .collect();
    model::build(files, vec!["/m".into()], 0)
}

#[test]
fn a_reference_survives_the_renumbering_a_scan_does() {
    // Catalog identifiers are positions in a vector, and a scan hands them
    // out afresh. Keying anything a user wrote by one of those is how the
    // imported analyses were lost the first time.
    let first = library(&["/m/a.flac", "/m/b.flac"]);
    let reference = EntityRef::of(&first, EntityKind::Track, 1).expect("a second track");

    // The same library, walked in another order, gives other identifiers.
    let second = library(&["/m/b.flac", "/m/a.flac"]);
    let resolved = reference.resolve(&second).expect("still found");
    assert_eq!(
        second
            .file(second.track(resolved).unwrap().file_id)
            .unwrap()
            .path,
        reference.key,
        "the reference names the file, not its position"
    );
}

#[test]
fn a_note_whose_target_is_gone_is_kept_waiting() {
    // The drive is unplugged, not the note deleted. Dropping what a user
    // wrote because a file is momentarily absent is the one unforgivable
    // failure in this program.
    let catalog = library(&["/m/a.flac"]);
    let mut data = UserData::default();
    let missing = EntityRef::new(EntityKind::Track, "/elsewhere/gone.flac");
    data.entry(LOCAL_USER, &missing, 10).note = Some("keep me".into());

    let report = reconcile(&mut data, &catalog);
    assert_eq!(report.waiting, 1);
    assert_eq!(data.annotations.len(), 1, "nothing was dropped");
    assert_eq!(data.annotations[0].note.as_deref(), Some("keep me"));
}

#[test]
fn a_file_that_moved_takes_its_note_with_it() {
    let mut data = UserData::default();
    let old = EntityRef::new(EntityKind::Track, "/old/place/a.flac");
    data.entry(LOCAL_USER, &old, 10).rating = Some(5);

    let catalog = library(&["/m/a.flac"]);
    let report = reconcile(&mut data, &catalog);
    assert_eq!(report.moved, 1);
    assert_eq!(data.annotations[0].target.key, "/m/a.flac");
    assert_eq!(data.annotations[0].rating, Some(5));
}

#[test]
fn two_files_of_the_same_name_are_not_guessed_between() {
    // Moving a note onto the wrong track is worse than leaving it waiting,
    // because nothing on screen would ever say so.
    let mut data = UserData::default();
    let old = EntityRef::new(EntityKind::Track, "/old/a.flac");
    data.entry(LOCAL_USER, &old, 10).loved = true;

    let catalog = library(&["/m/one/a.flac", "/m/two/a.flac"]);
    let report = reconcile(&mut data, &catalog);
    assert_eq!(report.moved, 0);
    assert_eq!(report.waiting, 1);
    assert_eq!(data.annotations[0].target.key, "/old/a.flac");
}

#[test]
fn one_record_holds_every_kind_of_opinion() {
    let mut data = UserData::default();
    let album = EntityRef::new(EntityKind::Release, "deicide|legion|/m");
    {
        let a = data.entry(LOCAL_USER, &album, 10);
        a.loved = true;
        a.rating = Some(4);
        a.note = Some("the reissue is better".into());
        a.tags.insert("to rip again".into());
    }
    assert_eq!(data.annotations.len(), 1, "one record, not four");

    // And copying it all onto another album is one operation.
    let other = EntityRef::new(EntityKind::Release, "deicide|once upon|/m");
    assert!(data.copy(LOCAL_USER, &album, &other, 20));
    let copied = data.find(LOCAL_USER, &other).expect("copied");
    assert_eq!(copied.rating, Some(4));
    assert!(copied.loved);
    assert_eq!(copied.tags.len(), 1);

    // Copying from a target nobody said anything about says so.
    let empty = EntityRef::new(EntityKind::Release, "nobody|nothing|/m");
    assert!(!data.copy(LOCAL_USER, &empty, &other, 30));
}

#[test]
fn a_record_that_says_nothing_is_forgotten() {
    let mut data = UserData::default();
    let target = EntityRef::new(EntityKind::Artist, "deicide");
    data.entry(LOCAL_USER, &target, 10).loved = true;
    data.entry(LOCAL_USER, &target, 10).loved = false;
    data.forget_empty();
    assert!(data.annotations.is_empty(), "no empty shell left behind");
}

#[test]
fn the_log_is_bounded_and_the_counters_are_not() {
    // The log answers "what did I listen to last night"; the counters
    // answer "what have I never heard", which a truncated log cannot.
    let mut data = UserData::default();
    let track = EntityRef::new(EntityKind::Track, "/m/a.flac");
    for i in 0..(HISTORY_LIMIT as u64 + 40) {
        data.record_play(Play {
            owner: LOCAL_USER.into(),
            track: track.clone(),
            at: 1000 + i,
            ms_played: 200_000,
            completed: true,
        });
    }
    assert_eq!(data.plays.len(), HISTORY_LIMIT, "the log is bounded");
    assert_eq!(
        data.play_count(LOCAL_USER, &track),
        HISTORY_LIMIT as u32 + 40,
        "the counter forgets nothing"
    );
    assert_eq!(
        data.plays.first().map(|p| p.at),
        Some(1040),
        "it is the oldest events that fall off"
    );
}

#[test]
fn a_round_trip_through_the_file_changes_nothing() {
    let mut data = UserData::default();
    let album = EntityRef::new(EntityKind::Release, "deicide|legion|/m");
    {
        let a = data.entry(LOCAL_USER, &album, 10);
        a.loved = true;
        a.rating = Some(5);
        a.note = Some("with a \"quote\" and a\nnewline".into());
        a.tags.insert("vinyl".into());
        a.tags.insert("to rip again".into());
    }
    data.record_play(Play {
        owner: LOCAL_USER.into(),
        track: EntityRef::new(EntityKind::Track, "/m/a: b.flac"),
        at: 99,
        ms_played: 1234,
        completed: false,
    });

    let back = from_json(&to_json(&data)).expect("read back");
    assert_eq!(back.annotations, data.annotations);
    assert_eq!(back.plays, data.plays);
    assert_eq!(back.counts, data.counts);
}

#[test]
fn one_broken_row_does_not_cost_every_note() {
    // The opposite of the catalog rule, and deliberately so: a catalog with
    // a broken row is a graph that does not hold together, and refusing is
    // the safe answer. Here each row stands alone, and refusing the file
    // would lose everything a user ever wrote over one bad line.
    use crate::json::Json;
    let mut root = Json::obj();
    root.set("format_version", USER_FORMAT_VERSION.into());
    let mut good = Json::obj();
    good.set("owner", LOCAL_USER.into());
    good.set("target", "artist:deicide".into());
    good.set("loved", Json::Bool(true));
    let mut broken = Json::obj();
    broken.set("owner", LOCAL_USER.into());
    broken.set("target", "nonsense-without-a-kind".into());
    broken.set("loved", Json::Bool(true));
    root.set("annotations", Json::Arr(vec![broken, good]));

    let data = from_json(&root).expect("the file still loads");
    assert_eq!(data.annotations.len(), 1, "the readable row survives");
    assert_eq!(data.annotations[0].target.key, "deicide");
}

#[test]
fn a_file_from_another_version_is_refused_rather_than_half_read() {
    use crate::json::Json;
    let mut root = Json::obj();
    root.set("format_version", (USER_FORMAT_VERSION + 1).into());
    root.set("annotations", Json::Arr(vec![]));
    assert!(
        matches!(
            from_json(&root),
            Err(crate::store::StoreError::Version { .. })
        ),
        "a newer file must not be silently emptied"
    );
}

#[test]
fn a_merge_keeps_both_halves_and_says_which_won() {
    // Someone restoring half a backup wants their two halves. An import
    // that replaced would be the one operation here able to lose
    // everything at once.
    let target = EntityRef::new(EntityKind::Artist, "deicide");
    let other = EntityRef::new(EntityKind::Artist, "ozzy osbourne");

    let mut mine = UserData::default();
    mine.entry(LOCAL_USER, &target, 100).note = Some("mine, newer".into());
    mine.annotations[0].updated_at = 100;

    let mut theirs = UserData::default();
    theirs.entry(LOCAL_USER, &target, 50).note = Some("theirs, older".into());
    theirs.annotations[0].updated_at = 50;
    theirs.entry(LOCAL_USER, &other, 50).loved = true;
    theirs.annotations[1].updated_at = 50;

    let report = merge(&mut mine, theirs);
    assert_eq!(report.added, 1, "what was missing came in");
    assert_eq!(report.kept, 1, "what was newer here stayed");
    assert_eq!(
        mine.find(LOCAL_USER, &target).unwrap().note.as_deref(),
        Some("mine, newer")
    );
    assert!(mine.find(LOCAL_USER, &other).unwrap().loved);
}

#[test]
fn importing_the_same_backup_twice_changes_nothing() {
    let track = EntityRef::new(EntityKind::Track, "/m/a.flac");
    let mut source = UserData::default();
    source.entry(LOCAL_USER, &track, 10).rating = Some(4);
    source.record_play(Play {
        owner: LOCAL_USER.into(),
        track: track.clone(),
        at: 999,
        ms_played: 1000,
        completed: true,
    });
    source.save_collection(LOCAL_USER, "metal", "genre:metal", 10);

    let mut into = UserData::default();
    merge(&mut into, source.clone());
    let second = merge(&mut into, source);

    assert_eq!(into.annotations.len(), 1);
    assert_eq!(into.plays.len(), 1, "an event is who, what and when");
    assert_eq!(into.collections.len(), 1);
    assert_eq!(second.plays, 0, "nothing new the second time");
    assert_eq!(
        into.play_count(LOCAL_USER, &track),
        1,
        "a counter is a total, and takes the larger"
    );
}

#[test]
fn a_saved_query_holds_the_question_and_not_the_answer() {
    // A collection that stored its result would be a playlist. Keeping the
    // expression is what makes it answer with what the library holds now.
    let mut data = UserData::default();
    assert!(!data.save_collection(LOCAL_USER, "Metal", "genre:metal", 10));
    assert_eq!(
        data.collection(LOCAL_USER, "metal")
            .map(|c| c.expression.as_str()),
        Some("genre:metal"),
        "found whatever the case it was typed in"
    );
    assert!(
        data.save_collection(LOCAL_USER, "metal", "genre:metal loved", 20),
        "saving over one says so"
    );
    assert_eq!(data.collections.len(), 1, "one name, one collection");
    assert!(data.forget_collection(LOCAL_USER, "METAL"));
    assert!(!data.forget_collection(LOCAL_USER, "metal"));
}

#[test]
fn a_token_survives_a_path_full_of_colons() {
    let reference = EntityRef::new(EntityKind::Track, "/m/a: b: c.flac");
    let back = EntityRef::parse_token(&reference.to_token()).expect("read back");
    assert_eq!(back, reference, "only the first colon separates");
}

#[test]
fn a_record_set_aside_survives_the_round_trip_and_a_merge() {
    let mut data = UserData::default();
    data.set_aside.push(SetAside {
        owner: LOCAL_USER.to_string(),
        release_group: "c9fdb94c".to_string(),
        title: "Sweet Dreams".to_string(),
        created_at: 1_700_000_000,
    });
    let text = to_json(&data).to_string_pretty();
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("user data");
    assert_eq!(back.set_aside, data.set_aside);

    // The title is stored beside the identifier so a listing can show
    // something a person recognises: a decision nobody can read is one nobody
    // can undo.
    assert_eq!(back.set_aside[0].title, "Sweet Dreams");

    // Importing the same backup twice must not file the decision twice: it was
    // taken or it was not, and there are no versions of it to arbitrate.
    let mut into = back.clone();
    let report = merge(&mut into, data.clone());
    assert_eq!(into.set_aside.len(), 1, "still one");
    assert_eq!(report.added, 0);

    // A different record is a different decision.
    let mut other = UserData::default();
    other.set_aside.push(SetAside {
        release_group: "aa11".to_string(),
        title: "The Manson Family Album".to_string(),
        ..data.set_aside[0].clone()
    });
    merge(&mut into, other);
    assert_eq!(into.set_aside.len(), 2);
}

#[test]
fn a_set_aside_row_without_an_identifier_is_not_read_back() {
    // The title alone cannot say which record was meant, and a wish list
    // quietly shortened by one is worse than one item too long.
    let text = format!(
        r#"{{"format_version":{USER_FORMAT_VERSION},"annotations":[],"plays":[],
             "counts":[],"collections":[],
             "set_aside":[{{"title":"No identifier"}},
                          {{"release_group":"aa11","title":"Kept"}}]}}"#
    );
    let back = from_json(&crate::json::parse(&text).expect("valid JSON")).expect("user data");
    assert_eq!(back.set_aside.len(), 1);
    assert_eq!(back.set_aside[0].release_group, "aa11");
}
