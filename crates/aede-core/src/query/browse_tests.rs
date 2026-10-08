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
