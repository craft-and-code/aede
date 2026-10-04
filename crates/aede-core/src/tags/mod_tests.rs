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
    assert_eq!(canonical_key("MOVEMENT NAME"), "movement");
    assert_eq!(canonical_key("Movement No"), "movementnumber");
    assert_eq!(canonical_key("Total Movements"), "movementtotal");
    assert_eq!(canonical_key("SOLOISTS"), "soloist");
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

#[test]
fn opened_metadata_reads_native_and_foreign_sources_after_path_replacement() {
    use std::io::{Seek, SeekFrom};

    let directory = crate::store_lock::test_support::Directory::new("opened_tags");
    for name in ["track.flac", "track.wv", "track.aac"] {
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let expected = read(&original).unwrap();
        let path = directory.path().join(name);
        std::fs::copy(original, &path).unwrap();
        let mut file = std::fs::File::open(&path).unwrap();
        file.seek(SeekFrom::Start(17)).unwrap();
        std::fs::rename(&path, path.with_extension("previous")).unwrap();
        std::fs::write(&path, b"replacement is not the opened audio").unwrap();
        let found = read_from_file(&mut file).unwrap();
        assert_eq!(
            found.fields, expected.fields,
            "{name} metadata stays on the opened source"
        );
        assert_eq!(
            found.properties, expected.properties,
            "{name} format stays on the opened source"
        );
        assert_eq!(found.has_embedded_art, expected.has_embedded_art);
    }
}
