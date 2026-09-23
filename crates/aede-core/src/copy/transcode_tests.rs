use super::*;

#[test]
fn a_target_is_named_the_way_a_person_would_name_it() {
    assert_eq!(Target::parse("mp3"), Some(Target::Mp3));
    assert_eq!(Target::parse("MP3"), Some(Target::Mp3));
    // Two words for one thing, because a user asking for "aac" and one
    // asking for "m4a" mean the same file.
    assert_eq!(Target::parse("aac"), Some(Target::Aac));
    assert_eq!(Target::parse("m4a"), Some(Target::Aac));
    assert_eq!(Target::parse("ogg"), Some(Target::Vorbis));
    assert_eq!(Target::parse("wma"), None);
    assert_eq!(
        Target::Aac.extension(),
        "m4a",
        "the container, not the codec"
    );
}

#[test]
fn a_quality_that_parses_as_nothing_is_refused() {
    // An encoder silently running at a setting nobody asked for produces
    // files that are wrong in a way nobody notices until the card is full.
    assert_eq!(Quality::parse("V0"), Some(Quality::Variable(0)));
    assert_eq!(Quality::parse("v2"), Some(Quality::Variable(2)));
    assert_eq!(Quality::parse("q6"), Some(Quality::Variable(6)));
    assert_eq!(Quality::parse("192k"), Some(Quality::Bitrate(192)));
    assert_eq!(Quality::parse("320"), Some(Quality::Bitrate(320)));
    assert_eq!(Quality::parse("best"), None);
    assert_eq!(Quality::parse("V99"), None, "no such level");
    assert_eq!(Quality::parse("1k"), None, "not a bitrate anybody means");
}

#[test]
fn each_encoder_is_asked_in_its_own_terms() {
    // MP3 counts down and Vorbis counts up; flattening the two into one
    // number would give somebody asking for the best Vorbis the worst one.
    assert_eq!(
        quality_arguments(Target::Mp3, None),
        vec!["-q:a".to_string(), "0".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Vorbis, None),
        vec!["-q:a".to_string(), "6".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Opus, None),
        vec!["-b:a".to_string(), "128k".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Mp3, Some(Quality::Bitrate(320))),
        vec!["-b:a".to_string(), "320k".to_string()]
    );
    // A lossless target has no knob, so it is given none rather than one
    // that would be ignored.
    assert!(quality_arguments(Target::Flac, Some(Quality::Bitrate(320))).is_empty());
}

#[test]
fn only_mp3_aac_and_flac_can_carry_a_cover_across() {
    // The three containers ffmpeg can mux a picture stream into — and,
    // proven separately by running real ffmpeg against each target, the
    // exact three it does not refuse outright.
    assert!(Target::Mp3.keeps_embedded_art());
    assert!(Target::Aac.keeps_embedded_art());
    assert!(Target::Flac.keeps_embedded_art());
    assert!(!Target::Wav.keeps_embedded_art());
    assert!(!Target::Opus.keeps_embedded_art());
    assert!(!Target::Vorbis.keeps_embedded_art());
}

#[test]
fn wav_drops_what_its_legacy_info_chunk_has_no_room_for() {
    let mut present = BTreeMap::new();
    for key in ["title", "artist", "album", "genre", "date", "tracknumber"] {
        present.insert(key.to_string(), vec!["x".to_string()]);
    }
    assert!(
        Target::Wav.tags_it_would_drop(&present).is_empty(),
        "the six wav is known to keep"
    );

    present.insert("composer".to_string(), vec!["x".to_string()]);
    present.insert("albumartist".to_string(), vec!["x".to_string()]);
    let mut dropped = Target::Wav.tags_it_would_drop(&present);
    dropped.sort();
    assert_eq!(
        dropped,
        vec!["albumartist".to_string(), "composer".to_string()]
    );

    // Every other target's tag format takes an arbitrary key: nothing
    // named here because nothing here is known to be lost.
    assert!(Target::Mp3.tags_it_would_drop(&present).is_empty());
    assert!(Target::Opus.tags_it_would_drop(&present).is_empty());
    assert!(Target::Vorbis.tags_it_would_drop(&present).is_empty());
}

#[test]
fn a_size_is_estimated_from_the_playing_time() {
    // Four minutes at 128 kbps is about 3.8 MB, and the point of the figure
    // is to answer "will this fit" before an hour of encoding.
    let four_minutes = 240_000;
    let estimate = estimated_size(Target::Opus, None, four_minutes, 40_000_000);
    assert!((3_500_000..4_200_000).contains(&estimate), "got {estimate}");
    // And a bitrate that was asked for is the one used.
    let higher = estimated_size(Target::Opus, Some(Quality::Bitrate(256)), four_minutes, 0);
    assert!(higher > estimate * 3 / 2, "{higher} vs {estimate}");
}
