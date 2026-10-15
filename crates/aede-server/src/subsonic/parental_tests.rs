use super::test_support::*;
use super::*;
use crate::accounts_test_support::{Fixture, login};
use crate::parental_test_support::{policy, save_policy};

#[tokio::test]
async fn parental_changes_invalidate_an_existing_original_transfer_identity() {
    let fixture = Fixture::new();
    let token = key(&fixture, "alice");
    let parameters = Parameters::from_pairs(&[("apiKey", &token)]);
    let identity = authentication::authenticate(&fixture.0, &parameters)
        .await
        .unwrap();
    let state = crate::parental::scoped(fixture.0.clone(), identity.owner.clone(), false, true)
        .await
        .unwrap();
    authentication::recheck(&state, &identity).await.unwrap();
    let mut rules = aede_core::parental::ParentalStore::default();
    rules
        .set_policy(
            &identity.owner,
            serde_json::from_value(policy(vec![])).unwrap(),
            0,
        )
        .unwrap();
    rules
        .save(&aede_core::parental::path(&state.data_dir))
        .unwrap();
    assert!(authentication::recheck(&state, &identity).await.is_err());
    assert!(authentication::recheck_blocking(&state, &identity).is_err());
}

#[tokio::test]
async fn parental_subsonic_browsing_is_filtered_and_original_transfers_are_refused() {
    let fixture = Fixture::new();
    let token = key(&fixture, "alice");
    let other = key(&fixture, "bob");
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        save_policy(address, &admin, "alice", policy(vec!["artist:ac/dc"]));
        let page = json_request(
            address,
            &format!("/rest/getArtists?apiKey={token}&v=1.16.1&c=test&f=json"),
        );
        assert_eq!(page["subsonic-response"]["status"], "ok", "{page}");
        assert!(
            page["subsonic-response"]["artists"]["index"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let page = json_request(
            address,
            &format!("/rest/getArtists?apiKey={other}&v=1.16.1&c=test&f=json"),
        );
        assert!(
            !page["subsonic-response"]["artists"]["index"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        for method in ["stream", "download", "getPlaylists"] {
            let answer = json_request(
                address,
                &format!("/rest/{method}?apiKey={token}&v=1.16.1&c=test&f=json"),
            );
            assert_eq!(answer["subsonic-response"]["error"]["code"], 50, "{answer}");
        }
    })
    .await
    .unwrap();
    server.abort();
}
