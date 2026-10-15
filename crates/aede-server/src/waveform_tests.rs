use super::*;
use crate::accounts_test_support::{ADMIN_TOKEN, Fixture, http};
use crate::playback_test_support::install_wav;
use crate::test_support::{encoded, start_server, test_runtime};
#[test]
fn waveform_cache_reuses_complete_source_data_and_refuses_changed_audio() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let track = install_wav(&fixture, 16000).await;
        let original = fs::read(&track.path).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let path = format!("/api/v1/waveform?ref={}", encoded(&track.reference));
        let first = http(address, "GET", &path, Some(ADMIN_TOKEN), "");
        assert_eq!(first.0, 200);
        let first: View = serde_json::from_value(first.1).unwrap();
        assert_eq!(first.duration_ms, 2000);
        assert!(valid(&first));
        let cache = fs::read_dir(fixture.0.data_dir.join("waveforms"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let before = fs::metadata(&cache).unwrap().modified().unwrap();
        let second = http(address, "GET", &path, Some(ADMIN_TOKEN), "");
        assert_eq!(second.0, 200);
        assert_eq!(fs::metadata(&cache).unwrap().modified().unwrap(), before);
        assert_eq!(fs::read(&track.path).unwrap(), original);
        fs::write(&track.path, [original.as_slice(), &[0]].concat()).unwrap();
        assert_eq!(http(address, "GET", &path, Some(ADMIN_TOKEN), "").0, 409);
        assert_eq!(
            http(
                address,
                "GET",
                "/api/v1/waveform?ref=release:one",
                Some(ADMIN_TOKEN),
                ""
            )
            .0,
            400
        );
        server.abort();
    });
}
