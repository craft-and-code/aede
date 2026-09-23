use super::*;

#[test]
fn canonical_keys() {
    assert_eq!(canonical_key("TRACKNUMBER"), "tracknumber");
    assert_eq!(canonical_key("Album Artist"), "albumartist");
    assert_eq!(canonical_key("PUBLISHER"), "label");
    assert_eq!(canonical_key("YEAR"), "date");
    assert_eq!(canonical_key("MusicBrainz Work Id"), "musicbrainz_workid");
    assert_eq!(canonical_key("MusicBrainz Label Id"), "musicbrainz_labelid");
    assert_eq!(canonical_key("Mastering Engineer"), "mastering_engineer");
    assert_eq!(canonical_key("DJ Mixer"), "djmixer");
}

#[test]
fn insertion_deduplicates_and_cleans() {
    let mut tags = RawTags::default();
    tags.insert("ARTIST", "  Miles Davis  ");
    tags.insert("artist", "Miles Davis");
    tags.insert("artist", "John Coltrane");
    tags.insert("artist", "   ");
    assert_eq!(tags.all("artist"), ["Miles Davis", "John Coltrane"]);
}

#[test]
fn quality_label() {
    let hires = AudioProperties {
        codec: "flac".into(),
        lossless: true,
        bit_depth: Some(24),
        sample_rate: Some(96_000),
        ..Default::default()
    };
    assert_eq!(hires.quality_label(), "FLAC 24/96");
    assert!(hires.is_hi_res());

    let cd = AudioProperties {
        codec: "flac".into(),
        lossless: true,
        bit_depth: Some(16),
        sample_rate: Some(44_100),
        ..Default::default()
    };
    assert_eq!(cd.quality_label(), "FLAC 16/44.1");
    assert!(!cd.is_hi_res());

    let lossy = AudioProperties {
        codec: "mp3".into(),
        bitrate_kbps: Some(320),
        ..Default::default()
    };
    assert_eq!(lossy.quality_label(), "MP3 320");
    assert!(!lossy.is_hi_res());
}

#[test]
fn extension_recognition() {
    assert!(is_audio_path(Path::new("/music/a.FLAC")));
    assert!(is_audio_path(Path::new("/music/a.mp3")));
    assert!(!is_audio_path(Path::new("/music/cover.jpg")));
    assert!(!is_audio_path(Path::new("/music/folder")));
}
