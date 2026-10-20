use super::*;

#[test]
fn playlist_text_input_preserves_the_exact_limit_and_refuses_excess_bytes() {
    let directory = crate::store_lock::test_support::Directory::new("playlist_text_limit");
    let path = directory.path().join("selection.m3u8");
    let original = "\u{feff}#EXTM3U\r\n one.flac\r\n";
    std::fs::write(&path, original).unwrap();
    assert_eq!(read_text(&path).unwrap(), original);
    // A single comment may occupy the complete budget without being truncated.
    let mut file = std::fs::File::create(&path).unwrap();
    std::io::Write::write_all(&mut file, b"#").unwrap();
    let limit = 16 * 1024 * 1024;
    file.set_len(limit).unwrap();
    let text = read_text(&path).unwrap();
    assert_eq!(text.len() as u64, limit);
    assert_eq!(text.as_bytes()[0], b'#');
    drop(text);
    file.set_len(limit + 1).unwrap();
    let error = read_text(&path).unwrap_err();
    assert!(error.contains("playlist exceeds the 16 MiB input limit"));
    assert!(error.contains(path.to_str().unwrap()));
    assert_eq!(file.metadata().unwrap().len(), limit + 1);
    drop(file);
    std::fs::write(&path, [0xff]).unwrap();
    assert!(read_text(&path).unwrap_err().contains("UTF-8"));
    assert!(read_text(directory.path()).is_err());
}

fn one_track(path: &str) -> Catalog {
    crate::model::build(
        vec![crate::model::ScannedFile {
            path: path.into(),
            size: 1,
            mtime: 0,
            tags: crate::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![],
        0,
        &[],
    )
}

#[test]
fn a_line_break_in_a_path_cannot_inject_another_playlist_entry() {
    let catalog = one_track("/music/real.flac\nhttps://unwanted.invalid/track");
    let text = render(&catalog, &[0], None, Style::Extended);
    assert!(!text.contains("https://unwanted"));
    assert!(
        !text.contains("#EXTINF"),
        "an unrepresentable path has no metadata row either"
    );
    assert!(try_render(&catalog, &[0], None, Style::Extended).is_err());
}

#[test]
fn relative_entries_with_initial_whitespace_resolve_without_renaming_files() {
    let parent = std::env::temp_dir().join("aede-playlist-whitespace");
    for relative in [
        " Leading.flac",
        "\u{3000}Wide.flac",
        " Leading/01.flac",
        "#Hash.flac",
        "\u{feff}Marked.flac",
    ] {
        let path = parent.join(relative);
        let catalog = one_track(path.to_str().unwrap());
        let tracks = vec![catalog.tracks[0].id; 2];
        for style in [Style::Simple, Style::Extended] {
            let rendered = try_render(&catalog, &tracks, Some(&parent), style).unwrap();
            assert_eq!(
                crate::playlist::selection::resolve(&catalog, &rendered, &parent).unwrap(),
                tracks,
                "the rendered paths must retain literal initial characters and repeated occurrences"
            );
            assert_eq!(
                rendered
                    .lines()
                    .filter(|line| !line.starts_with('#'))
                    .collect::<Vec<_>>(),
                vec![format!("./{relative}"); 2]
            );
        }
        assert_eq!(
            render(&catalog, &[tracks[0]], None, Style::Simple),
            format!("{}\n", path.display()),
            "absolute source paths retain their original spelling"
        );
    }
}

#[test]
fn relative_file_scheme_names_resolve_as_catalogued_files() {
    let parent = std::env::temp_dir().join("aede-playlist-file-scheme");
    for relative in ["file:track.flac", "file:Album/01.flac"] {
        let path = parent.join(relative);
        let catalog = one_track(path.to_str().unwrap());
        let tracks = vec![catalog.tracks[0].id; 2];
        for style in [Style::Simple, Style::Extended] {
            let rendered = try_render(&catalog, &tracks, Some(&parent), style).unwrap();
            assert_eq!(
                crate::playlist::selection::resolve(&catalog, &rendered, &parent).unwrap(),
                tracks,
                "a literal file: name must not become a file URI"
            );
            assert_eq!(
                rendered
                    .lines()
                    .filter(|line| !line.starts_with('#'))
                    .collect::<Vec<_>>(),
                vec![format!("./{relative}"); 2]
            );
        }
        assert_eq!(
            render(&catalog, &[tracks[0]], None, Style::Simple),
            format!("{}\n", path.display()),
            "absolute paths retain their original spelling"
        );
        assert!(
            crate::playlist::selection::resolve(&catalog, "file:///track.flac\n", &parent).is_err(),
            "actual file URIs remain unsupported"
        );
    }
}

#[test]
fn rounding_an_extreme_duration_does_not_overflow() {
    let mut catalog = one_track("/music/track.flac");
    catalog.tracks[0].duration_ms = Some(u64::MAX);
    let text = render(&catalog, &[0], None, Style::Extended);
    assert!(text.contains("#EXTINF:18446744073709552,"));
}

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
    let long = "album/track.flac\n".repeat(2048);
    std::fs::write(&path, &long).unwrap();
    assert!(already_says(&path, &long));
    let mut changed = long.clone().into_bytes();
    changed[8192] = b'X';
    std::fs::write(&path, changed).unwrap();
    assert!(!already_says(&path, &long), "a later chunk differs");
    std::fs::write(&path, format!("{long}extra")).unwrap();
    assert!(!already_says(&path, &long), "extra bytes are not ignored");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    assert!(!already_says(&path, "#EXTM3U\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn special_playlist_destinations_are_not_opened_or_treated_as_current() {
    const CHILD_PATH: &str = "AEDE_PLAYLIST_SPECIAL_TEST_PATH";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        assert!(!already_says(Path::new(&path), ""));
        return;
    }

    let directory = crate::store_lock::test_support::Directory::new("playlist_special");
    let fifo = directory.path().join("pipe.m3u");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    // An older blocking reader must not leave the test suite stuck on open.
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "playlist::tests::special_playlist_destinations_are_not_opened_or_treated_as_current",
            "--nocapture",
        ])
        .env(CHILD_PATH, &fifo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    assert!(!timed_out, "the playlist comparison blocked on a FIFO");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let regular = directory.path().join("regular.m3u");
    let link = directory.path().join("linked.m3u");
    let content = "#EXTM3U\n01.flac\n";
    std::fs::write(&regular, content).unwrap();
    std::os::unix::fs::symlink(&regular, &link).unwrap();
    assert!(already_says(&regular, content));
    assert!(
        !already_says(&link, content),
        "a link is not an accepted output"
    );
    assert!(write_atomic(&link, content).is_err());
    assert_eq!(std::fs::read_to_string(&regular).unwrap(), content);
    assert!(!already_says(directory.path(), content));
}
