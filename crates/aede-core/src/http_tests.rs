use super::*;

#[test]
fn the_user_agent_carries_a_way_to_reach_us() {
    // The format MusicBrainz documents. A generic one is throttled as part
    // of a shared pool, so getting this wrong is not a cosmetic mistake.
    let agent = Client::identify("aede", "0.2.0", "https://example.org/aede");
    assert_eq!(agent, "aede/0.2.0 ( https://example.org/aede )");
}

#[test]
fn the_second_request_waits_for_its_turn() {
    // No network involved: the throttle is a clock, and a clock can be
    // tested. This is the one part of the client that has to be right on
    // the first run, because getting it wrong means a 503 for everything.
    let mut client = Client::new("test", Duration::from_millis(120));
    let start = Instant::now();
    client.wait_turn();
    assert!(
        start.elapsed() < Duration::from_millis(50),
        "the first request waits for nobody"
    );
    client.wait_turn();
    assert!(
        start.elapsed() >= Duration::from_millis(120),
        "the second waited: {:?}",
        start.elapsed()
    );
}
