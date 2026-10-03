//! Real HTTP workflows: client authentication, binary media and personal data.

use super::artwork::test_support as pictures;
use super::test_support::*;
use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn head_requests_cannot_change_favourites_or_record_a_scrobble() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let token = key(&fixture, "alice");
    let id = opaque_id(
        fixture.0.catalog.try_read().unwrap().as_ref().unwrap(),
        EntityKind::Track,
        0,
    )
    .unwrap();
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    for method in ["star", "scrobble"] {
        let (status, _, bytes) = request(
            address,
            "HEAD",
            &format!("/rest/{method}?u=alice&p={token}&v=1.16.1&c=test&id={id}"),
            "",
            "",
        );
        let untouched = aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
            .unwrap()
            .is_none();
        assert!(
            untouched,
            "HEAD {method} must leave the personal store untouched"
        );
        assert_eq!(status, 405, "HEAD {method} must be rejected");
        assert!(bytes.is_empty());
    }
    server.abort();
}

#[test]
fn legacy_key_client_browses_artwork_and_keeps_its_playlist_rating_and_scrobble_private() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let audio = runtime.block_on(crate::playback_test_support::install_wav(&fixture, 800));
    let original = std::fs::read(&audio.path).unwrap();
    let picture = pictures::LARGE_PNG;
    let (cover_id, cover_path) = runtime.block_on(pictures::install(&fixture, picture));
    store::save_catalog_only(
        fixture.0.catalog.try_read().unwrap().as_ref().unwrap(),
        &store::catalog_path(&fixture.0.data_dir),
    )
    .unwrap();
    let token = key(&fixture, "alice");
    let bob = key(&fixture, "bob");
    let auditor = key(&fixture, "auditor");
    let common = format!("u=alice&p={token}&v=1.16.1&c=legacy-client&f=json");
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    let get = |method: &str, parameters: &str| {
        let mut path = format!("/rest/{method}.view?{common}");
        if !parameters.is_empty() {
            path.push('&');
            path.push_str(parameters);
        }
        let value = json_request(address, &path);
        assert_eq!(value["subsonic-response"]["status"], "ok", "{value}");
        value["subsonic-response"].clone()
    };
    let post = |method: &str, parameters: &str| {
        let (status, _, bytes) = request(
            address,
            "POST",
            &format!("/rest/{method}.view"),
            "Content-Type: application/x-www-form-urlencoded\r\n",
            &format!("{common}&{parameters}"),
        );
        assert_eq!(status, 200);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["subsonic-response"]["status"], "ok", "{value}");
        value["subsonic-response"].clone()
    };

    let found = get("search3", "query=&songCount=1&albumCount=1");
    let song = &found["searchResult3"]["song"][0];
    let song_id = song["id"].as_str().unwrap();
    let album_id = found["searchResult3"]["album"][0]["id"].as_str().unwrap();
    assert_eq!(song["coverArt"], cover_id);
    assert_eq!(song["playCount"], 0);
    let album = get("getAlbum", &format!("id={album_id}"));
    assert_eq!(album["album"]["song"][0]["id"], song_id);
    assert_eq!(album["album"]["coverArt"], cover_id);
    for size in ["", "&size=1"] {
        let (status, headers, bytes) = request(
            address,
            "GET",
            &format!("/rest/getCoverArt.view?{common}&id={cover_id}{size}"),
            "",
            "",
        );
        assert_eq!(status, 200);
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-type: image/png")
        );
        if size.is_empty() {
            assert_eq!(bytes, picture);
        } else {
            assert_ne!(bytes, picture);
            aede_core::coverart::render_image(&bytes, None).unwrap();
        }
    }
    let (status, _, bytes) = request(
        address,
        "GET",
        &format!("/rest/stream.view?{common}&id={song_id}"),
        "",
        "",
    );
    assert_eq!(status, 200);
    assert_eq!(bytes, original);
    assert!(
        aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
            .unwrap()
            .is_none()
    );

    post("star", &format!("id={song_id}&albumId={album_id}"));
    post("setRating", &format!("id={song_id}&rating=5"));
    let created = post(
        "createPlaylist",
        &format!("name=Evening&songId={song_id}&songId={song_id}"),
    );
    let playlist_id = created["playlist"]["id"].as_str().unwrap();
    assert_eq!(created["playlist"]["songCount"], 2);
    assert_eq!(created["playlist"]["owner"], "alice");
    assert_eq!(created["playlist"]["public"], false);
    post(
        "updatePlaylist",
        &format!(
            "playlistId={playlist_id}&comment=Private+mix&songIndexToRemove=0&songIdToAdd={song_id}"
        ),
    );
    let playlist = get("getPlaylist", &format!("id={playlist_id}"));
    assert_eq!(playlist["playlist"]["comment"], "Private mix");
    let entries = playlist["playlist"]["entry"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry["id"] == song_id
        && entry["userRating"] == 5
        && entry["coverArt"] == cover_id));

    post("scrobble", &format!("id={song_id}&submission=false"));
    assert_eq!(
        get("getNowPlaying", "")["nowPlaying"]["entry"][0]["id"],
        song_id
    );
    post(
        "scrobble",
        &format!("id={song_id}&time=1234&submission=true"),
    );
    let starred = get("getStarred2", "");
    assert_eq!(starred["starred2"]["song"][0]["userRating"], 5);
    assert_eq!(starred["starred2"]["song"][0]["playCount"], 1);
    assert_eq!(
        get("getSong", &format!("id={song_id}"))["song"]["playCount"],
        1
    );

    let other = json_request(
        address,
        &format!("/rest/getSong?apiKey={bob}&v=1.16.1&c=test&f=json&id={song_id}"),
    );
    assert_eq!(other["subsonic-response"]["song"]["playCount"], 0);
    assert!(other["subsonic-response"]["song"].get("starred").is_none());
    assert!(
        other["subsonic-response"]["song"]
            .get("userRating")
            .is_none()
    );
    let denied = json_request(
        address,
        &format!("/rest/getPlaylist?apiKey={bob}&v=1.16.1&c=test&f=json&id={playlist_id}"),
    );
    assert_eq!(denied["subsonic-response"]["error"]["code"], 70);
    let denied = json_request(
        address,
        &format!("/rest/star?apiKey={auditor}&v=1.16.1&c=test&f=json&id={song_id}"),
    );
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);
    let (status, _, image) = request(
        address,
        "GET",
        &format!("/rest/getCoverArt?apiKey={auditor}&v=1.16.1&c=test&id={cover_id}"),
        "",
        "",
    );
    assert_eq!(status, 200);
    assert_eq!(image, picture);

    let data = aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
        .unwrap()
        .unwrap();
    assert!(
        data.plays.is_empty(),
        "client declarations never invent native duration/completion evidence"
    );
    assert_eq!(data.scrobbles.len(), 1);
    assert_eq!(data.scrobbles[0].at_ms, 1234);
    assert_eq!(data.counts.len(), 1);
    assert_eq!(data.counts[0].count, 1);
    assert_eq!(data.playlists.len(), 1);
    assert_eq!(std::fs::read(&audio.path).unwrap(), original);
    assert_eq!(std::fs::read(cover_path).unwrap(), picture);
    server.abort();
}
