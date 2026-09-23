use super::*;

#[test]
fn the_message_names_what_wanted_it_and_how_to_get_it() {
    // "ffmpeg was not found" alone leaves the reader wondering what they
    // have lost and what to do about it.
    let text = missing("--compress");
    assert!(text.starts_with("--compress needs ffmpeg"), "{text}");
    assert!(text.contains("brew install ffmpeg"), "{text}");
    assert!(missing("spectrum").starts_with("spectrum needs ffmpeg"));
}
