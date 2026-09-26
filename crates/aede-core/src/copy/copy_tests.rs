//! Tests for [`super`], split out of `mod.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::{self, ScannedFile};
use crate::tags::RawTags;

fn file(path: &str, album: &str) -> ScannedFile {
    let mut tags = RawTags::default();
    for (key, value) in [
        ("title", "T"),
        ("artist", "A"),
        ("albumartist", "A"),
        ("album", album),
        ("date", "1990"),
    ] {
        tags.insert(key, value);
    }
    tags.properties.codec = "flac".into();
    tags.properties.duration_ms = Some(1000);
    ScannedFile {
        path: path.into(),
        size: 1000,
        mtime: 0,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

fn catalog_of(paths: &[(&str, &str)], roots: &[&str]) -> Catalog {
    model::build(
        paths.iter().map(|(p, a)| file(p, a)).collect(),
        roots.iter().map(|r| (*r).to_string()).collect(),
        0,
        &[],
    )
}

fn all_tracks(catalog: &Catalog) -> Vec<Id> {
    catalog.tracks.iter().map(|t| t.id).collect()
}

#[test]
fn windows_copy_plans_keep_the_tree_and_refuse_destinations_inside_the_library() {
    for root in [
        r"C:\Music",
        r"\\?\C:\Music",
        r"\\nas\music",
        r"\\?\UNC\nas\music",
    ] {
        let path = format!("{root}\\Artist\\Album\\01.flac");
        let catalog = catalog_of(&[(&path, "Album")], &[root]);
        let plan = plan(
            &catalog,
            &all_tracks(&catalog),
            &Recipe {
                extras: Extras::None,
                ..Default::default()
            },
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].source, PathBuf::from(&path));
        assert_eq!(
            plan.items[0].relative,
            PathBuf::from("Artist").join("Album").join("01.flac")
        );
        assert!(inside_a_watched_root(&catalog, Path::new(&format!("{root}\\backup"))).is_some());
        assert!(inside_a_watched_root(&catalog, Path::new(&format!("{root}-backup"))).is_none());
    }
}

#[test]
fn the_tree_is_kept_relative_to_the_root_that_holds_it() {
    // The whole promise of the command: what sat under the watched folder
    // arrives under the destination in the same shape.
    let catalog = catalog_of(
        &[
            ("/m/Ozzy/1980 Blizzard/01.flac", "Blizzard"),
            ("/m/Danzig/1994 Danzig 4/02.flac", "Danzig 4"),
        ],
        &["/m"],
    );
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    let places: Vec<PathBuf> = plan.items.iter().map(|i| i.relative.clone()).collect();
    assert_eq!(
        places,
        vec![
            PathBuf::from("Danzig/1994 Danzig 4/02.flac"),
            PathBuf::from("Ozzy/1980 Blizzard/01.flac"),
        ]
    );
    assert_eq!(plan.total_bytes(), 2000);
    assert!(plan.renamed.is_empty(), "nothing needed adapting");
}

#[test]
fn the_deepest_watched_root_decides_the_tree() {
    // Watching a folder and something inside it is legal. Taking the
    // shorter root would carry the intermediate folders into a destination
    // the user asked precisely to be rid of.
    let catalog = catalog_of(&[("/m/live/set/01.flac", "Set")], &["/m", "/m/live"]);
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    assert_eq!(plan.items[0].relative, Path::new("set/01.flac"));
}

#[test]
fn a_file_under_no_root_is_reported_and_not_invented_a_place_for() {
    // It has no tree to keep. Dropping it in at the top would silently mix
    // it in with the folders that do have one.
    let mut catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
    catalog.roots = vec!["/elsewhere".into()];
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    assert!(plan.items.is_empty());
    assert_eq!(plan.rootless, vec![PathBuf::from("/m/a/01.flac")]);
}

#[test]
fn one_file_reached_by_two_tracks_is_copied_once() {
    let catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
    let id = catalog.tracks[0].id;
    let plan = plan(
        &catalog,
        &[id, id, id],
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    assert_eq!(plan.items.len(), 1);
}

#[test]
fn a_restricted_destination_renames_and_says_so() {
    // The punctuation a music library is full of and a card refuses. Every
    // change is reported: a copy whose names quietly differ from the
    // library cannot be compared against it afterwards.
    let catalog = catalog_of(
        &[(
            "/m/Pixies/Surfer Rosa/Where Is My Mind?.flac",
            "Surfer Rosa",
        )],
        &["/m"],
    );
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            restrict_names: true,
            ..Default::default()
        },
    );
    assert_eq!(
        plan.items[0].relative,
        Path::new("Pixies/Surfer Rosa/Where Is My Mind_.flac")
    );
    assert_eq!(plan.renamed.len(), 1, "and it is reported");

    // Left alone when the destination takes them.
    let unrestricted = super::plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    assert_eq!(
        unrestricted.items[0].relative,
        Path::new("Pixies/Surfer Rosa/Where Is My Mind?.flac")
    );
    assert!(unrestricted.renamed.is_empty());
}

#[test]
fn two_names_that_adapt_to_one_do_not_become_one_file() {
    // "1: Live" and "1? Live" both become "1_ Live". Writing the second
    // over the first would lose a track and report a complete copy.
    let catalog = catalog_of(
        &[("/m/a/1: Live.flac", "A"), ("/m/a/1? Live.flac", "A")],
        &["/m"],
    );
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            restrict_names: true,
            ..Default::default()
        },
    );
    let places: BTreeSet<PathBuf> = plan.items.iter().map(|i| i.relative.clone()).collect();
    assert_eq!(places.len(), 2, "two files, two destinations: {places:?}");
}

#[test]
fn the_same_file_name_in_two_albums_is_not_renamed() {
    // Uniqueness is per folder. Forcing it across the tree would rename
    // half a library, where every album legitimately opens with an `01`.
    let catalog = catalog_of(&[("/m/a/01.flac", "A"), ("/m/b/01.flac", "B")], &["/m"]);
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            restrict_names: true,
            ..Default::default()
        },
    );
    assert!(plan.renamed.is_empty(), "{:?}", plan.renamed);
    assert_eq!(plan.items.len(), 2);
}

#[test]
fn a_destination_inside_a_watched_root_is_refused() {
    // The next scan would read the copies as new files, the catalog would
    // double, and doctor would report every album as its own duplicate.
    let catalog = catalog_of(&[("/m/a/01.flac", "A")], &["/m"]);
    assert_eq!(
        inside_a_watched_root(&catalog, Path::new("/m/backup")).as_deref(),
        Some("/m")
    );
    // And the other way round: a destination that would swallow the root.
    assert_eq!(
        inside_a_watched_root(&catalog, Path::new("/")).as_deref(),
        Some("/m")
    );
    assert_eq!(
        inside_a_watched_root(&catalog, Path::new("/Volumes/USB")),
        None
    );
}

/// A catalog holding one file of a given codec, lossless or not.
fn catalog_of_codec(path: &str, codec: &str, lossless: bool) -> Catalog {
    let mut f = file(path, "A");
    f.tags.properties.codec = codec.into();
    f.tags.properties.lossless = lossless;
    f.tags.properties.duration_ms = Some(240_000);
    model::build(vec![f], vec!["/m".into()], 0, &[])
}

fn converted_to(catalog: &Catalog, target: transcode::Target) -> Option<transcode::Target> {
    let plan = plan(
        catalog,
        &all_tracks(catalog),
        &Recipe {
            extras: Extras::None,
            convert: Some(target),
            ..Default::default()
        },
    );
    plan.items[0].convert
}

#[test]
fn only_a_lossless_source_is_ever_encoded() {
    use transcode::Target;
    // The rule, and the three cases it settles at once.
    let flac = catalog_of_codec("/m/a/01.flac", "flac", true);
    assert_eq!(converted_to(&flac, Target::Mp3), Some(Target::Mp3));

    // Already lossy, lossy asked for: a second pass over a first one is
    // audible, and the file was already small, which was the point.
    let mp3 = catalog_of_codec("/m/a/01.mp3", "mp3", false);
    assert_eq!(
        converted_to(&mp3, Target::Opus),
        None,
        "copied as it stands"
    );

    // Already lossy, lossless asked for: the result would be larger than
    // the source and no better — lossless in name only. Producing it
    // deliberately would be absurd.
    assert_eq!(converted_to(&mp3, Target::Flac), None);

    // Already in the target format: nothing to do.
    let already = catalog_of_codec("/m/a/01.mp3", "mp3", false);
    assert_eq!(converted_to(&already, Target::Mp3), None);
    let flac_to_flac = catalog_of_codec("/m/a/01.flac", "flac", true);
    assert_eq!(converted_to(&flac_to_flac, Target::Flac), None);

    // Lossless to lossless, different format: the reason to convert a WAV
    // rip at all.
    let wav = catalog_of_codec("/m/a/01.wav", "pcm", true);
    assert_eq!(converted_to(&wav, Target::Flac), Some(Target::Flac));
}

#[test]
fn a_converted_file_carries_the_new_extension() {
    use transcode::Target;
    let catalog = catalog_of_codec("/m/a/01 Crazy Train.flac", "flac", true);
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            convert: Some(Target::Mp3),
            ..Default::default()
        },
    );
    assert_eq!(plan.items[0].relative, Path::new("a/01 Crazy Train.mp3"));
    assert_eq!(plan.converted(), 1);
    assert!(plan.size_is_estimated(), "an encoder's output is a guess");
}

#[test]
fn two_sources_landing_on_one_name_do_not_become_one_file() {
    use transcode::Target;
    // `01.flac` and `01.wav` both want to be `01.mp3`. Writing the second
    // over the first would lose a track and report a complete copy — which
    // is why the extension changes before the name is placed.
    let mut a = file("/m/a/01.flac", "A");
    a.tags.properties.lossless = true;
    a.tags.properties.duration_ms = Some(1000);
    let mut b = file("/m/a/01.wav", "A");
    b.tags.properties.codec = "pcm".into();
    b.tags.properties.lossless = true;
    b.tags.properties.duration_ms = Some(1000);
    let catalog = model::build(vec![a, b], vec!["/m".into()], 0, &[]);
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            convert: Some(Target::Mp3),
            ..Default::default()
        },
    );
    let places: BTreeSet<PathBuf> = plan.items.iter().map(|i| i.relative.clone()).collect();
    assert_eq!(places.len(), 2, "two sources, two destinations: {places:?}");
}

#[test]
fn a_title_whose_own_dot_is_not_an_extension_keeps_it() {
    use transcode::Target;
    // `Path::set_extension` takes everything after the first dot on some
    // platforms and would turn this into `Vol.mp3`.
    assert_eq!(
        with_extension("a/Vol. 2 - Live.flac", "mp3"),
        "a/Vol. 2 - Live.mp3"
    );
    assert_eq!(with_extension("no folder.flac", "opus"), "no folder.opus");
    assert_eq!(
        with_extension("a/no extension", "mp3"),
        "a/no extension.mp3"
    );
    let _ = Target::Mp3;
}

#[test]
fn a_cover_and_an_extended_tag_lost_to_wav_are_both_counted() {
    use transcode::Target;
    // The source carries an embedded cover and a composer tag, neither of
    // which a `--compress wav` file can hold — the first because ffmpeg
    // refuses to mux a picture into a WAV at all, the second because the
    // format's legacy INFO chunk has no field for it.
    let mut f = file("/m/a/01.flac", "A");
    f.tags.properties.lossless = true;
    f.tags.properties.duration_ms = Some(1000);
    f.tags.has_embedded_art = true;
    f.tags.insert("composer", "Miles Davis");
    let catalog = model::build(vec![f], vec!["/m".into()], 0, &[]);
    let wav_plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            convert: Some(Target::Wav),
            ..Default::default()
        },
    );
    assert_eq!(wav_plan.covers_the_target_cannot_hold, 1);
    assert_eq!(wav_plan.tags_the_target_cannot_hold, 1);

    // The same file converted to MP3 loses neither: MP3 carries the cover and
    // its tag format takes an arbitrary key.
    let mp3_plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            convert: Some(Target::Mp3),
            ..Default::default()
        },
    );
    assert_eq!(mp3_plan.covers_the_target_cannot_hold, 0);
    assert_eq!(mp3_plan.tags_the_target_cannot_hold, 0);

    // A plain copy encodes nothing, so neither count applies to it either.
    let uncoverted = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::None,
            ..Default::default()
        },
    );
    assert_eq!(uncoverted.covers_the_target_cannot_hold, 0);
    assert_eq!(uncoverted.tags_the_target_cannot_hold, 0);
}

#[test]
fn a_word_that_names_no_extras_is_refused_rather_than_guessed_at() {
    assert_eq!(Extras::parse("cover"), Some(Extras::Cover));
    assert_eq!(Extras::parse("COVERS"), Some(Extras::Cover));
    assert_eq!(Extras::parse("none"), Some(Extras::None));
    assert_eq!(Extras::parse("everything"), None);
}

#[test]
fn extras_reach_one_level_into_a_subfolder_beside_the_audio() {
    // Aède writes what it draws beside a track into a subfolder of its own —
    // `spectrograms/` — rather than loose next to the audio file, and
    // `beside` used to look at the album folder's own files only: a
    // directory entry there failed `file_type().is_file()` and was skipped
    // outright, whatever `--extras` asked for. Both `Extras::Images` and
    // `Extras::All` document reaching a spectrogram; neither did.
    let dir = std::env::temp_dir().join("aede_copy_extras_subfolder_test");
    let _ = std::fs::remove_dir_all(&dir);
    let album = dir.join("Danzig").join("1994 Danzig 4");
    std::fs::create_dir_all(album.join("spectrograms")).unwrap();
    std::fs::write(album.join("02.flac"), b"fake audio").unwrap();
    std::fs::write(album.join("cover.jpg"), b"fake cover").unwrap();
    std::fs::write(album.join("spectrograms").join("02.png"), b"fake png").unwrap();
    // A subfolder inside the subfolder is one level too many, and must not
    // appear: nothing Aède writes today is shaped like that, and reaching
    // for it would be a guess.
    std::fs::create_dir_all(album.join("spectrograms").join("too-deep")).unwrap();
    std::fs::write(
        album.join("spectrograms").join("too-deep").join("x.png"),
        b"should not travel",
    )
    .unwrap();

    let audio_path = album.join("02.flac").to_string_lossy().to_string();
    let catalog = catalog_of(&[(&audio_path, "Danzig 4")], &[dir.to_str().unwrap()]);
    let plan = plan(
        &catalog,
        &all_tracks(&catalog),
        &Recipe {
            extras: Extras::All,
            ..Default::default()
        },
    );
    let extras: Vec<PathBuf> = plan
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Other)
        .map(|i| i.relative.clone())
        .collect();
    assert!(
        extras.iter().any(|p| p.ends_with("spectrograms/02.png")),
        "the spectrogram must travel too: {extras:?}"
    );
    assert!(
        extras.iter().any(|p| p.ends_with("cover.jpg")),
        "an ordinary file beside the audio must still travel: {extras:?}"
    );
    assert!(
        !extras
            .iter()
            .any(|p| p.components().any(|c| c.as_os_str() == "too-deep")),
        "a second level down is not walked: {extras:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
