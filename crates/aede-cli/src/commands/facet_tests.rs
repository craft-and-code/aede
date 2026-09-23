use aede_core::model::{Artist, Credit, EntityKind};

use super::*;

fn artist(id: Id, name: &str) -> Artist {
    Artist {
        id,
        name: name.into(),
        sort_name: name.into(),
        key: name.to_lowercase(),
        mbid: None,
        aliases: Vec::new(),
    }
}

fn credit(artist_id: Id, track: Id, role: &str) -> Credit {
    Credit {
        artist_id,
        entity_kind: EntityKind::Track,
        entity_id: track,
        role: role.into(),
        credited_as: None,
        attributes: Vec::new(),
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|n| n.to_string()).collect()
}

#[test]
fn a_page_that_answers_does_not_open_by_denying() {
    // `label earache` printed "no label is called \"earache\"" directly
    // above a heading reading "Earache Records", while `albums --label
    // earache` narrowed on the same text without a word. The note was
    // reporting the mechanism — exact lookup failed, substring lookup ran —
    // where the question had in fact been answered.
    assert_eq!(
        match_note(
            TitleMatch::Partial,
            "earache",
            &names(&["Earache Records"]),
            "label"
        ),
        None,
        "one name needs no gloss: the heading is the answer"
    );
    assert_eq!(
        match_note(
            TitleMatch::Exact,
            "Columbia",
            &names(&["Columbia"]),
            "label"
        ),
        None
    );

    // Several do, because the heading joins them with a comma and reads as
    // a single name.
    assert_eq!(
        match_note(
            TitleMatch::Partial,
            "metal",
            &names(&["Black Metal", "Doom Metal", "Metal"]),
            "genre"
        )
        .as_deref(),
        Some("\"metal\" matches 3 genres; this page covers them all")
    );
}

#[test]
fn an_artist_counts_once_per_track_however_many_roles_they_hold() {
    // A well-tagged file carries ARTIST and PERFORMER, so the band is
    // credited twice on every track of its own album. The label page read
    // that as 57 tracks for a band whose albums on the page held 29, right
    // under an albums table that added up to 29.
    let mut catalog = Catalog {
        artists: vec![artist(0, "Deicide"), artist(1, "Steve Asheim")],
        ..Default::default()
    };
    for track in 0..3 {
        catalog.credits.push(credit(0, track, "main"));
        catalog.credits.push(credit(0, track, "performer"));
        // A non-performing role stays out of the reckoning entirely.
        catalog.credits.push(credit(1, track, "lyricist"));
    }
    // And one track where the drummer is heard as well as credited.
    catalog.credits.push(credit(1, 0, "performer"));

    let rows = tracks_per_artist(&catalog, &[0, 1, 2]);
    assert_eq!(rows, vec![(0, 3), (1, 1)], "counted rows, not tracks");
}

#[test]
fn no_artist_can_carry_more_tracks_than_the_page_holds() {
    // The bound the printed page must always satisfy, whatever the tags do.
    let mut catalog = Catalog {
        artists: vec![artist(0, "Bolt Thrower")],
        ..Default::default()
    };
    for track in 0..9 {
        for role in ["main", "performer", "conductor", "remixer"] {
            catalog.credits.push(credit(0, track, role));
        }
    }
    let tracks: Vec<Id> = (0..9).collect();
    for (_, count) in tracks_per_artist(&catalog, &tracks) {
        assert!(count <= tracks.len(), "{count} of {}", tracks.len());
    }
}
