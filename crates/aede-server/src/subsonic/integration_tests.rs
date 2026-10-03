use super::test_support::*;
use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn discovery_is_public_even_with_bad_credentials_and_authentication_is_protocol_scoped() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let token = key(&fixture, "alice");
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    let discovery = json_request(
        address,
        "/rest/getOpenSubsonicExtensions.view?f=json&apiKey=invalid",
    );
    assert_eq!(
        discovery["subsonic-response"]["openSubsonicExtensions"][0]["name"],
        "apiKeyAuthentication"
    );
    for _ in 0..8 {
        let value = json_request(
            address,
            &format!("/rest/ping.view?apiKey={token}&v=1.16.1&c=test&f=json"),
        );
        assert_eq!(value["subsonic-response"]["status"], "ok");
    }
    for (query, code) in [("u=alice&p=x", 42), ("u=alice&t=abc&s=salt", 41)] {
        let value = json_request(
            address,
            &format!("/rest/ping?{query}&v=1.16.1&c=test&f=json"),
        );
        assert_eq!(value["subsonic-response"]["error"]["code"], code);
    }
    let conflict = json_request(
        address,
        &format!("/rest/ping?apiKey={token}&u=alice&v=1.16.1&c=test&f=json"),
    );
    assert_eq!(conflict["subsonic-response"]["error"]["code"], 43);
    let (status, _, _) = request(
        address,
        "GET",
        "/api/v1/library",
        &format!("Authorization: Bearer {token}\r\n"),
        "",
    );
    assert_eq!(status, 401);
    let mut accounts = fixture.accounts();
    accounts
        .revoke_api_key("alice", token.split_once('.').unwrap().0)
        .unwrap();
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
    )
    .unwrap();
    let revoked = json_request(
        address,
        &format!("/rest/ping?apiKey={token}&v=1.16.1&c=test&f=json"),
    );
    assert_eq!(revoked["subsonic-response"]["error"]["code"], 44);
    std::fs::write(
        aede_core::accounts::accounts_path(&fixture.0.data_dir),
        "corrupt",
    )
    .unwrap();
    let public = json_request(address, "/rest/getOpenSubsonicExtensions?f=json");
    assert_eq!(public["subsonic-response"]["status"], "ok");
    server.abort();
}

#[test]
fn form_post_xml_json_and_unsupported_operations_obey_the_adapter_contract() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let token = key(&fixture, "alice");
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    let body = format!("apiKey={token}&v=1.16.1&c=test&f=json&query=&songCount=1");
    let (status, _, body) = request(
        address,
        "POST",
        "/rest/search3.view",
        "Content-Type: application/x-www-form-urlencoded\r\n",
        &body,
    );
    assert_eq!(status, 200);
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value["subsonic-response"]["searchResult3"]["song"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "{value}"
    );
    let path = format!(
        "/rest/getSong?apiKey={token}&v=1.16.1&c=test&id={}",
        opaque_id(
            fixture.0.catalog.try_read().unwrap().as_ref().unwrap(),
            EntityKind::Track,
            0
        )
        .unwrap()
    );
    let (_, headers, xml) = request(address, "GET", &path, "", "");
    let xml = String::from_utf8(xml).unwrap();
    assert!(headers.to_ascii_lowercase().contains("application/xml"));
    assert!(xml.contains("<song "));
    assert!(!xml.contains("/music/"));
    for endpoint in ["scrobble", "createUser", "startScan", "getCoverArt"] {
        let value = json_request(
            address,
            &format!("/rest/{endpoint}?apiKey={token}&v=1.16.1&c=test&f=json"),
        );
        assert_eq!(value["subsonic-response"]["status"], "failed");
    }
    let other = json_request(
        address,
        &format!("/rest/getUser?apiKey={token}&v=1.16.1&c=test&f=json&username=bob"),
    );
    assert_eq!(other["subsonic-response"]["error"]["code"], 50);
    let protected = format!("/rest/ping?apiKey={token}&v=1.16.1&c=test&f=json");
    for header in [
        "Origin: http://foreign.invalid\r\n",
        "Host: foreign.invalid\r\n",
    ] {
        let (status, _, _) = request(address, "GET", &protected, header, "");
        assert_eq!(status, 403);
    }
    let permit = fixture
        .0
        .inspection_slots
        .clone()
        .try_acquire_many_owned(2)
        .unwrap();
    let busy = json_request(
        address,
        &format!("/rest/getArtists?apiKey={token}&v=1.16.1&c=test&f=json"),
    );
    assert_eq!(busy["subsonic-response"]["status"], "failed");
    let cheap = json_request(address, &protected);
    assert_eq!(cheap["subsonic-response"]["status"], "ok");
    drop(permit);
    server.abort();
}

#[test]
fn original_audio_ranges_and_auditor_rules_preserve_bytes_without_recording_a_play() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let installed = runtime.block_on(crate::playback_test_support::install_wav(&fixture, 800));
    let original = std::fs::read(&installed.path).unwrap();
    let token = key(&fixture, "alice");
    let auditor = key(&fixture, "auditor");
    let id = opaque_id(
        fixture.0.catalog.try_read().unwrap().as_ref().unwrap(),
        EntityKind::Track,
        0,
    )
    .unwrap();
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    let path = format!("/rest/stream?apiKey={token}&v=1.16.1&c=test&id={id}");
    let (status, headers, bytes) = request(address, "GET", &path, "", "");
    assert_eq!(status, 200);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-type: audio/wav")
    );
    assert_eq!(bytes, original);
    let (status, headers, bytes) = request(address, "GET", &path, "Range: bytes=7-35\r\n", "");
    assert_eq!(status, 206);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-range: bytes 7-35/1644")
    );
    assert_eq!(bytes, original[7..36]);
    let form = format!("apiKey={token}&v=1.16.1&c=test&id={id}");
    let (status, headers, bytes) = request(
        address,
        "POST",
        "/rest/download.view",
        "Content-Type: application/x-www-form-urlencoded\r\n",
        &form,
    );
    assert_eq!(status, 200);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-disposition: attachment; filename=\"audio.wav\"")
    );
    assert_eq!(bytes, original);
    let (status, _, bytes) = request(
        address,
        "GET",
        &path,
        "Range: bytes=7-35\r\nIf-Range: \"unknown\"\r\n",
        "",
    );
    assert_eq!(status, 200);
    assert_eq!(bytes, original);
    let (status, headers, bytes) = request(address, "GET", &path, "Range: bytes=99999-\r\n", "");
    assert_eq!(status, 416);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-range: bytes */1644")
    );
    assert!(bytes.is_empty());
    let response = json_request(
        address,
        &format!("/rest/stream?apiKey={auditor}&v=1.16.1&c=test&id={id}&f=json"),
    );
    assert_eq!(response["subsonic-response"]["error"]["code"], 50);
    let unsupported = json_request(address, &format!("{path}&maxBitRate=128&f=json"));
    assert_eq!(unsupported["subsonic-response"]["status"], "failed");
    assert_eq!(std::fs::read(&installed.path).unwrap(), original);
    assert!(
        aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
            .unwrap()
            .is_none()
    );
    server.abort();
}

#[test]
fn revocation_stops_an_unconsumed_original_transfer_and_discards_buffered_bytes() {
    let runtime = crate::test_support::test_runtime();
    runtime.block_on(async {
        let fixture = Fixture::new();
        let installed = crate::playback_test_support::install_wav(&fixture, 100_000).await;
        let token = key(&fixture, "alice");
        let params = Parameters::from_pairs(&[("apiKey", &token)]);
        let identity = authentication::authenticate(&fixture.0, &params)
            .await
            .unwrap();
        let track = EntityRef::parse_token(&installed.reference).unwrap();
        let response = media::response(fixture.0.clone(), identity, track, HeaderMap::new(), false)
            .await
            .unwrap();
        let mut accounts = fixture.accounts();
        accounts
            .revoke_api_key("alice", token.split_once('.').unwrap().0)
            .unwrap();
        aede_core::accounts::save(
            &accounts,
            &aede_core::accounts::accounts_path(&fixture.0.data_dir),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
        assert!(
            aede_core::user::load(&aede_core::user::user_path(&fixture.0.data_dir))
                .unwrap()
                .is_none()
        );
    });
}
