use super::*;

#[test]
fn windows_playlists_are_relative_without_rewriting_absolute_verbatim_paths() {
    for root in [
        r"C:\Music",
        r"\\?\C:\Music",
        r"\\nas\music",
        r"\\?\UNC\nas\music",
    ] {
        let path = format!("{root}\\Artist\\Album\\01.flac");
        let base = format!("{root}\\Artist");
        assert_eq!(relative_to(&path, Some(Path::new(&base))), "Album/01.flac");
        assert_eq!(relative_to(&path, None), path);
        assert_eq!(relative_to(&path, Some(Path::new(r"D:\Other"))), path);
    }
}

#[test]
fn a_playlist_beside_its_music_names_it_relatively() {
    assert_eq!(
        relative_to("/m/Album/01.flac", Some(Path::new("/m/Album"))),
        "01.flac"
    );
    // From the artist folder, the album is part of the name.
    assert_eq!(
        relative_to("/m/Artist/Album/01.flac", Some(Path::new("/m/Artist"))),
        "Album/01.flac"
    );
    // No base at all: a playlist that may be written anywhere.
    assert_eq!(relative_to("/m/Album/01.flac", None), "/m/Album/01.flac");
    // A track that is not under the base is named the long way rather than
    // dropped — a playlist missing a track without saying so is worse.
    assert_eq!(
        relative_to("/elsewhere/01.flac", Some(Path::new("/m/Album"))),
        "/elsewhere/01.flac"
    );
    // A base written with a trailing separator must not eat the first
    // letter of the name.
    assert_eq!(
        relative_to("/m/Album/01.flac", Some(Path::new("/m/Album/"))),
        "01.flac"
    );
}

#[test]
fn the_playlist_is_named_after_its_folder() {
    assert_eq!(
        file_name(Path::new("/m/1959 Kind of Blue [FLAC]")),
        "1959 Kind of Blue [FLAC].m3u"
    );
    assert_eq!(file_name(Path::new("/")), "playlist.m3u");
}

#[test]
fn an_unchanged_playlist_is_recognised_and_a_changed_one_is_not() {
    let dir = std::env::temp_dir().join("aede_playlist_same");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.m3u");
    assert!(!already_says(&path, "x"), "nothing there yet");
    std::fs::write(&path, "#EXTM3U\n01.flac\n").unwrap();
    assert!(already_says(&path, "#EXTM3U\n01.flac\n"));
    assert!(!already_says(&path, "#EXTM3U\n01.flac\n02.flac\n"));
    let _ = std::fs::remove_dir_all(&dir);
}
