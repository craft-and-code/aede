//! Integration tests on real audio files.
//!
//! The files in `tests/fixtures/` were produced with ffmpeg and verified with
//! ffprobe: they are real containers, not mock-ups. That is the only way to
//! catch misreadings of the specifications.
//!
//! `compilation.flac` is the one exception: it is `track.flac` with its
//! Vorbis comment block rewritten to carry `ALBUMARTIST=Various Artists`, the
//! padding block shrunk by exactly as much so the audio is untouched. A
//! library with no compilation in it cannot test the one thing that
//! distinguishes a compilation from an album.

use std::path::PathBuf;

use aede_core::tags::{self, RawTags};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read_fixture(name: &str) -> RawTags {
    tags::read(&fixture(name)).unwrap_or_else(|e| panic!("reading {name}: {e}"))
}

/// The common fields written into every reference track.
fn check_common_tags(t: &RawTags, name: &str) {
    assert_eq!(t.first("artist"), Some("Miles Davis"), "artist of {name}");
    assert_eq!(t.first("album"), Some("Kind of Blue"), "album of {name}");
    assert_eq!(
        t.first("albumartist"),
        Some("Miles Davis Sextet"),
        "album artist of {name}"
    );
    assert_eq!(t.first("date"), Some("1959"), "date of {name}");
    assert_eq!(t.first("genre"), Some("Jazz"), "genre of {name}");
    assert!(
        t.first("tracknumber").unwrap_or("").starts_with('1'),
        "track number of {name}"
    );
}

#[test]
fn flac_cd_quality() {
    let t = read_fixture("track.flac");
    check_common_tags(&t, "track.flac");
    assert_eq!(t.first("title"), Some("So What"));
    assert_eq!(t.first("label"), Some("Columbia"));
    assert_eq!(t.properties.codec, "flac");
    assert_eq!(t.properties.container, "flac");
    assert!(t.properties.lossless);
    assert_eq!(t.properties.sample_rate, Some(44_100));
    assert_eq!(t.properties.bit_depth, Some(16));
    assert_eq!(t.properties.channels, Some(1));
    assert_eq!(t.properties.duration_ms, Some(1000));
    assert!(!t.properties.is_hi_res());
}

#[test]
fn flac_high_resolution() {
    let t = read_fixture("hires.flac");
    assert_eq!(t.first("title"), Some("Blue in Green"));
    assert_eq!(t.properties.sample_rate, Some(96_000));
    assert_eq!(t.properties.bit_depth, Some(24));
    assert!(t.properties.is_hi_res());
    assert_eq!(t.properties.quality_label(), "FLAC 24/96");
}

#[test]
fn mp3_constant_bitrate() {
    let t = read_fixture("track.mp3");
    check_common_tags(&t, "track.mp3");
    assert_eq!(t.properties.codec, "mp3");
    assert!(!t.properties.lossless);
    assert_eq!(t.properties.sample_rate, Some(44_100));
    // ffprobe reports 1.0449 s: a 20 ms discrepancy is tolerated.
    let ms = t.properties.duration_ms.expect("duration");
    assert!((1000..=1100).contains(&ms), "duration obtained: {ms} ms");
    let bitrate = t.properties.bitrate_kbps.expect("bitrate");
    assert!(
        (185..=205).contains(&bitrate),
        "bitrate obtained: {bitrate} kbit/s"
    );
    // The LAME tag carries the encoder delay, which gapless playback needs.
    assert!(t.first("encoder_delay").is_some(), "encoder delay missing");
}

#[test]
fn mp3_variable_bitrate() {
    let t = read_fixture("vbr.mp3");
    check_common_tags(&t, "vbr.mp3");
    let ms = t.properties.duration_ms.expect("duration");
    assert!((1000..=1100).contains(&ms), "duration obtained: {ms} ms");
    // VBR must be detected: the real bitrate is far below the nominal bitrate
    // of the first frame.
    let bitrate = t.properties.bitrate_kbps.expect("bitrate");
    assert!(bitrate < 100, "VBR bitrate obtained: {bitrate} kbit/s");
}

#[test]
fn mp4_alac_lossless() {
    let t = read_fixture("track.m4a");
    check_common_tags(&t, "track.m4a");
    assert_eq!(t.properties.codec, "alac");
    assert_eq!(t.properties.container, "mp4");
    assert!(t.properties.lossless);
    assert_eq!(t.properties.sample_rate, Some(44_100));
    assert_eq!(t.properties.bit_depth, Some(24));
    assert_eq!(t.properties.duration_ms, Some(1000));
    // The trkn/disk pairs split into a number and a total.
    assert_eq!(t.first("tracknumber"), Some("1"));
    assert_eq!(t.first("tracktotal"), Some("5"));
}

#[test]
fn mp4_aac_lossy() {
    let t = read_fixture("aac.m4a");
    check_common_tags(&t, "aac.m4a");
    assert_eq!(t.properties.codec, "aac");
    assert!(!t.properties.lossless);
    assert_eq!(t.properties.duration_ms, Some(1000));
}

#[test]
fn ogg_vorbis() {
    let t = read_fixture("track.ogg");
    check_common_tags(&t, "track.ogg");
    assert_eq!(t.properties.codec, "vorbis");
    assert_eq!(t.properties.container, "ogg");
    assert_eq!(t.properties.sample_rate, Some(44_100));
    assert_eq!(t.properties.duration_ms, Some(1000));
}

#[test]
fn ogg_opus() {
    let t = read_fixture("track.opus");
    check_common_tags(&t, "track.opus");
    assert_eq!(t.properties.codec, "opus");
    // Opus always decodes at 48 kHz, whatever the source rate was.
    assert_eq!(t.properties.sample_rate, Some(48_000));
    let ms = t.properties.duration_ms.expect("duration");
    assert!((980..=1020).contains(&ms), "duration obtained: {ms} ms");
    // Internal fields must not leak into the metadata.
    assert!(t.fields.keys().all(|k| !k.starts_with("__")));
}

#[test]
fn wav_pcm() {
    let t = read_fixture("track.wav");
    assert_eq!(t.first("title"), Some("So What"));
    assert_eq!(t.first("artist"), Some("Miles Davis"));
    assert_eq!(t.properties.codec, "pcm");
    assert!(t.properties.lossless);
    assert_eq!(t.properties.bit_depth, Some(16));
    assert_eq!(t.properties.duration_ms, Some(250));
}

#[test]
fn aiff_with_id3_chunk() {
    let t = read_fixture("track.aiff");
    assert_eq!(t.first("title"), Some("So What"));
    assert_eq!(t.first("album"), Some("Kind of Blue"));
    assert_eq!(t.properties.container, "aiff");
    assert_eq!(t.properties.sample_rate, Some(44_100));
    assert_eq!(t.properties.bit_depth, Some(16));
    assert_eq!(t.properties.duration_ms, Some(250));
}

#[test]
fn non_audio_file_is_rejected() {
    let path = fixture("track.flac");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] = b'X'; // break the signature
    let temp = std::env::temp_dir().join("aede_broken_signature.flac");
    std::fs::write(&temp, &bytes).unwrap();
    assert!(tags::read(&temp).is_err(), "an invalid signature must fail");
    let _ = std::fs::remove_file(temp);
}

#[test]
fn truncated_file_does_not_panic() {
    let path = fixture("track.m4a");
    let bytes = std::fs::read(&path).unwrap();
    for fraction in [4, 3, 2] {
        let temp = std::env::temp_dir().join(format!("aede_truncated_{fraction}.m4a"));
        std::fs::write(&temp, &bytes[..bytes.len() / fraction]).unwrap();
        // The result does not matter: only the absence of a panic does.
        let _ = tags::read(&temp);
        let _ = std::fs::remove_file(temp);
    }
}

// The formats below have no parser of their own: they are read through the
// fallback. What matters is that the file lands on the right reader and comes
// back in the same vocabulary as the rest.

#[test]
fn wavpack_through_the_fallback() {
    let t = read_fixture("track.wv");
    assert_eq!(t.first("title"), Some("Blue in Green"));
    assert_eq!(t.first("artist"), Some("Miles Davis"));
    assert_eq!(t.first("album"), Some("Kind of Blue"));
    assert_eq!(t.first("date"), Some("1959"));
    assert_eq!(t.first("tracknumber"), Some("3"));
    assert_eq!(t.properties.codec, "wavpack");
    assert!(t.properties.lossless, "WavPack is a lossless codec");
    assert_eq!(t.properties.sample_rate, Some(44_100));
    assert_eq!(t.properties.bit_depth, Some(16));
    assert_eq!(t.properties.channels, Some(2));
    assert_eq!(t.properties.duration_ms, Some(1000));
}

#[test]
fn raw_aac_is_not_mistaken_for_mpeg() {
    // An ADTS frame and an MPEG frame share their sync word; only the layer
    // field tells them apart. Read as MPEG, this file used to announce 384
    // kbps and a duration of zero.
    let t = read_fixture("track.aac");
    assert_eq!(t.properties.container, "adts", "container of track.aac");
    assert_eq!(t.properties.codec, "aac");
    assert!(!t.properties.lossless);
    assert_eq!(t.properties.sample_rate, Some(44_100));
    // Slightly above a second: the AAC encoder pads the last frame.
    let duration = t.properties.duration_ms.unwrap_or(0);
    assert!((1000..1200).contains(&duration), "duration: {duration} ms");
    assert_eq!(t.properties.bit_depth, None, "a lossy codec has no depth");
    // The ID3v2 tag in front of the stream is read like any other.
    assert_eq!(t.first("title"), Some("Blue in Green"));
    assert_eq!(t.first("genre"), Some("Jazz"));
}

#[test]
fn speex_falls_through_the_ogg_parser() {
    // The native Ogg parser only knows Vorbis, Opus and FLAC. Anything else
    // must be handed over rather than returned empty.
    let t = read_fixture("track.spx");
    assert_eq!(t.properties.codec, "speex");
    assert_eq!(t.properties.container, "ogg");
    assert!(t.properties.sample_rate.is_some(), "properties are read");
}

#[test]
fn a_truncated_foreign_file_does_not_panic() {
    let bytes = std::fs::read(fixture("track.wv")).unwrap();
    for fraction in [4, 3, 2] {
        let temp = std::env::temp_dir().join(format!("aede_truncated_{fraction}.wv"));
        std::fs::write(&temp, &bytes[..bytes.len() / fraction]).unwrap();
        let _ = tags::read(&temp);
        let _ = std::fs::remove_file(temp);
    }
}
