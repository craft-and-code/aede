//! Local navigation honours the requested output, scope and window.

mod navigation_m0_support;

use aede_core::json;
use navigation_m0_support::Library;

#[test]
fn search_outputs_write_requested_files_and_refuse_an_export_without_a_format() {
    let library = Library::new("Jazz");
    let output = library.dir.join("search.json");
    for term in ["Needle", "missing-name"] {
        let displayed = library.success(&[
            "search",
            term,
            "--json",
            "--output",
            output.to_str().unwrap(),
        ]);
        assert!(output.is_file(), "search ignored the requested JSON file");
        let text = std::fs::read_to_string(&output).unwrap();
        let exported = json::parse(&text).unwrap();
        let rows = exported.as_arr().unwrap();
        assert_eq!(rows.is_empty(), term == "missing-name");
        assert!(!displayed.trim_start().starts_with('['));
    }
    let catalog_path = aede_core::store::catalog_path(&library.dir);
    aede_core::store::save(&aede_core::model::Catalog::default(), &catalog_path).unwrap();
    let original = std::fs::read(&catalog_path).unwrap();
    let unformatted = library.dir.join("unformatted.txt");
    let result = library.run(&[
        "search",
        "Needle",
        "--output",
        unformatted.to_str().unwrap(),
    ]);
    assert!(
        !result.status.success(),
        "search ignored --output on an empty catalog"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("--output writes what"));
    assert!(!unformatted.exists());
    assert_eq!(std::fs::read(&catalog_path).unwrap(), original);
    let output = library.success(&["search", "Needle", "--json"]);
    assert!(json::parse(&output).unwrap().as_arr().unwrap().is_empty());
    let output = library.success(&["search", "Needle", "--csv"]);
    assert_eq!(output.lines().count(), 1);
}

#[test]
fn search_track_exports_page_after_projecting_and_deduplicating_hits() {
    let library = Library::new("Jazz");
    for extra in [vec![], vec!["--comments"]] {
        let mut args = vec!["search", "Needle", "--m3u", "--offset=1", "--limit=1"];
        args.extend(extra);
        let output = library.success(&args);
        let paths: Vec<_> = output
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect();
        assert_eq!(paths, vec!["/music/Needle Album 1/02.flac"]);
    }
    let output = library.success(&["search", "Needle", "--csv", "--comments", "--limit=1"]);
    assert_eq!(
        output.lines().count(),
        2,
        "CSV window includes exactly one track"
    );
    assert!(output.contains("Needle 11"));
    assert!(!output.contains("Needle 12"));
}

#[test]
fn album_exports_keep_every_track_of_the_paged_editions() {
    let library = Library::new("Jazz");
    let output = library.success(&["album", "Needle Album", "--m3u", "--offset=1", "--limit=2"]);
    let paths: Vec<_> = output
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(
        paths,
        vec![
            "/music/Needle Album 2/01.flac",
            "/music/Needle Album 2/02.flac",
            "/music/Needle Album 3/01.flac",
            "/music/Needle Album 3/02.flac",
        ]
    );
    let output = library.success(&["album", "Needle Album 1", "--json", "--limit=1"]);
    assert_eq!(json::parse(&output).unwrap().as_arr().unwrap().len(), 2);
    let output = library.success(&["album", "Needle Album 1", "--json", "--offset=9"]);
    assert_eq!(json::parse(&output).unwrap().as_arr().unwrap().len(), 0);
    let output = library.success(&["album", "Needle Album", "--m3u", "--all"]);
    assert_eq!(
        output.lines().filter(|line| !line.starts_with('#')).count(),
        6
    );
}

#[test]
fn local_artist_tables_and_track_selections_honour_the_window() {
    let mut library = Library::new("Jazz");
    // Album identifiers follow paths, not release years. Narrowed human and
    // exported track pages must still agree when those orders differ.
    library.catalog.releases[0].year = Some(2003);
    library.catalog.releases[2].year = Some(2001);
    aede_core::store::save(
        &library.catalog,
        &aede_core::store::catalog_path(&library.dir),
    )
    .unwrap();
    let output = library.success(&["artist", "Lead", "--offset=1", "--limit=1"]);
    let discography = output
        .split("Discography")
        .nth(1)
        .unwrap()
        .split("Played with")
        .next()
        .unwrap();
    assert!(!discography.contains("Needle Album 1"));
    assert!(discography.contains("Needle Album 2"));
    assert!(!discography.contains("Needle Album 3"));
    for (narrowing, expected) in [
        (vec![], "Needle 12"),
        (vec!["--role=main"], "Needle 32"),
        (vec!["--with=Guest"], "Needle 32"),
    ] {
        let mut args = vec!["artist", "Lead", "--json", "--offset=1", "--limit=1"];
        args.extend(narrowing);
        let output = library.success(&args);
        let parsed = json::parse(&output).unwrap();
        let rows = parsed.as_arr().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("title").and_then(|value| value.as_str()),
            Some(expected)
        );
        let mut empty_args = args.clone();
        empty_args[3] = "--offset=99";
        let output = library.success(&empty_args);
        assert!(json::parse(&output).unwrap().as_arr().unwrap().is_empty());
    }
    for narrowing in ["--role=main", "--with=Guest"] {
        let output = library.success(&["artist", "Lead", narrowing, "--offset=1", "--limit=1"]);
        assert!(!output.contains("Needle 11"));
        assert!(output.contains("Needle 32"));
        assert!(!output.contains("Needle 21"));
    }
    let output = library.success(&["artist", "Lead", "--all", "--json"]);
    assert_eq!(json::parse(&output).unwrap().as_arr().unwrap().len(), 6);
}

#[test]
fn incompatible_artist_questions_are_refused_instead_of_ignored() {
    let library = Library::new("Jazz");
    for options in [
        vec!["--with=Guest", "--role=main"],
        vec!["--members", "--role=main"],
        vec!["--members", "--with=Guest"],
    ] {
        let mut args = vec!["artist", "Lead"];
        args.extend(options);
        let result = library.run(&args);
        assert!(!result.status.success(), "ignored option in {args:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("cannot be combined"));
    }
    for option in [
        "--json",
        "--csv",
        "--m3u",
        "--offset=1",
        "--limit=1",
        "--all",
    ] {
        let result = library.run(&["artist", "Lead", "--members", option]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("options are not supported"));
    }
    assert!(
        !library
            .run(&["artist", "Lead", "--limit=0"])
            .status
            .success()
    );
}

#[test]
fn artist_genre_tags_display_literally_and_exports_keep_original_values() {
    let genre = "Jazz\x1b]52;c;dGVzdA==\x07\rgenre";
    let library = Library::new(genre);
    let output = library.success(&["artist", "Lead"]);
    assert!(
        !output
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
    );
    assert!(output.contains("\\u{1b}]52"));
    assert!(output.contains("\\u{7}\\rgenre"));
    let output = library.success(&["genres", "--json"]);
    let exported = json::parse(&output).unwrap();
    // Semicolons separate genre values during catalog construction; exports
    // preserve each stored name, including its original control characters.
    let mut actual: Vec<_> = exported
        .as_arr()
        .unwrap()
        .iter()
        .map(|row| row.get("genre").and_then(|value| value.as_str()).unwrap())
        .collect();
    let mut expected: Vec<_> = library
        .catalog
        .genres
        .iter()
        .map(|genre| genre.name.as_str())
        .collect();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn facet_exports_preserve_the_sorted_window_including_an_empty_page() {
    let library = Library::new("Jazz");
    for (command, column, expected) in [
        ("genres", "genre", "Facet 2"),
        ("labels", "label", "Label 2"),
    ] {
        let output =
            library.success(&[command, "--json", "--sort=name", "--offset=1", "--limit=1"]);
        let parsed = json::parse(&output).unwrap();
        let rows = parsed.as_arr().unwrap();
        assert_eq!(rows.len(), 1, "{command} ignored its export window");
        assert_eq!(
            rows[0].get(column).and_then(|value| value.as_str()),
            Some(expected)
        );
        let output = library.success(&[command, "--csv", "--sort=name", "--offset=1", "--limit=1"]);
        assert_eq!(output.lines().count(), 2);
        assert!(output.contains(expected));
        let output = library.success(&[command, "--json", "--offset=99"]);
        assert!(json::parse(&output).unwrap().as_arr().unwrap().is_empty());
        let output = library.success(&[command, "--csv", "--offset=99"]);
        assert_eq!(output.lines().count(), 1);
    }
}

#[test]
fn year_exports_apply_the_requested_sort_and_refuse_an_unknown_column() {
    let library = Library::new("Jazz");
    let output = library.success(&["years", "--json", "--sort=year-"]);
    let parsed = json::parse(&output).unwrap();
    let years: Vec<_> = parsed
        .as_arr()
        .unwrap()
        .iter()
        .map(|row| row.get("year").unwrap().as_u32().unwrap())
        .collect();
    assert_eq!(years, vec![2003, 2002, 2001]);
    let output = library.success(&["years", "--csv", "--sort=year-"]);
    assert!(output.lines().nth(1).unwrap().starts_with("2003,"));
    for format in ["--json", "--csv"] {
        let result = library.run(&["years", format, "--sort=banana"]);
        assert!(
            !result.status.success(),
            "years export ignored an invalid sort"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("not something to sort on"));
    }
}
