use super::*;
use aede_core::model::{AudioFile, Credit, Genre, GenreLink};
use aede_core::tags::AudioProperties;

fn catalog() -> Catalog {
    let mut catalog = Catalog {
        scanned_at: 1_700_000_000,
        roots: vec!["/private/music".into()],
        artists: vec![
            Artist {
                id: 0,
                name: "The Écho".into(),
                sort_name: "Écho, The".into(),
                key: "echo".into(),
                aliases: vec!["Échos".into()],
                ..Default::default()
            },
            Artist {
                id: 1,
                name: "Björk".into(),
                sort_name: "Björk".into(),
                key: "bjork".into(),
                mbid: Some("artist-mbid".into()),
                ..Default::default()
            },
            Artist {
                id: 2,
                name: "Writer".into(),
                sort_name: "Writer".into(),
                key: "writer".into(),
                ..Default::default()
            },
            Artist {
                id: 3,
                name: "Guest".into(),
                sort_name: "Guest".into(),
                key: "guest".into(),
                ..Default::default()
            },
        ],
        releases: vec![
            Release {
                id: 0,
                title: "Zebra".into(),
                album_artist_id: Some(0),
                year: Some(2020),
                folder: "/private/music/zebra".into(),
                cover_path: Some("/private/music/zebra/cover.jpg".into()),
                track_ids: vec![0, 1],
                ..Default::default()
            },
            Release {
                id: 1,
                title: "Árbor".into(),
                album_artist_id: Some(1),
                year: Some(2000),
                folder: "/private/music/arbor".into(),
                track_ids: vec![2],
                ..Default::default()
            },
            Release {
                id: 2,
                title: "Compilation".into(),
                is_compilation: true,
                folder: "/private/music/compilation".into(),
                track_ids: vec![3],
                ..Default::default()
            },
            Release {
                id: 3,
                title: "Alpha".into(),
                album_artist_id: Some(0),
                year: Some(2000),
                folder: "/private/music/alpha".into(),
                ..Default::default()
            },
        ],
        tracks: vec![
            Track {
                id: 0,
                file_id: 0,
                title: "First".into(),
                release_id: Some(0),
                disc_no: Some(1),
                track_no: Some(2),
                duration_ms: Some(1500),
                ..Default::default()
            },
            Track {
                id: 1,
                file_id: 1,
                title: "Another".into(),
                release_id: Some(0),
                disc_no: Some(2),
                track_no: Some(1),
                duration_ms: Some(1750),
                ..Default::default()
            },
            Track {
                id: 2,
                file_id: 2,
                title: "Island".into(),
                release_id: Some(1),
                duration_ms: Some(3000),
                ..Default::default()
            },
            Track {
                id: 3,
                file_id: 3,
                title: "Party".into(),
                release_id: Some(2),
                ..Default::default()
            },
            Track {
                id: 4,
                file_id: 4,
                title: "Lone".into(),
                ..Default::default()
            },
        ],
        genres: vec![
            Genre {
                id: 0,
                name: "Rock".into(),
                key: "rock".into(),
            },
            Genre {
                id: 1,
                name: "Electronic".into(),
                key: "electronic".into(),
            },
            Genre {
                id: 2,
                name: "Unused".into(),
                key: "unused".into(),
            },
        ],
        ..Default::default()
    };
    catalog.files = (0..5)
        .map(|id| AudioFile {
            id,
            path: format!("/private/music/{id}.flac"),
            size: 42,
            properties: AudioProperties {
                codec: "flac".into(),
                container: "flac".into(),
                bitrate_kbps: Some(900),
                ..Default::default()
            },
            tags: [("comment".into(), vec!["/private/credentials".into()])].into(),
            ..Default::default()
        })
        .collect();
    catalog.credits = vec![
        credit(0, 0, "main"),
        credit(0, 3, "featured"),
        credit(0, 3, "featured"),
        credit(0, 2, "composer"),
        credit(0, 2, "performer"),
        credit(2, 1, "main"),
        credit(3, 3, "main"),
    ];
    catalog.genre_links = vec![
        genre_link(EntityKind::Release, 0, 0),
        genre_link(EntityKind::Release, 0, 0),
        genre_link(EntityKind::Release, 1, 1),
        genre_link(EntityKind::Track, 0, 1),
        genre_link(EntityKind::Track, 0, 1),
    ];
    catalog
}

fn credit(track: Id, artist: Id, role: &str) -> Credit {
    Credit {
        entity_kind: EntityKind::Track,
        entity_id: track,
        artist_id: artist,
        role: role.into(),
        source: "tags".into(),
        credited_as: None,
        attributes: Vec::new(),
        began: None,
        ended: None,
        order: None,
        source_id: None,
    }
}

fn genre_link(kind: EntityKind, id: Id, genre: Id) -> GenreLink {
    GenreLink {
        entity_kind: kind,
        entity_id: id,
        genre_id: genre,
    }
}

fn call(catalog: &Catalog, method: &str, parameters: &[(&str, &str)]) -> Value {
    dispatch(method, &Parameters::from_pairs(parameters), catalog).unwrap()
}

fn list_names(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect()
}

fn failure(catalog: &Catalog, method: &str, parameters: &[(&str, &str)]) -> u32 {
    dispatch(method, &Parameters::from_pairs(parameters), catalog)
        .unwrap_err()
        .code
}

#[test]
fn browsing_uses_one_logical_library_and_opaque_ids_without_private_paths() {
    let catalog = catalog();
    assert_eq!(
        call(&catalog, "getMusicFolders", &[]),
        json!({"musicFolders": {"musicFolder": [{"id": 1, "name": "Aède"}]}})
    );
    let artists = call(&catalog, "getArtists", &[("musicFolderId", "1")]);
    let mut serialized = artists.to_string();
    for (method, kind, id) in [
        ("getArtist", EntityKind::Artist, 0),
        ("getAlbum", EntityKind::Release, 0),
        ("getSong", EntityKind::Track, 0),
    ] {
        let id = opaque_id(&catalog, kind, id).unwrap();
        serialized.push_str(&call(&catalog, method, &[("id", &id)]).to_string());
        assert!(!id.contains("private"));
    }
    for private in ["/private", "cover_path", "\"path\"", "credentials"] {
        assert!(!serialized.contains(private), "{serialized}");
    }
}

#[test]
fn artist_indexes_are_sorted_with_accents_and_keep_zero_album_artists() {
    let value = call(&catalog(), "getArtists", &[]);
    assert_eq!(value["artists"]["ignoredArticles"], "");
    let groups = value["artists"]["index"].as_array().unwrap();
    assert_eq!(
        groups
            .iter()
            .map(|group| group["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["B", "E", "G", "W"]
    );
    assert_eq!(groups[0]["artist"][0]["name"], "Björk");
    assert_eq!(groups[0]["artist"][0]["musicBrainzId"], "artist-mbid");
    assert_eq!(groups[1]["artist"][0]["musicBrainzId"], "");
    assert_eq!(groups[1]["artist"][0]["albumCount"], 2);
    assert_eq!(groups[2]["artist"][0]["albumCount"], 0);
    assert_eq!(groups[3]["artist"][0]["albumCount"], 0);
}

#[test]
fn artist_albums_do_not_turn_composing_or_guest_credits_into_discography() {
    let catalog = catalog();
    for artist in [2, 3] {
        let id = opaque_id(&catalog, EntityKind::Artist, artist).unwrap();
        let value = call(&catalog, "getArtist", &[("id", &id)]);
        assert_eq!(value["artist"]["albumCount"], 0);
        assert_eq!(value["artist"]["album"], json!([]));
    }
    let id = opaque_id(&catalog, EntityKind::Artist, 0).unwrap();
    let value = call(&catalog, "getArtist", &[("id", &id)]);
    assert_eq!(list_names(&value["artist"]["album"]), ["Alpha", "Zebra"]);
}

#[test]
fn explicit_co_album_artist_gets_the_album_without_promoting_other_release_roles() {
    let mut catalog = catalog();
    for (artist, role) in [(3, "album"), (3, "album"), (2, "composer"), (2, "featured")] {
        let mut link = credit(0, artist, role);
        link.entity_kind = EntityKind::Release;
        catalog.credits.push(link);
    }
    let co_artist = opaque_id(&catalog, EntityKind::Artist, 3).unwrap();
    let value = call(&catalog, "getArtist", &[("id", &co_artist)]);
    assert_eq!(value["artist"]["albumCount"], 1);
    assert_eq!(list_names(&value["artist"]["album"]), ["Zebra"]);
    assert_eq!(
        list_names(&call(&catalog, "search3", &[("query", "Guest")])["searchResult3"]["album"]),
        ["Zebra"]
    );
    let writer = opaque_id(&catalog, EntityKind::Artist, 2).unwrap();
    assert_eq!(
        call(&catalog, "getArtist", &[("id", &writer)])["artist"]["albumCount"],
        0
    );
    let album = opaque_id(&catalog, EntityKind::Release, 0).unwrap();
    let value = call(&catalog, "getAlbum", &[("id", &album)]);
    assert_eq!(
        value["album"]["artistId"],
        opaque_id(&catalog, EntityKind::Artist, 0).unwrap()
    );
}

#[test]
fn album_detail_keeps_track_order_and_uses_scan_time_as_creation_proxy() {
    let catalog = catalog();
    let album_id = opaque_id(&catalog, EntityKind::Release, 0).unwrap();
    let value = call(&catalog, "getAlbum", &[("id", &album_id)]);
    let album = &value["album"];
    assert_eq!(album["songCount"], 2);
    assert_eq!(album["duration"], 3);
    assert_eq!(album["created"], "2023-11-14T22:13:20Z");
    assert_eq!(album["genre"], "Rock");
    assert_eq!(album["song"][0]["title"], "First");
    assert_eq!(album["song"][1]["title"], "Another");
    assert_eq!(album["song"][0]["albumId"], album_id);
    assert_eq!(album["song"][0]["parent"], album_id);
}

#[test]
fn songs_project_only_primary_and_featured_artists_with_album_fallback() {
    let catalog = catalog();
    let first = opaque_id(&catalog, EntityKind::Track, 0).unwrap();
    let song = call(&catalog, "getSong", &[("id", &first)]);
    assert_eq!(song["song"]["artist"], "The Écho / Guest");
    assert_eq!(
        song["song"]["artistId"],
        opaque_id(&catalog, EntityKind::Artist, 0).unwrap()
    );
    assert_eq!(song["song"]["track"], 2);
    assert_eq!(song["song"]["discNumber"], 1);
    assert_eq!(song["song"]["duration"], 1);
    assert_eq!(song["song"]["contentType"], "audio/flac");
    assert_eq!(song["song"]["suffix"], "flac");
    assert_eq!(song["song"]["bitRate"], 900);
    let another = opaque_id(&catalog, EntityKind::Track, 1).unwrap();
    assert_eq!(
        call(&catalog, "getSong", &[("id", &another)])["song"]["artist"],
        "The Écho"
    );
}

#[test]
fn compilations_and_tracks_without_an_album_remain_browsable() {
    let catalog = catalog();
    let album = opaque_id(&catalog, EntityKind::Release, 2).unwrap();
    let value = call(&catalog, "getAlbum", &[("id", &album)]);
    assert!(value["album"].get("artistId").is_none());
    assert_eq!(value["album"]["song"][0]["artist"], "Guest");
    let track = opaque_id(&catalog, EntityKind::Track, 4).unwrap();
    let value = call(&catalog, "getSong", &[("id", &track)]);
    assert_eq!(value["song"]["title"], "Lone");
    assert!(value["song"].get("albumId").is_none());
    assert!(value["song"].get("artistId").is_none());
    assert!(value["song"].get("duration").is_none());
}

#[test]
fn album_lists_sort_and_page_by_name_artist_or_year() {
    let catalog = catalog();
    let by_name = call(&catalog, "getAlbumList2", &[("type", "alphabeticalByName")]);
    assert_eq!(
        list_names(&by_name["albumList2"]["album"]),
        ["Alpha", "Árbor", "Compilation", "Zebra"]
    );
    let by_artist = call(
        &catalog,
        "getAlbumList2",
        &[("type", "alphabeticalByArtist")],
    );
    assert_eq!(
        list_names(&by_artist["albumList2"]["album"]),
        ["Compilation", "Árbor", "Alpha", "Zebra"]
    );
    let page = call(
        &catalog,
        "getAlbumList2",
        &[
            ("type", "alphabeticalByName"),
            ("offset", "1"),
            ("size", "2"),
        ],
    );
    assert_eq!(
        list_names(&page["albumList2"]["album"]),
        ["Árbor", "Compilation"]
    );
    for (from, to, expected) in [
        ("2000", "2020", vec!["Alpha", "Árbor", "Zebra"]),
        ("2020", "2000", vec!["Zebra", "Alpha", "Árbor"]),
        ("2001", "2019", vec![]),
    ] {
        let value = call(
            &catalog,
            "getAlbumList2",
            &[("type", "byYear"), ("fromYear", from), ("toYear", to)],
        );
        assert_eq!(list_names(&value["albumList2"]["album"]), expected);
    }
}

#[test]
fn empty_search_supports_independent_pages_and_zero_counts() {
    let value = call(
        &catalog(),
        "search3",
        &[
            ("query", ""),
            ("artistOffset", "1"),
            ("artistCount", "1"),
            ("albumOffset", "1"),
            ("albumCount", "2"),
            ("songOffset", "2"),
            ("songCount", "1"),
        ],
    );
    assert_eq!(value["searchResult3"]["artist"][0]["name"], "The Écho");
    assert_eq!(
        list_names(&value["searchResult3"]["album"]),
        ["Árbor", "Compilation"]
    );
    assert_eq!(value["searchResult3"]["song"][0]["title"], "Island");
    let zero = call(
        &catalog(),
        "search3",
        &[
            ("query", ""),
            ("artistCount", "0"),
            ("albumCount", "0"),
            ("songCount", "0"),
        ],
    );
    assert_eq!(
        zero,
        json!({"searchResult3": {"artist": [], "album": [], "song": []}})
    );
}

#[test]
fn search_matches_normalized_names_aliases_album_titles_and_song_artists() {
    let catalog = catalog();
    for query in ["BJORK", "Björk"] {
        let value = call(&catalog, "search3", &[("query", query)]);
        assert_eq!(value["searchResult3"]["artist"][0]["name"], "Björk");
        assert_eq!(list_names(&value["searchResult3"]["album"]), ["Árbor"]);
        assert_eq!(value["searchResult3"]["song"][0]["title"], "Island");
    }
    assert_eq!(
        call(&catalog, "search3", &[("query", "echos")])["searchResult3"]["artist"][0]["name"],
        "The Écho"
    );
    let album = call(&catalog, "search3", &[("query", "zebra")]);
    assert_eq!(album["searchResult3"]["song"].as_array().unwrap().len(), 2);
    let unknown = call(&catalog, "search3", &[("query", "/private/credentials")]);
    assert_eq!(
        unknown,
        json!({"searchResult3": {"artist": [], "album": [], "song": []}})
    );
}

#[test]
fn genre_counts_deduplicate_links_and_use_album_only_when_track_has_no_genre() {
    let catalog = catalog();
    assert_eq!(
        call(&catalog, "getGenres", &[]),
        json!({"genres": {"genre": [
            {"value": "Electronic", "songCount": 2, "albumCount": 1},
            {"value": "Rock", "songCount": 1, "albumCount": 1},
            {"value": "Unused", "songCount": 0, "albumCount": 0},
        ]}})
    );
    for (track, genre) in [(0, "Electronic"), (1, "Rock"), (2, "Electronic")] {
        let id = opaque_id(&catalog, EntityKind::Track, track).unwrap();
        assert_eq!(
            call(&catalog, "getSong", &[("id", &id)])["song"]["genre"],
            genre
        );
    }
}

#[test]
fn empty_catalog_responses_keep_arrays_and_unknown_ids_fail_cleanly() {
    let catalog = Catalog::default();
    assert_eq!(
        call(&catalog, "getArtists", &[]),
        json!({"artists": {"ignoredArticles": "", "index": []}})
    );
    assert_eq!(
        call(&catalog, "getGenres", &[]),
        json!({"genres": {"genre": []}})
    );
    assert_eq!(
        call(&catalog, "getAlbumList2", &[("type", "alphabeticalByName")]),
        json!({"albumList2": {"album": []}})
    );
    for method in ["getArtist", "getAlbum", "getSong"] {
        assert_eq!(failure(&catalog, method, &[("id", "unknown")]), 70);
    }
    assert!(track_reference(&catalog, "unknown").is_err());
}

#[test]
fn unsupported_options_and_invalid_pages_are_refused() {
    let catalog = catalog();
    for kind in [
        "newest", "highest", "frequent", "recent", "starred", "unknown",
    ] {
        assert_eq!(failure(&catalog, "getAlbumList2", &[("type", kind)]), 0);
    }
    for (method, pairs) in [
        ("getMusicFolders", vec![("musicFolderId", "1")]),
        ("getArtists", vec![("musicFolderId", "2")]),
        (
            "getAlbumList2",
            vec![("type", "alphabeticalByName"), ("fromYear", "2000")],
        ),
        (
            "getAlbumList2",
            vec![("type", "byYear"), ("fromYear", "2000")],
        ),
        (
            "getAlbumList2",
            vec![("type", "alphabeticalByName"), ("size", "501")],
        ),
        (
            "getAlbumList2",
            vec![("type", "alphabeticalByName"), ("offset", "-1")],
        ),
        ("search3", vec![]),
        ("search3", vec![("query", ""), ("songCount", "1001")]),
        (
            "search3",
            vec![("query", ""), ("songOffset", "18446744073709551616")],
        ),
        ("search3", vec![("query", ""), ("artistCount", "1.0")]),
        ("getSong", vec![]),
    ] {
        assert!(
            dispatch(method, &Parameters::from_pairs(&pairs), &catalog).is_err(),
            "{method} {pairs:?}"
        );
    }
    assert_eq!(
        failure(&catalog, "search3", &[("query", &"x".repeat(1025))]),
        0
    );
    assert_eq!(failure(&catalog, "notImplemented", &[]), 0);
    let artist = opaque_id(&catalog, EntityKind::Artist, 0).unwrap();
    assert_eq!(failure(&catalog, "getSong", &[("id", &artist)]), 70);
}

#[test]
fn full_library_sync_accepts_one_thousand_songs_per_page() {
    let value = call(
        &catalog(),
        "search3",
        &[
            ("query", "\"\""),
            ("songCount", "1000"),
            ("songOffset", "1"),
        ],
    );
    let songs = value["searchResult3"]["song"].as_array().unwrap();
    assert_eq!(songs.len(), 4);
    assert_eq!(songs[0]["title"], "First");
}

#[test]
fn supersonic_album_limit_alias_is_honoured_without_ambiguous_page_sizes() {
    let catalog = catalog();
    let value = call(
        &catalog,
        "getAlbumList2",
        &[
            ("type", "byYear"),
            ("fromYear", "2000"),
            ("toYear", "2020"),
            ("limit", "1"),
            ("offset", "1"),
        ],
    );
    assert_eq!(list_names(&value["albumList2"]["album"]), ["Árbor"]);
    for pairs in [
        vec![("type", "random"), ("limit", "501")],
        vec![("type", "random"), ("limit", "-1")],
        vec![("type", "random"), ("limit", "1"), ("size", "1")],
    ] {
        assert!(
            dispatch("getAlbumList2", &Parameters::from_pairs(&pairs), &catalog).is_err(),
            "{pairs:?}"
        );
    }
}

#[test]
fn genre_lists_match_named_genres_and_page_without_duplicates() {
    let catalog = catalog();
    assert_eq!(
        list_names(
            &call(
                &catalog,
                "getAlbumList2",
                &[("type", "byGenre"), ("genre", "rOck")]
            )["albumList2"]["album"]
        ),
        ["Zebra"]
    );
    let songs = call(
        &catalog,
        "getSongsByGenre",
        &[("genre", "Electronic"), ("count", "1"), ("offset", "1")],
    );
    assert_eq!(songs["songsByGenre"]["song"].as_array().unwrap().len(), 1);
    assert_eq!(songs["songsByGenre"]["song"][0]["title"], "Island");
    assert_eq!(
        call(&catalog, "getSongsByGenre", &[("genre", "Rock")])["songsByGenre"]["song"][0]["title"],
        "Another"
    );
    assert_eq!(
        call(&catalog, "getSongsByGenre", &[("genre", "missing")])["songsByGenre"]["song"],
        json!([])
    );
    for (method, pairs) in [
        ("getAlbumList2", vec![("type", "byGenre")]),
        (
            "getAlbumList2",
            vec![("type", "byGenre"), ("genre", "Rock"), ("fromYear", "2000")],
        ),
        (
            "getAlbumList2",
            vec![("type", "alphabeticalByName"), ("genre", "Rock")],
        ),
        ("getSongsByGenre", vec![("genre", "Rock"), ("count", "501")]),
        ("getSongsByGenre", vec![("genre", "Rock"), ("offset", "-1")]),
        (
            "getSongsByGenre",
            vec![("genre", "Rock"), ("musicFolderId", "2")],
        ),
        ("getSongsByGenre", vec![]),
    ] {
        assert!(
            dispatch(method, &Parameters::from_pairs(&pairs), &catalog).is_err(),
            "{method} {pairs:?}"
        );
    }
}

#[test]
fn random_lists_are_bounded_permutations_and_honour_filters() {
    let catalog = catalog();
    let albums = call(
        &catalog,
        "getAlbumList2",
        &[("type", "random"), ("size", "500")],
    );
    let names = list_names(&albums["albumList2"]["album"]);
    assert_eq!(names.len(), 4);
    assert_eq!(names.iter().copied().collect::<HashSet<_>>().len(), 4);
    assert_eq!(
        call(
            &catalog,
            "getAlbumList2",
            &[("type", "random"), ("offset", "99")]
        )["albumList2"]["album"],
        json!([])
    );
    let songs = call(
        &catalog,
        "getRandomSongs",
        &[
            ("genre", "Electronic"),
            ("fromYear", "2020"),
            ("toYear", "2020"),
            ("size", "500"),
        ],
    );
    assert_eq!(songs["randomSongs"]["song"].as_array().unwrap().len(), 1);
    assert_eq!(songs["randomSongs"]["song"][0]["title"], "First");
    let songs = call(&catalog, "getRandomSongs", &[("size", "500")]);
    let rows = songs["randomSongs"]["song"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows.iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<HashSet<_>>()
            .len(),
        5
    );
    for pairs in [
        vec![("size", "501")],
        vec![("fromYear", "-1")],
        vec![("fromYear", "2020"), ("toYear", "2000")],
        vec![("genre", "")],
        vec![("musicFolderId", "2")],
    ] {
        assert!(
            dispatch("getRandomSongs", &Parameters::from_pairs(&pairs), &catalog).is_err(),
            "{pairs:?}"
        );
    }
    assert_eq!(
        call(&catalog, "getRandomSongs", &[("size", "0")])["randomSongs"]["song"],
        json!([])
    );
}

#[test]
fn zero_sized_and_out_of_range_album_pages_are_empty() {
    for parameters in [
        vec![("type", "alphabeticalByName"), ("size", "0")],
        vec![("type", "alphabeticalByName"), ("offset", "999999999")],
    ] {
        assert_eq!(
            call(&catalog(), "getAlbumList2", &parameters)["albumList2"]["album"],
            json!([])
        );
    }
}

#[test]
fn opaque_song_ids_resolve_to_native_playback_references_only_inside_server() {
    let catalog = catalog();
    let id = opaque_id(&catalog, EntityKind::Track, 2).unwrap();
    assert_eq!(
        track_reference(&catalog, &id).unwrap(),
        EntityRef::of(&catalog, EntityKind::Track, 2).unwrap()
    );
    let album = opaque_id(&catalog, EntityKind::Release, 1).unwrap();
    assert!(track_reference(&catalog, &album).is_err());
}

#[test]
fn oversized_duration_saturates_without_panicking() {
    let mut catalog = catalog();
    catalog.tracks[0].duration_ms = Some(u64::MAX);
    catalog.tracks[1].duration_ms = Some(u64::MAX);
    let id = opaque_id(&catalog, EntityKind::Release, 0).unwrap();
    assert_eq!(
        call(&catalog, "getAlbum", &[("id", &id)])["album"]["duration"],
        u64::MAX / 1000
    );
}

#[test]
fn response_order_and_ids_survive_dense_id_reassignment() {
    let mut reordered = catalog();
    let expected = call(&reordered, "search3", &[("query", "")]);
    reordered.artists.reverse();
    for (at, artist) in reordered.artists.iter_mut().enumerate() {
        artist.id = at as Id;
    }
    reordered.releases.reverse();
    for (at, album) in reordered.releases.iter_mut().enumerate() {
        album.id = at as Id;
        album.album_artist_id = album.album_artist_id.map(|id| 3 - id);
        for track in &mut album.track_ids {
            *track = 4 - *track;
        }
    }
    reordered.tracks.reverse();
    for (at, song) in reordered.tracks.iter_mut().enumerate() {
        song.id = at as Id;
        song.release_id = song.release_id.map(|id| 3 - id);
    }
    for credit in &mut reordered.credits {
        credit.artist_id = 3 - credit.artist_id;
        credit.entity_id = 4 - credit.entity_id;
    }
    for genre in &mut reordered.genre_links {
        genre.entity_id = match genre.entity_kind {
            EntityKind::Release => 3 - genre.entity_id,
            EntityKind::Track => 4 - genre.entity_id,
            _ => genre.entity_id,
        };
    }
    assert_eq!(call(&reordered, "search3", &[("query", "")]), expected);
}

#[test]
fn malformed_dense_ids_or_ambiguous_stable_keys_fail_without_panic() {
    let mut invalid = catalog();
    invalid.artists[0].id = 1;
    assert!(dispatch("getArtists", &Parameters::default(), &invalid).is_err());
    let mut ambiguous = catalog();
    ambiguous.artists[1].key = ambiguous.artists[0].key.clone();
    assert!(dispatch("getArtists", &Parameters::default(), &ambiguous).is_err());
}

#[test]
fn shared_projection_resolves_only_allowed_kinds_and_keeps_references_private() {
    let catalog = catalog();
    let index = Index::new(&catalog).unwrap();
    for (kind, id) in [
        (EntityKind::Artist, 0),
        (EntityKind::Release, 1),
        (EntityKind::Track, 2),
    ] {
        let wanted = opaque_id(&catalog, kind, id).unwrap();
        let reference = index.reference(&wanted, &[kind]).unwrap();
        assert_eq!(reference, EntityRef::of(&catalog, kind, id).unwrap());
        assert_eq!(
            index.render_reference(&reference).unwrap(),
            index.render(kind, id).unwrap()
        );
        assert!(
            !index
                .render_reference(&reference)
                .unwrap()
                .to_string()
                .contains("/private")
        );
        assert!(index.reference(&wanted, &[]).is_err());
    }
    assert_eq!(index.render(EntityKind::Track, 99).unwrap_err().code, 70);
    assert_eq!(
        index
            .render_reference(&EntityRef::new(EntityKind::Track, "/missing"))
            .unwrap_err()
            .code,
        70
    );
    let mbid = EntityRef::new(EntityKind::Artist, "mbid:artist-mbid");
    assert_eq!(
        index.render_reference(&mbid).unwrap(),
        index.render(EntityKind::Artist, 1).unwrap()
    );
    let album = EntityRef::of(&catalog, EntityKind::Release, 0).unwrap();
    let legacy = EntityRef::new(EntityKind::Release, format!("{}/CD1", album.key));
    assert_eq!(
        index.render_reference(&legacy).unwrap(),
        index.render(EntityKind::Release, 0).unwrap()
    );
    assert_eq!(
        index
            .track_duration(&EntityRef::of(&catalog, EntityKind::Track, 0).unwrap())
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        index
            .track_duration(&EntityRef::of(&catalog, EntityKind::Track, 3).unwrap())
            .unwrap(),
        None
    );
    assert_eq!(index.track_duration(&album).unwrap_err().code, 70);
}

#[test]
fn album_sidecar_cover_ids_are_shared_by_its_songs_without_exposing_paths() {
    let mut catalog = catalog();
    let folder = std::env::temp_dir().join("aede_projection_cover");
    catalog.releases[0].folder = folder.to_string_lossy().into_owned();
    catalog.releases[0].cover_path = Some(folder.join("cover.png").to_string_lossy().into_owned());
    let index = Index::new(&catalog).unwrap();
    let album = index.render(EntityKind::Release, 0).unwrap();
    let cover = album["coverArt"].as_str().unwrap();
    assert!(cover.starts_with("cover-"));
    assert_ne!(cover, album["id"].as_str().unwrap());
    for id in [0, 1] {
        assert_eq!(
            index.render(EntityKind::Track, id).unwrap()["coverArt"],
            cover
        );
    }
    for (kind, id) in [
        (EntityKind::Artist, 0),
        (EntityKind::Release, 1),
        (EntityKind::Track, 4),
    ] {
        assert!(index.render(kind, id).unwrap().get("coverArt").is_none());
    }
    catalog.releases[0].cover_path = Some(folder.join("cover.gif").to_string_lossy().into_owned());
    assert!(
        Index::new(&catalog)
            .unwrap()
            .render(EntityKind::Release, 0)
            .unwrap()
            .get("coverArt")
            .is_none()
    );
}

#[test]
fn scan_status_reports_real_activity_and_last_published_snapshot() {
    for scanning in [false, true] {
        assert_eq!(
            scan_status(&catalog(), scanning).unwrap(),
            json!({"scanStatus": {
                "scanning": scanning, "count": 5
            }})
        );
    }
}
