//! Tests for [`super`], split out of `builder.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::tests::{example_catalog, first_release, track};

#[test]
fn entities_are_deduplicated() {
    let c = example_catalog();
    // Metallica appears only once despite two tracks.
    assert_eq!(
        c.artists.iter().filter(|a| a.name == "Metallica").count(),
        1
    );
    assert_eq!(c.releases.len(), 2);
    assert_eq!(c.tracks.len(), 3);
}

#[test]
fn one_musicbrainz_recording_can_have_several_local_placements() {
    // The same performance on an album and a compilation is one recording in
    // two release positions. A title alone never makes this assertion; the
    // shared MusicBrainz recording identifier does.
    let mut album = track(
        "/music/Album/01.flac",
        &[
            ("title", "So What"),
            ("artist", "Miles Davis"),
            ("album", "Kind of Blue"),
            ("musicbrainz_recordingid", "recording-so-what"),
        ],
        545_000,
    );
    album.tags.insert("tracknumber", "1");
    let compilation = track(
        "/music/Compilation/04.flac",
        &[
            ("title", "So What"),
            ("artist", "Miles Davis"),
            ("album", "Jazz Classics"),
            ("musicbrainz_recordingid", "recording-so-what"),
        ],
        545_000,
    );

    let catalog = build(vec![album, compilation], vec!["/music".into()], 0, &[]);
    assert_eq!(catalog.tracks.len(), 2);
    assert_eq!(catalog.recordings.len(), 1);
    let recording = &catalog.recordings[0];
    assert_eq!(recording.mbid.as_deref(), Some("recording-so-what"));
    assert_eq!(recording.track_ids, vec![0, 1]);
    assert_eq!(catalog.tracks[0].recording_id, recording.id);
    assert_eq!(catalog.tracks[1].recording_id, recording.id);
}

#[test]
fn equal_titles_without_an_identifier_stay_different_recordings() {
    let first = track(
        "/music/Studio/01.flac",
        &[
            ("title", "Changes"),
            ("artist", "Black Sabbath"),
            ("album", "Vol. 4"),
        ],
        280_000,
    );
    let live = track(
        "/music/Live/01.flac",
        &[
            ("title", "Changes"),
            ("artist", "Black Sabbath"),
            ("album", "Live"),
        ],
        280_000,
    );

    let catalog = build(vec![first, live], vec!["/music".into()], 0, &[]);
    assert_eq!(catalog.recordings.len(), 2, "a title is not identity");
    assert_ne!(
        catalog.tracks[0].recording_id,
        catalog.tracks[1].recording_id
    );
}

#[test]
fn recordings_with_one_musicbrainz_work_identifier_share_a_work() {
    let studio = track(
        "/music/Studio/01.flac",
        &[
            ("title", "All Along the Watchtower"),
            ("artist", "Jimi Hendrix"),
            ("album", "Electric Ladyland"),
            ("musicbrainz_recordingid", "hendrix-recording"),
            ("musicbrainz_workid", "dylan-work"),
            ("work", "All Along the Watchtower"),
        ],
        240_000,
    );
    let cover = track(
        "/music/Cover/01.flac",
        &[
            ("title", "All Along the Watchtower"),
            ("artist", "Bob Dylan"),
            ("album", "John Wesley Harding"),
            ("musicbrainz_recordingid", "dylan-recording"),
            ("musicbrainz_workid", "dylan-work"),
            ("work", "All Along the Watchtower"),
        ],
        150_000,
    );

    let catalog = build(vec![studio, cover], vec!["/music".into()], 0, &[]);
    assert_eq!(catalog.recordings.len(), 2);
    assert_eq!(catalog.works.len(), 1);
    let work = &catalog.works[0];
    assert_eq!(work.mbid, "dylan-work");
    assert_eq!(work.recording_ids, vec![0, 1]);
    assert_eq!(catalog.recordings[0].work_ids, vec![work.id]);
    assert_eq!(catalog.recordings[1].work_ids, vec![work.id]);
}

#[test]
fn work_titles_without_identifiers_do_not_create_or_merge_works() {
    let first = track(
        "/music/First/01.flac",
        &[
            ("title", "Changes"),
            ("artist", "Black Sabbath"),
            ("album", "Vol. 4"),
            ("work", "Changes"),
        ],
        280_000,
    );
    let second = track(
        "/music/Second/01.flac",
        &[
            ("title", "Changes"),
            ("artist", "David Bowie"),
            ("album", "Hunky Dory"),
            ("work", "Changes"),
        ],
        210_000,
    );

    let catalog = build(vec![first, second], vec!["/music".into()], 0, &[]);
    assert!(catalog.works.is_empty(), "a title is not a work identity");
}

#[test]
fn editions_with_one_release_group_identifier_share_a_group() {
    let original = track(
        "/music/Original/01.flac",
        &[
            ("title", "War Pigs"),
            ("artist", "Black Sabbath"),
            ("album", "Paranoid"),
            ("musicbrainz_releasegroupid", "paranoid-group"),
        ],
        470_000,
    );
    let remaster = track(
        "/music/Remaster/01.flac",
        &[
            ("title", "War Pigs"),
            ("artist", "Black Sabbath"),
            ("album", "Paranoid (Remaster)"),
            ("musicbrainz_releasegroupid", "paranoid-group"),
        ],
        470_000,
    );

    let catalog = build(vec![original, remaster], vec!["/music".into()], 0, &[]);
    assert_eq!(catalog.release_groups.len(), 1);
    let group = &catalog.release_groups[0];
    assert_eq!(group.mbid, "paranoid-group");
    assert_eq!(group.release_ids, vec![0, 1]);
    assert_eq!(catalog.releases[0].release_group_id, Some(group.id));
    assert_eq!(catalog.releases[1].release_group_id, Some(group.id));
}

#[test]
fn featuring_creates_two_artists_and_one_link() {
    let c = example_catalog();
    let garou = c.find_artist("Garou").expect("Garou present");
    let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
    let neighbours = c.neighbours_of_artist(garou.id);
    assert_eq!(neighbours.len(), 1);
    assert_eq!(neighbours[0].0.id, celine.id);
    assert_eq!(neighbours[0].1, 1, "one track in common");
    // The link is indeed symmetric.
    assert_eq!(c.neighbours_of_artist(celine.id)[0].0.id, garou.id);
}

#[test]
fn various_artists_is_not_an_artist() {
    let c = build(
        vec![track(
            "/m/Various/Hits/01 Song.flac",
            &[
                ("title", "Song"),
                ("artist", "Performer"),
                ("album", "Hits"),
                ("albumartist", "Various Artists"),
            ],
            60_000,
        )],
        vec!["/m".into()],
        0,
        &[],
    );
    assert!(c.find_artist("Various Artists").is_none());
    assert_eq!(c.artists.len(), 1, "only the performer must exist");
    let hits = first_release(&c, "Hits").unwrap();
    assert!(hits.is_compilation);
    assert_eq!(hits.album_artist_id, None);
}

#[test]
fn a_box_set_in_disc_folders_is_one_album() {
    // The layout every box set and every game soundtrack uses:
    // `Album/Disc 1`, `Album/Disc 2`. Keyed on the folder they landed in,
    // one release became two of the same name, each numbering its tracks
    // from one — and nothing on screen said which disc was which except
    // the path. A disc folder is a subdivision of a release, not another
    // edition of it.
    let mut files = Vec::new();
    for disc in 1..=2 {
        for track in 1..=2 {
            let mut tags = RawTags::default();
            tags.insert("title", format!("D{disc} T{track}"));
            tags.insert("artist", "Nobuo Uematsu");
            tags.insert("albumartist", "Nobuo Uematsu");
            tags.insert("album", "FINAL FANTASY VII");
            tags.insert("date", "1997");
            tags.insert("tracknumber", track.to_string());
            tags.insert("discnumber", disc.to_string());
            files.push(ScannedFile {
                path: format!("/m/Uematsu/FF7/Disc {disc}/{track:02}.flac"),
                size: 100,
                mtime: 0,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            });
        }
    }
    let c = build(files, vec!["/m".into()], 0, &[]);
    assert_eq!(c.releases.len(), 1, "one album, not one per disc");
    assert_eq!(c.releases[0].track_ids.len(), 4);
    // The release lives where the album does, not in one of its discs —
    // which is also what `copy` reproduces and what `doctor` names.
    assert_eq!(c.releases[0].folder, "/m/Uematsu/FF7");
    let discs: Vec<Option<u32>> = c.tracks.iter().map(|t| t.disc_no).collect();
    assert_eq!(discs, vec![Some(1), Some(1), Some(2), Some(2)]);
}

#[test]
fn a_disc_folder_supplies_the_number_the_tags_forgot() {
    // A rip that split the discs into folders and left `discnumber` empty
    // is common. Merged without this, both discs would be disc one and the
    // release would hold two track 1s — worse than the split it replaces.
    let mut files = Vec::new();
    for disc in 1..=2 {
        let mut tags = RawTags::default();
        tags.insert("title", format!("D{disc}"));
        tags.insert("artist", "A");
        tags.insert("albumartist", "A");
        tags.insert("album", "Box");
        tags.insert("tracknumber", "1");
        files.push(ScannedFile {
            path: format!("/m/A/Box/CD{disc}/01.flac"),
            size: 100,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let c = build(files, vec!["/m".into()], 0, &[]);
    assert_eq!(c.releases.len(), 1);
    let discs: Vec<Option<u32>> = c.tracks.iter().map(|t| t.disc_no).collect();
    assert_eq!(discs, vec![Some(1), Some(2)], "read from the folder");
}

#[test]
fn two_editions_in_two_folders_are_still_two_albums() {
    // The folder is in the release key to tell two editions apart — a CD
    // rip beside a vinyl rip. Folding disc folders in must not fold those.
    let edition = |folder: &str| {
        let mut tags = RawTags::default();
        tags.insert("title", "T");
        tags.insert("artist", "A");
        tags.insert("albumartist", "A");
        tags.insert("album", "Album");
        tags.insert("tracknumber", "1");
        ScannedFile {
            path: format!("/m/A/{folder}/01.flac"),
            size: 100,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    };
    let c = build(
        vec![edition("Album (CD rip)"), edition("Album (vinyl rip)")],
        vec!["/m".into()],
        0,
        &[],
    );
    assert_eq!(c.releases.len(), 2, "two editions stay two");
}

#[test]
fn compilation_has_no_album_artist() {
    let c = example_catalog();
    let duos = first_release(&c, "Duos").expect("compilation found");
    assert!(duos.is_compilation);
    assert_eq!(duos.album_artist_id, None);
}

#[test]
fn interning_reuses_entities_and_keeps_ids_contiguous() {
    let mut b = Builder::new(vec!["/m".into()], 0, Default::default());
    let first = b.intern_artist("The Beatles");
    assert_eq!(
        first,
        b.intern_artist("Beatles, The"),
        "normalization matches them"
    );
    assert_eq!(b.intern_artist("Björk"), 1, "a new name takes the next id");
    assert_eq!(b.intern_label("Columbia", None), 0);
    assert_eq!(b.intern_genre("Jazz"), 0);
    let catalog = b.finish();
    assert_eq!(catalog.artists.len(), 2);
    for (index, artist) in catalog.artists.iter().enumerate() {
        assert_eq!(artist.id as usize, index, "ids index the vector");
    }
}

#[test]
fn an_explicit_label_identifier_becomes_canonical_without_name_matching() {
    let item = track(
        "/music/Album/01.flac",
        &[
            ("title", "Song"),
            ("artist", "Artist"),
            ("album", "Album"),
            ("label", "Columbia"),
            ("musicbrainz_labelid", "label-id"),
        ],
        60_000,
    );
    let catalog = build(vec![item], vec!["/music".into()], 0, &[]);
    assert_eq!(catalog.labels.len(), 1);
    assert_eq!(catalog.labels[0].mbid.as_deref(), Some("label-id"));
}

#[test]
fn the_same_credit_is_never_recorded_twice() {
    let mut b = Builder::new(vec![], 0, Default::default());
    let artist = b.intern_artist("Miles Davis");
    b.push_credit(artist, EntityKind::Track, 0, "main");
    b.push_credit(artist, EntityKind::Track, 0, "main");
    b.push_credit(artist, EntityKind::Track, 0, "composer");
    let catalog = b.finish();
    assert_eq!(
        catalog.credits.len(),
        2,
        "same role once, different role kept"
    );
}

#[test]
fn placeholder_album_artists_are_recognised() {
    for name in ["Various Artists", "various", "VA", "Artistes divers"] {
        assert!(is_various_artists(name), "{name} should be a placeholder");
    }
    for name in ["Various Cruelties", "Miles Davis"] {
        assert!(!is_various_artists(name), "{name} is a real artist");
    }
}

#[test]
fn title_inferred_from_filename() {
    assert_eq!(title_from_filename("/m/01 - So What.flac"), "So What");
    assert_eq!(title_from_filename("/m/So What.flac"), "So What");
    assert_eq!(track_from_filename("/m/07 - Blue in Green.flac"), Some(7));
    assert_eq!(track_from_filename("/m/Blue in Green.flac"), None);
}

#[test]
fn deterministic_build() {
    let a = example_catalog();
    let b = example_catalog();
    let names_a: Vec<&str> = a.artists.iter().map(|x| x.name.as_str()).collect();
    let names_b: Vec<&str> = b.artists.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names_a, names_b, "identifiers must be stable");
}

#[test]
fn two_spellings_under_one_musicbrainz_id_build_one_artist() {
    // The half of artist identity that needs no heuristic. Before this, a
    // library holding `Ozzy Osbourne` on one album and `O. Osbourne` on another
    // held two musicians, and every count, every listing and every page was
    // wrong by one.
    let file = |album: &str, artist: &str, mbid: Option<&str>| {
        let mut tags = RawTags::default();
        tags.insert("artist", artist);
        tags.insert("albumartist", artist);
        tags.insert("album", album);
        tags.insert("title", "A track");
        if let Some(mbid) = mbid {
            tags.insert("musicbrainz_artistid", mbid);
            tags.insert("musicbrainz_albumartistid", mbid);
        }
        ScannedFile {
            path: format!("/music/{artist}/{album}/01.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    };

    let catalog = build(
        vec![
            file("Blizzard of Ozz", "Ozzy Osbourne", Some("ozzy")),
            file("Diary of a Madman", "Ozzy Osbourne", Some("ozzy")),
            file("Bark at the Moon", "O. Osbourne", Some("ozzy")),
        ],
        vec!["/music".to_string()],
        1,
        &[],
    );

    assert_eq!(catalog.artists.len(), 1, "one man, {:?}", catalog.artists);
    let ozzy = &catalog.artists[0];
    assert_eq!(
        ozzy.name, "Ozzy Osbourne",
        "and named by the spelling that names the most tracks, not by the \
         matching key and not by whichever file was read first"
    );
    assert_eq!(ozzy.key, "ozzy osbourne");
    assert_eq!(
        ozzy.mbid.as_deref(),
        Some("ozzy"),
        "the identifier that merged them is kept"
    );
    // Three albums, all his: the merge has to reach the credits and not only
    // the artist list, or a page would show one man with one album.
    assert_eq!(catalog.releases.len(), 3);
    assert!(
        catalog
            .releases
            .iter()
            .all(|r| r.album_artist_id == Some(ozzy.id))
    );
}

#[test]
fn without_an_identifier_two_spellings_stay_two_artists() {
    // The other half, and it is not this module's to solve: nobody on earth
    // knows that a particular `O. Osbourne` is Ozzy except the person whose
    // disk it is. Guessing from the strings is what would merge Angus Young
    // with Neil Young.
    let file = |artist: &str| {
        let mut tags = RawTags::default();
        tags.insert("artist", artist);
        tags.insert("albumartist", artist);
        tags.insert("album", "An album");
        tags.insert("title", "A track");
        ScannedFile {
            path: format!("/music/{artist}/01.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    };
    let catalog = build(
        vec![file("Ozzy Osbourne"), file("O. Osbourne")],
        vec!["/music".to_string()],
        1,
        &[],
    );
    assert_eq!(catalog.artists.len(), 2, "{:?}", catalog.artists);
}

#[test]
fn two_artists_on_one_track_leave_their_identifiers_unpaired() {
    // A tag naming two artists arrives as one string that `split_artists` cuts
    // in two — on a semicolon here, one of the separators it treats as hard —
    // while the identifiers arrive as their own list in an order nothing
    // guarantees to match. Pairing by position would file an identifier against
    // whichever name sorted first: an invention, and the kind this program
    // refuses rather than arbitrates.
    let mut tags = RawTags::default();
    tags.insert("artist", "Queen; David Bowie");
    tags.insert("albumartist", "Queen; David Bowie");
    tags.insert("album", "Hot Space");
    tags.insert("title", "Under Pressure");
    tags.insert("musicbrainz_artistid", "queen-id");
    tags.insert("musicbrainz_artistid", "bowie-id");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Queen/Hot Space/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    assert_eq!(catalog.artists.len(), 2, "{:?}", catalog.artists);
    assert!(
        catalog.artists.iter().all(|a| a.mbid.is_none()),
        "neither is given an identifier on the strength of its position: {:?}",
        catalog.artists
    );
}

#[test]
fn a_collaboration_credit_is_two_artists_when_the_tags_say_which_two() {
    // `Rob Zombie & Ozzy Osbourne` reached the shelf as an *artist*, with one
    // track and one album, beside the real Ozzy — and four such rows made
    // `aede artist ozzy` an ambiguity between five. They are not artists; they
    // are credits nobody split, because `&` cannot be split safely from the
    // string alone: `Simon & Garfunkel` is one band.
    //
    // `ARTISTS` is the tag written for this, one value per artist, and a file
    // that carries it has already answered the question.
    let mut tags = RawTags::default();
    tags.insert("artist", "Rob Zombie & Ozzy Osbourne");
    tags.insert("artists", "Rob Zombie");
    tags.insert("artists", "Ozzy Osbourne");
    tags.insert("albumartist", "Rob Zombie");
    tags.insert("album", "Educated Horses");
    tags.insert("title", "Iron Head");

    let catalog = build(
        vec![ScannedFile {
            path: "/music/Rob Zombie/Educated Horses/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );

    let names: Vec<&str> = catalog.artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Rob Zombie", "Ozzy Osbourne"], "{names:?}");
    assert!(
        !names.iter().any(|n| n.contains('&')),
        "the joined credit is not an artist: {names:?}"
    );
}

#[test]
fn a_band_whose_name_holds_an_ampersand_is_still_one_band() {
    // The other side of the same coin, and the reason `&` is not a separator:
    // splitting the string would shatter Simon & Garfunkel, Earth, Wind & Fire
    // and every band like them. With no `ARTISTS` tag to say otherwise, the
    // name stands.
    let mut tags = RawTags::default();
    tags.insert("artist", "Simon & Garfunkel");
    tags.insert("albumartist", "Simon & Garfunkel");
    tags.insert("album", "Bookends");
    tags.insert("title", "America");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Simon & Garfunkel/Bookends/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );
    let names: Vec<&str> = catalog.artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["Simon & Garfunkel"], "{names:?}");
}

#[test]
fn a_performer_tag_that_names_the_pair_and_then_each_of_them_names_two_people() {
    // Measured on a real file, and the last of the five rows `aede artist ozzy`
    // had to refuse between. `War Pigs (charity version)` carries
    // `PERFORMER=Ozzy Osbourne; Judas Priest; Judas Priest & Ozzy Osbourne`:
    // the pair, then each of them. `ARTISTS` cannot help here — this is not the
    // artist tag — but the list answers for itself, because the third value is
    // made of the first two and nothing else.
    let mut tags = RawTags::default();
    tags.insert("artist", "Judas Priest featuring Ozzy Osbourne");
    tags.insert("artists", "Judas Priest");
    tags.insert("artists", "Ozzy Osbourne");
    tags.insert("albumartist", "Judas Priest featuring Ozzy Osbourne");
    tags.insert(
        "performer",
        "Ozzy Osbourne; Judas Priest; Judas Priest & Ozzy Osbourne",
    );
    tags.insert("album", "War Pigs (charity version)");
    tags.insert("title", "War Pigs (charity version)");

    let catalog = build(
        vec![ScannedFile {
            path: "/music/War Pigs/01.flac".to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    );

    let names: Vec<&str> = catalog.artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(
        !names.iter().any(|n| n.contains('&')),
        "the joint credit is the same two people said again: {names:?}"
    );
}
