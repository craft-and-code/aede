use super::*;
use crate::model::{Artist, Release, Track};
use crate::user::UserData;
use serde_json::json;
fn filters() -> Value {
    json!({"view":"albums","mode":"years","years":[1990,2000],"alphabet":[0,26],"includeUnknownYear":false,"query":""})
}
#[test]
fn dynamic_filters_accept_future_releases_and_hidden_rails_do_not_filter() {
    let mut c = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Ensemble".into(),
            ..Default::default()
        }],
        releases: vec![Release {
            id: 0,
            title: "First".into(),
            year: Some(1995),
            album_artist_id: Some(0),
            track_ids: vec![0],
            ..Default::default()
        }],
        tracks: vec![Track {
            id: 0,
            release_id: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };
    let d = UserData::default();
    let f = parse(&filters().to_string()).unwrap();
    assert_eq!(tracks(&f, &Context::new(&c, &d, "owner")), vec![0]);
    let mut release = c.releases[0].clone();
    release.id = 1;
    release.track_ids = vec![1];
    c.releases.push(release);
    c.tracks.push(Track {
        id: 1,
        release_id: Some(1),
        ..Default::default()
    });
    assert_eq!(tracks(&f, &Context::new(&c, &d, "owner")), vec![0, 1]);
    let mut v = filters();
    v["years"] = json!([0, 0]);
    v["alphabet"] = json!([26, 26]);
    v["mode"] = json!("none");
    assert_eq!(
        tracks(
            &parse(&v.to_string()).unwrap(),
            &Context::new(&c, &d, "owner")
        ),
        vec![0, 1]
    );
    v["mode"] = json!("alphabet");
    assert!(
        tracks(
            &parse(&v.to_string()).unwrap(),
            &Context::new(&c, &d, "owner")
        )
        .is_empty()
    );
}
#[test]
fn malformed_filters_and_unknown_fields_are_refused() {
    for (key, value) in [
        ("years", json!([2000, 1990])),
        ("alphabet", json!([0, 27])),
        ("view", json!("history")),
        ("country", json!("US")),
        ("includeUnknownYear", Value::Null),
    ] {
        let mut v = filters();
        v[key] = value;
        assert!(parse(&v.to_string()).is_err(), "{key}");
    }
}

#[test]
fn multiple_genres_are_a_bounded_union_and_legacy_genre_remains_supported() {
    let mut value = filters();
    value["genres"] = json!(["Metal", "Rock"]);
    assert!(parse(&value.to_string()).is_ok());
    value["genres"] = json!([""]);
    assert!(parse(&value.to_string()).is_err());
    value["genres"] = json!([42]);
    assert!(parse(&value.to_string()).is_err());
    value["genres"] = json!(vec!["Rock"; 257]);
    assert!(parse(&value.to_string()).is_err());
    value.as_object_mut().unwrap().remove("genres");
    value["genre"] = json!("Rock");
    assert!(parse(&value.to_string()).is_ok());
}

#[test]
fn genre_union_includes_each_matching_artist_and_still_intersects_countries() {
    use crate::model::{EntityKind, Genre, GenreLink};
    let c = Catalog {
        artists: vec![
            Artist {
                id: 0,
                name: "First ensemble".into(),
                ..Default::default()
            },
            Artist {
                id: 1,
                name: "Second ensemble".into(),
                ..Default::default()
            },
        ],
        releases: vec![
            Release {
                id: 0,
                title: "First".into(),
                year: Some(1995),
                album_artist_id: Some(0),
                track_ids: vec![0],
                ..Default::default()
            },
            Release {
                id: 1,
                title: "Second".into(),
                year: Some(1996),
                album_artist_id: Some(1),
                track_ids: vec![1],
                ..Default::default()
            },
        ],
        tracks: vec![
            Track {
                id: 0,
                release_id: Some(0),
                ..Default::default()
            },
            Track {
                id: 1,
                release_id: Some(1),
                ..Default::default()
            },
        ],
        genres: vec![
            Genre {
                id: 0,
                name: "Metal".into(),
                key: "metal".into(),
            },
            Genre {
                id: 1,
                name: "Rock".into(),
                key: "rock".into(),
            },
        ],
        genre_links: vec![
            GenreLink {
                genre_id: 0,
                entity_kind: EntityKind::Release,
                entity_id: 0,
            },
            GenreLink {
                genre_id: 1,
                entity_kind: EntityKind::Release,
                entity_id: 1,
            },
        ],
        ..Default::default()
    };
    let data = UserData::default();
    let context = Context::new(&c, &data, "owner");
    let mut value = filters();
    value["genres"] = json!(["Metal", "Rock"]);
    assert_eq!(
        tracks(&parse(&value.to_string()).unwrap(), &context),
        vec![0, 1]
    );
    value["genres"] = json!(["Rock"]);
    assert_eq!(
        tracks(&parse(&value.to_string()).unwrap(), &context),
        vec![1]
    );
    value["countries"] = json!(["FR"]);
    assert!(tracks(&parse(&value.to_string()).unwrap(), &context).is_empty());
}

#[test]
fn personal_browsing_uses_principal_ratings_and_owner_favourites_dynamically() {
    let c = Catalog {
        artists: vec![
            Artist {
                id: 0,
                name: "Ensemble".into(),
                key: "ensemble".into(),
                ..Default::default()
            },
            Artist {
                id: 1,
                name: "Guest".into(),
                key: "guest".into(),
                ..Default::default()
            },
        ],
        releases: (0..3)
            .map(|id| Release {
                id,
                title: format!("Edition {id}"),
                year: Some(1995),
                album_artist_id: Some(if id == 2 { 1 } else { 0 }),
                track_ids: vec![id],
                ..Default::default()
            })
            .collect(),
        tracks: (0..3)
            .map(|id| Track {
                id,
                release_id: Some(id),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut data = UserData::default();
    let artist = EntityRef::of(&c, EntityKind::Artist, 0).unwrap();
    let release = EntityRef::of(&c, EntityKind::Release, 2).unwrap();
    data.entry("owner", &artist, 1).rating = Some(4);
    data.entry("other", &release, 1).rating = Some(3);
    let mut value = filters();
    value["view"] = json!("rated");
    value["ratings"] = json!([2, 4]);
    let f = parse(&value.to_string()).unwrap();
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![0, 1]);
    assert_eq!(tracks(&f, &Context::new(&c, &data, "other")), vec![2]);
    data.entry("owner", &release, 2).rating = Some(3);
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![0, 1, 2]);
    data.entry("owner", &artist, 3).rating = Some(5);
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![2]);
    value["view"] = json!("favourites");
    value.as_object_mut().unwrap().remove("ratings");
    let f = parse(&value.to_string()).unwrap();
    assert!(tracks(&f, &Context::new(&c, &data, "owner")).is_empty());
    data.entry("owner", &artist, 4).loved = true;
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![0, 1]);
    data.entry("owner", &release, 5).loved = true;
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![0, 1, 2]);
}
#[test]
fn rating_collection_bounds_are_strict_and_never_ignored() {
    for bounds in [
        json!([0, 5]),
        json!([2, 6]),
        json!([4, 2]),
        json!([2]),
        json!([2.5, 4]),
        Value::Null,
    ] {
        let mut v = filters();
        v["view"] = json!("rated");
        v["ratings"] = bounds;
        assert!(parse(&v.to_string()).is_err());
    }
    let mut v = filters();
    v["view"] = json!("rated");
    assert!(parse(&v.to_string()).is_err());
    v["view"] = json!("albums");
    v["ratings"] = json!([1, 5]);
    assert!(parse(&v.to_string()).is_err());
}

#[test]
fn tagged_collections_follow_owner_artist_album_and_track_tags_without_guest_inheritance() {
    let c = Catalog {
        artists: (0..2)
            .map(|id| Artist {
                id,
                key: format!("artist-{id}"),
                ..Default::default()
            })
            .collect(),
        releases: (0..3)
            .map(|id| Release {
                id,
                key: format!("release-{id}"),
                title: format!("Edition {id}"),
                album_artist_id: Some(if id == 0 { 0 } else { 1 }),
                track_ids: vec![id * 2, id * 2 + 1],
                ..Default::default()
            })
            .collect(),
        files: (0..7)
            .map(|id| crate::model::AudioFile {
                id,
                path: format!("/music/track-{id}.flac"),
                ..Default::default()
            })
            .collect(),
        tracks: (0..7)
            .map(|id| Track {
                id,
                file_id: id,
                release_id: (id < 6).then_some(id / 2),
                ..Default::default()
            })
            .collect(),
        credits: vec![crate::model::Credit {
            artist_id: 0,
            entity_kind: EntityKind::Track,
            entity_id: 4,
            role: "featured".into(),
            credited_as: None,
            attributes: vec![],
            began: None,
            ended: None,
            order: None,
            source: "tags".into(),
            source_id: None,
        }],
        ..Default::default()
    };
    let mut data = UserData::default();
    let artist = EntityRef::of(&c, EntityKind::Artist, 0).unwrap();
    let album = EntityRef::of(&c, EntityKind::Release, 1).unwrap();
    let track = EntityRef::of(&c, EntityKind::Track, 4).unwrap();
    let orphan = EntityRef::of(&c, EntityKind::Track, 6).unwrap();
    data.entry("owner", &artist, 1)
        .tags
        .insert("Énergie".into());
    data.entry("owner", &album, 1).tags.insert("Calme".into());
    data.entry("owner", &track, 1).tags.insert("Calme".into());
    data.entry("owner", &orphan, 1).tags.insert("Calme".into());
    data.entry("other", &album, 1).tags.insert("Énergie".into());
    let mut v = filters();
    v["view"] = json!("tags");
    v["mode"] = json!("none");
    v["tags"] = json!(["energie"]);
    let f = parse(&v.to_string()).unwrap();
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![0, 1]);
    assert_eq!(tracks(&f, &Context::new(&c, &data, "other")), vec![2, 3]);
    v["tags"] = json!(["energie", "calme"]);
    let f = parse(&v.to_string()).unwrap();
    assert_eq!(
        tracks(&f, &Context::new(&c, &data, "owner")),
        vec![0, 1, 2, 3, 4, 6]
    );
    v["tags"] = json!([]);
    v["query"] = json!("alm");
    let f = parse(&v.to_string()).unwrap();
    assert_eq!(
        tracks(&f, &Context::new(&c, &data, "owner")),
        vec![2, 3, 4, 6]
    );
    data.entry("owner", &album, 2).tags.clear();
    assert_eq!(tracks(&f, &Context::new(&c, &data, "owner")), vec![4, 6]);
}

#[test]
fn tag_collection_payloads_are_bounded_and_cannot_be_silently_ignored() {
    let mut v = filters();
    v["view"] = json!("tags");
    v["mode"] = json!("none");
    for tags in [
        Value::Null,
        json!("music"),
        json!([""]),
        json!([" "]),
        json!([5]),
        json!(["a".repeat(257)]),
        json!(vec!["a"; 101]),
    ] {
        v["tags"] = tags;
        assert!(parse(&v.to_string()).is_err());
    }
    v["tags"] = json!(["music"]);
    v["view"] = json!("albums");
    assert!(parse(&v.to_string()).is_err());
    v["view"] = json!("tags");
    v.as_object_mut().unwrap().remove("tags");
    assert!(parse(&v.to_string()).is_err());
}

#[test]
fn label_collections_keep_stable_membership_and_include_future_matching_editions() {
    use crate::model::Label;
    let mut c = Catalog {
        labels: vec![Label {
            id: 0,
            name: "Test Label".into(),
            key: "test-label".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let d = UserData::default();
    let mut value = filters();
    value["view"] = json!("label");
    value["label"] = json!(EntityRef::of(&c, EntityKind::Label, 0).unwrap().to_token());
    let f = parse(&value.to_string()).unwrap();
    for id in 0..3 {
        c.releases.push(Release {
            id,
            title: "First".into(),
            year: Some(if id == 2 { 2010 } else { 1995 }),
            track_ids: vec![id],
            label_ids: if id == 1 { vec![] } else { vec![0] },
            ..Default::default()
        });
    }
    assert_eq!(tracks(&f, &Context::new(&c, &d, "owner")), vec![0]);
    c.releases.push(Release {
        id: 3,
        title: "Future addition".into(),
        year: Some(1997),
        track_ids: vec![3],
        label_ids: vec![0],
        ..Default::default()
    });
    assert_eq!(tracks(&f, &Context::new(&c, &d, "owner")), vec![0, 3]);
    c.labels.clear();
    assert!(tracks(&f, &Context::new(&c, &d, "owner")).is_empty());
    value["label"] = json!("artist:other");
    assert!(parse(&value.to_string()).is_err());
    value.as_object_mut().unwrap().remove("label");
    assert!(parse(&value.to_string()).is_err());
    value["view"] = json!("albums");
    value["label"] = json!("label:test-label");
    assert!(parse(&value.to_string()).is_err());
}

#[test]
fn track_filters_keep_individual_membership_unknown_years_and_numeric_order() {
    let mut c = Catalog {
        releases: vec![Release {
            id: 0,
            title: "Shared album".into(),
            year: Some(1995),
            track_ids: vec![0, 1],
            ..Default::default()
        }],
        tracks: vec![
            Track {
                id: 0,
                title: "Alpha".into(),
                release_id: Some(0),
                ..Default::default()
            },
            Track {
                id: 1,
                title: "Beta".into(),
                release_id: Some(0),
                ..Default::default()
            },
            Track {
                id: 2,
                title: "Albumless".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let data = UserData::default();
    let mut value = filters();
    value["view"] = json!("tracks");
    value["query"] = json!("Alpha");
    let selected = |c: &Catalog, v: &Value| {
        tracks(
            &parse(&v.to_string()).unwrap(),
            &Context::new(c, &data, "owner"),
        )
    };
    assert_eq!(selected(&c, &value), vec![0]);
    value["query"] = json!("");
    value["mode"] = json!("both");
    value["alphabet"] = json!([1, 1]);
    assert_eq!(selected(&c, &value), vec![0]);
    value["includeUnknownYear"] = json!(true);
    assert_eq!(selected(&c, &value), vec![0, 2]);
    value["reverse"] = json!(true);
    assert_eq!(selected(&c, &value), vec![2, 0]);
    c.tracks.push(Track {
        id: 3,
        title: "Another".into(),
        release_id: Some(0),
        ..Default::default()
    });
    assert_eq!(selected(&c, &value), vec![3, 2, 0]);
    value["reverse"] = json!("yes");
    assert!(parse(&value.to_string()).is_err());
}

#[test]
fn composer_collections_select_whole_editions_by_exact_credit_and_year() {
    let mut c = Catalog {
        artists: vec![Artist {
            id: 0,
            key: "writer".into(),
            name: "Writer".into(),
            ..Default::default()
        }],
        releases: vec![Release {
            id: 0,
            key: "first".into(),
            title: "First".into(),
            year: Some(1995),
            track_ids: vec![0, 1],
            ..Default::default()
        }],
        tracks: vec![
            Track {
                id: 0,
                release_id: Some(0),
                ..Default::default()
            },
            Track {
                id: 1,
                release_id: Some(0),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let d = UserData::default();
    let mut f = filters();
    f["view"] = json!("composers");
    f["composers"] = json!(["artist:writer"]);
    let rule = parse(&f.to_string()).unwrap();
    assert!(tracks(&rule, &Context::new(&c, &d, "owner")).is_empty());
    c.credits.push(crate::model::Credit {
        artist_id: 0,
        entity_kind: EntityKind::Track,
        entity_id: 0,
        role: "composer".into(),
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    });
    assert_eq!(tracks(&rule, &Context::new(&c, &d, "owner")), [0, 1]);
    f["composers"] = json!(["artist:absent"]);
    assert!(
        tracks(
            &parse(&f.to_string()).unwrap(),
            &Context::new(&c, &d, "owner")
        )
        .is_empty()
    );
    f["composers"] = json!([]);
    assert_eq!(
        tracks(
            &parse(&f.to_string()).unwrap(),
            &Context::new(&c, &d, "owner")
        ),
        [0, 1]
    );
    f["years"] = json!([2000, 2020]);
    assert!(
        tracks(
            &parse(&f.to_string()).unwrap(),
            &Context::new(&c, &d, "owner")
        )
        .is_empty()
    );
    for invalid in [
        json!(["release:first"]),
        json!([42]),
        json!(vec!["artist:writer"; 101]),
    ] {
        f["composers"] = invalid;
        assert!(parse(&f.to_string()).is_err());
    }
    f["view"] = json!("albums");
    f["composers"] = json!([]);
    assert!(parse(&f.to_string()).is_err());
}
