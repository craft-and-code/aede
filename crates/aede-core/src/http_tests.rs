use super::*;

#[test]
fn the_decoded_response_is_bounded_before_its_remaining_bytes_are_read() {
    let mut decoded = std::io::Cursor::new(vec![b'x'; 1024]);
    assert_eq!(read_limited(&mut decoded, 16), Err(Error::BodyTooLarge(16)));
    assert_eq!(
        decoded.position(),
        17,
        "one byte beyond the limit is enough"
    );
}

#[test]
fn artwork_between_ten_and_thirty_two_mib_is_downloaded() {
    let bytes = vec![0xFF; 10 * 1024 * 1024 + 1];
    let mut body = ureq::Body::builder().data(bytes.clone());
    assert_eq!(read_bytes_body(&mut body).expect("below our limit"), bytes);
}

#[test]
fn the_body_limit_accepts_its_boundary_and_refuses_larger_unknown_length_streams() {
    use std::io::Read;
    let mut exact = ureq::Body::builder().reader(std::io::repeat(b'x').take(MAX_BODY));
    assert_eq!(
        read_bytes_body(&mut exact).expect("at the limit").len() as u64,
        MAX_BODY
    );
    let mut oversized = ureq::Body::builder().reader(std::io::repeat(b'x'));
    assert_eq!(
        read_bytes_body(&mut oversized),
        Err(Error::BodyTooLarge(MAX_BODY))
    );
}

#[test]
fn an_invalid_utf8_json_answer_is_refused_without_replacing_the_source_text() {
    let mut body = ureq::Body::builder()
        .mime_type("text/plain")
        .data(b"{\"name\":\"Bj\xFFrk\"}".to_vec());
    assert!(read_json_body(&mut body).is_err());
}

#[test]
fn the_user_agent_carries_a_way_to_reach_us() {
    // The format MusicBrainz documents. A generic one is throttled as part
    // of a shared pool, so getting this wrong is not a cosmetic mistake.
    let agent = Client::identify("aede", "0.3.0", "https://example.org/aede");
    assert_eq!(agent, "aede/0.3.0 ( https://example.org/aede )");
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
