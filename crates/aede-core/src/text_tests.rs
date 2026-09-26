//! Tests for [`super`], split out of `text.rs`.

use super::*;

#[test]
fn windows_catalog_paths_keep_their_native_spelling_and_folder_boundaries() {
    for (path, parent, root) in [
        (
            r"C:\Music\Artist\Album\01.flac",
            r"C:\Music\Artist\Album",
            r"C:\Music",
        ),
        (
            r"\\?\C:\Music\Artist\Album\01.flac",
            r"\\?\C:\Music\Artist\Album",
            r"\\?\C:\Music",
        ),
        (
            r"\\nas\music\Artist\Album\01.flac",
            r"\\nas\music\Artist\Album",
            r"\\nas\music",
        ),
        (
            r"\\?\UNC\nas\music\Artist\Album\01.flac",
            r"\\?\UNC\nas\music\Artist\Album",
            r"\\?\UNC\nas\music",
        ),
    ] {
        assert_eq!(file_name(path), "01.flac");
        assert_eq!(folder(path), parent);
        assert!(is_under(path, root));
        assert!(is_under(path, &format!("{root}\\")));
        assert!(!is_under(&format!("{root}backup\\01.flac"), root));
    }
    assert_eq!(folder(r"C:\01.flac"), "C:\\");
    assert_eq!(folder(r"\\?\C:\01.flac"), "\\\\?\\C:\\");
    assert!(is_under(r"C:\Music\Album\01.flac", "C:/Music/Album"));
    assert!(!is_under(r"D:\Music\01.flac", r"C:\Music"));
    assert_eq!(
        relative_under(r"C:\Music\Album\01.flac", "C:/Music/"),
        Some("Album/01.flac".into())
    );
    assert_eq!(
        relative_under(r"\\?\C:\Music\01.flac", r"\\?\C:/Music"),
        None
    );
    assert_eq!(folder("/music/name:/01.flac"), "/music/name:");
}

#[cfg(unix)]
#[test]
fn unix_backslashes_remain_file_name_characters() {
    assert_eq!(
        file_name(r"/music/artist\name/01\title.flac"),
        r"01\title.flac"
    );
    assert_eq!(
        folder(r"/music/artist\name/01\title.flac"),
        r"/music/artist\name"
    );
    assert!(!is_under(r"/music/artist\name", "/music/artist"));
}

#[test]
fn a_collaboration_written_with_a_slash_after_w_is_two_names() {
    // `Ozzy Osbourne w/Therapy?` arrived on a real shelf as one artist with one
    // track. The marker carries no trailing space on purpose — it is written
    // both ways — and the space *before* it is what keeps it from matching
    // inside a name.
    assert_eq!(
        split_artists("Ozzy Osbourne w/Therapy?"),
        vec!["Ozzy Osbourne", "Therapy?"]
    );
    assert_eq!(
        split_artists("Ozzy Osbourne w/ Therapy?"),
        vec!["Ozzy Osbourne", "Therapy?"]
    );
}

#[test]
fn an_ampersand_is_never_a_separator_however_tempting() {
    // The rule this whole fallback rests on: no amount of looking at the string
    // tells `Rob Zombie & Ozzy Osbourne` from `Simon & Garfunkel`. The tag that
    // can is `ARTISTS`, and it is read before this is reached.
    for one in [
        "Simon & Garfunkel",
        "Earth, Wind & Fire",
        "Nick Cave & the Bad Seeds",
        "Rob Zombie & Ozzy Osbourne",
    ] {
        assert_eq!(split_artists(one), vec![one], "{one}");
    }
}

#[test]
fn a_disc_folder_is_recognised_and_nothing_else_is() {
    // A box set laid out as Album/Disc 1, Album/Disc 2 is one album, not
    // two. Reading the folder wrongly in either direction is expensive:
    // missing it splits a release, claiming it merges two.
    for (name, number) in [
        ("Disc 1", 1),
        ("disc1", 1),
        ("CD2", 2),
        ("cd 2", 2),
        ("Disque 3", 3),
        ("Disk-4", 4),
        ("DISC_10", 10),
    ] {
        assert_eq!(disc_folder(name), Some(number), "{name}");
    }
    for name in [
        "CD Singles",
        "Discography",
        "disc one",
        "Bonus",
        "cd2 bonus",
        "Disc",
        "1",
        "Disc 0",
    ] {
        assert_eq!(disc_folder(name), None, "{name} is not a disc folder");
    }
}

#[test]
fn a_folder_is_not_a_prefix_of_its_name() {
    assert!(is_under("/music/Rock/01.flac", "/music/Rock"));
    assert!(is_under("/music/Rock", "/music/Rock"));
    assert!(is_under("/music/Rock/01.flac", "/music/Rock/"));
    // The trap: one name beginning with the other.
    assert!(!is_under("/music/Rockabilly/01.flac", "/music/Rock"));
    assert!(!is_under("/music", "/music/Rock"));
    assert!(!is_under("/other/Rock/01.flac", "/music/Rock"));
}

#[test]
fn path_parts() {
    assert_eq!(file_name("/music/a/01.flac"), "01.flac");
    assert_eq!(folder("/music/a/01.flac"), "/music/a");
    // A bare name is its own file name, and is in no folder.
    assert_eq!(file_name("01.flac"), "01.flac");
    assert_eq!(folder("01.flac"), "");
    // A file sitting at the root has no folder either: the root is not a
    // grouping.
    assert_eq!(folder("/01.flac"), "");
}

#[test]
fn article_normalization() {
    assert_eq!(normalize("The Beatles"), normalize("Beatles, The"));
    assert_eq!(normalize("The Beatles"), "beatles");
    assert_eq!(normalize("  the   ROLLING   Stones "), "rolling stones");
    // A lone article must not disappear.
    assert_eq!(normalize("The The"), "the");
}

#[test]
fn accent_and_punctuation_normalization() {
    assert_eq!(normalize("Björk"), "bjork");
    assert_eq!(normalize("Sigur Rós"), "sigur ros");
    assert_eq!(normalize("AC/DC"), "ac dc");
    assert_eq!(normalize("Motörhead!"), "motorhead");
    assert_eq!(normalize("Émilie Simon"), "emilie simon");
}

#[test]
fn artist_splitting() {
    assert_eq!(split_artists("Miles Davis"), vec!["Miles Davis"]);
    assert_eq!(
        split_artists("Miles Davis; John Coltrane"),
        vec!["Miles Davis", "John Coltrane"]
    );
    assert_eq!(
        split_artists("Daft Punk feat. Pharrell Williams"),
        vec!["Daft Punk", "Pharrell Williams"]
    );
    // Ampersands inside band names must NOT be cut.
    assert_eq!(
        split_artists("Simon & Garfunkel"),
        vec!["Simon & Garfunkel"]
    );
    assert_eq!(
        split_artists("Earth, Wind & Fire"),
        vec!["Earth, Wind & Fire"]
    );
}

#[test]
fn sort_names() {
    assert_eq!(sort_name("The Beatles"), "Beatles, The");
    assert_eq!(sort_name("Miles Davis"), "Miles Davis");
    assert_eq!(sort_name("Les Rita Mitsouko"), "Rita Mitsouko, Les");
}

#[test]
fn year_extraction() {
    assert_eq!(extract_year("1959"), Some(1959));
    assert_eq!(extract_year("1959-08-17"), Some(1959));
    assert_eq!(extract_year("17/08/1959"), Some(1959));
    assert_eq!(extract_year("unknown"), None);
    assert_eq!(extract_year("12"), None);
}

#[test]
fn track_numbers() {
    assert_eq!(parse_track_number("5"), (Some(5), None));
    assert_eq!(parse_track_number("5/12"), (Some(5), Some(12)));
    assert_eq!(parse_track_number("noise"), (None, None));
}

#[test]
fn formatting() {
    assert_eq!(format_duration(65_000), "1:05");
    assert_eq!(format_duration(3_725_000), "1:02:05");
    // Rounded, not truncated: 4 min 20.7 s is 4:21, as in any player.
    assert_eq!(format_duration(260_700), "4:21");
    assert_eq!(format_duration(260_400), "4:20");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1500), "1.5 kB");
    // Decimal units, like the Finder: 315.7 MB, not 301.1 "MB".
    assert_eq!(format_size(315_727_769), "315.7 MB");
}

#[test]
fn a_joint_credit_beside_its_members_is_the_same_thing_said_twice() {
    // Measured on a real file. `War Pigs (charity version)` carries
    // `PERFORMER=Ozzy Osbourne; Judas Priest; Judas Priest & Ozzy Osbourne`,
    // and the third value put a third artist on the shelf — one that made
    // `aede artist ozzy` refuse among five names, four of which were nobody.
    let said = |list: &[&str]| without_restatements(list.iter().map(|s| s.to_string()).collect());
    assert_eq!(
        said(&[
            "Ozzy Osbourne",
            "Judas Priest",
            "Judas Priest & Ozzy Osbourne"
        ]),
        vec!["Ozzy Osbourne", "Judas Priest"]
    );
    // The joining word is no obstacle, and neither is the order.
    assert_eq!(
        said(&[
            "Post Malone feat. Ozzy Osbourne",
            "Ozzy Osbourne",
            "Post Malone"
        ]),
        vec!["Ozzy Osbourne", "Post Malone"]
    );
}

#[test]
fn a_band_name_survives_a_list_that_never_names_its_parts() {
    // The fault the whole rule exists to avoid. `Kool & the Gang` is one band,
    // and it stays one band as long as the tag does not also credit `Kool` and
    // `the Gang` separately — which no tag does, and which would itself be the
    // file naming them.
    let said = |list: &[&str]| without_restatements(list.iter().map(|s| s.to_string()).collect());
    for band in ["Simon & Garfunkel", "Kool & the Gang", "Earth, Wind & Fire"] {
        assert_eq!(said(&[band]), vec![band], "{band}");
        assert_eq!(
            said(&["Ozzy Osbourne", band]),
            vec!["Ozzy Osbourne", band],
            "{band}"
        );
    }
}

#[test]
fn one_name_inside_another_is_not_a_restatement() {
    // Two names eaten, not one. A single match would mean that any credit
    // containing another credit restates it, and `Therapy?` sitting beside
    // `Ozzy Osbourne w/Therapy?` would delete the collaboration rather than the
    // duplicate — the opposite of the intention.
    let said = |list: &[&str]| without_restatements(list.iter().map(|s| s.to_string()).collect());
    assert_eq!(
        said(&["Therapy?", "Ozzy Osbourne w/Therapy?"]),
        vec!["Therapy?", "Ozzy Osbourne w/Therapy?"]
    );
    // And a leftover that is a word rather than a joiner keeps the value too:
    // `Tribute` names something the other two do not.
    assert_eq!(
        said(&[
            "Ozzy Osbourne",
            "Judas Priest",
            "Judas Priest Tribute Ozzy Osbourne"
        ])
        .len(),
        3
    );
}
