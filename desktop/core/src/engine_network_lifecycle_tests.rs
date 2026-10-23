use super::*;
#[path = "engine_network_lifecycle_test_support.rs"]
mod support;
use support::{NetworkFixture, Renderer};

#[test]
fn failed_network_play_allows_desktop_stop_status_and_restart() {
    let mut fixture = NetworkFixture::new();
    let renderer = Renderer::new(true);
    fixture.play(&renderer);
    fixture.wait_for_status("failed");
    assert_eq!(renderer.snapshot().descriptions, 1);
    fixture.stop_and_restart(None);
}

#[test]
fn completed_network_play_allows_desktop_stop_status_and_restart() {
    let mut fixture = NetworkFixture::new();
    let renderer = Renderer::new(false);
    fixture.play(&renderer);
    let uri = fixture.wait_for_playing(&renderer);
    assert_eq!(support::read_original(&uri), fixture.original);
    renderer.finish();
    fixture.wait_for_status("completed");
    assert_eq!(renderer.snapshot().stops, 0);
    fixture.stop_and_restart(Some(&uri));
}

#[test]
fn desktop_stop_cancels_active_network_play_and_releases_its_media_listener() {
    let mut fixture = NetworkFixture::new();
    let renderer = Renderer::new(false);
    fixture.play(&renderer);
    let uri = fixture.wait_for_playing(&renderer);
    assert_eq!(support::read_original(&uri), fixture.original);
    assert_eq!(fixture.network_status()["status"], "running");
    fixture.stop_and_restart(Some(&uri));
    assert_eq!(renderer.snapshot().stops, 1);
}
