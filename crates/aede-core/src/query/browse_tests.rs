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
