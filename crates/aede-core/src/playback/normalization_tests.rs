use super::{Error, Mode, Selection, Source, select};
use crate::model::AudioFile;
use crate::tags::{AudioProperties, RawTags};

fn file(codec: &str, entries: &[(&str, &str)]) -> AudioFile {
    let mut tags = RawTags::default();
    for &(key, value) in entries {
        tags.insert(key, value);
    }
    AudioFile {
        properties: AudioProperties {
            codec: codec.into(),
            ..Default::default()
        },
        tags: tags.fields,
        ..Default::default()
    }
}

#[test]
fn replaygain_selects_the_requested_scope_and_its_peak() {
    let audio = file(
        "flac",
        &[
            ("REPLAYGAIN_TRACK_GAIN", "-7.50 dB"),
            ("REPLAYGAIN_TRACK_PEAK", "0.98"),
            ("REPLAYGAIN_ALBUM_GAIN", "-5.00 dB"),
            ("REPLAYGAIN_ALBUM_PEAK", "1.02"),
        ],
    );
    assert_eq!(
        select(&audio, Mode::Track, -18.0),
        Ok(Some(Selection {
            gain_db: -7.5,
            source: Source::ReplayGainTrack,
            source_peak: Some(0.98),
        }))
    );
    assert_eq!(
        select(&audio, Mode::Album, -23.0),
        Ok(Some(Selection {
            gain_db: -10.0,
            source: Source::ReplayGainAlbum,
            source_peak: Some(1.02),
        }))
    );
}

#[test]
fn missing_requested_scope_uses_the_other_scope() {
    let audio = file("mp3", &[("replaygain_album_gain", "+1.5 dB")]);
    assert_eq!(
        select(&audio, Mode::Track, -18.0),
        Ok(Some(Selection {
            gain_db: 1.5,
            source: Source::ReplayGainAlbum,
            source_peak: None,
        }))
    );
}

#[test]
fn opus_r128_uses_q7_8_and_its_own_reference_level() {
    let audio = file(
        "opus",
        &[
            ("R128_TRACK_GAIN", "-573"),
            ("R128_ALBUM_GAIN", "111"),
            ("REPLAYGAIN_TRACK_GAIN", "+12 dB"),
        ],
    );
    assert_eq!(
        select(&audio, Mode::Track, -18.0),
        Ok(Some(Selection {
            gain_db: 5.0 - 573.0 / 256.0,
            source: Source::OpusR128Track,
            source_peak: None,
        }))
    );
    assert_eq!(
        select(&audio, Mode::Album, -23.0),
        Ok(Some(Selection {
            gain_db: 111.0 / 256.0,
            source: Source::OpusR128Album,
            source_peak: None,
        }))
    );
}

#[test]
fn r128_tags_on_another_codec_are_not_applied() {
    let audio = file("flac", &[("r128_track_gain", "256")]);
    assert_eq!(select(&audio, Mode::Track, -18.0), Ok(None));
}

#[test]
fn malformed_or_repeated_selected_tags_are_errors() {
    let malformed = file(
        "opus",
        &[
            ("r128_track_gain", "0.5"),
            ("replaygain_track_gain", "-3 dB"),
        ],
    );
    assert_eq!(
        select(&malformed, Mode::Track, -18.0),
        Err(Error::InvalidTag("r128_track_gain"))
    );
    let repeated = file(
        "flac",
        &[
            ("replaygain_track_gain", "-3 dB"),
            ("replaygain_track_gain", "-4 dB"),
        ],
    );
    assert_eq!(
        select(&repeated, Mode::Track, -18.0),
        Err(Error::RepeatedTag("replaygain_track_gain"))
    );
    let bad_peak = file(
        "flac",
        &[
            ("replaygain_track_gain", "-3 dB"),
            ("replaygain_track_peak", "NaN"),
        ],
    );
    assert_eq!(
        select(&bad_peak, Mode::Track, -18.0),
        Err(Error::InvalidTag("replaygain_track_peak"))
    );
}

#[test]
fn off_mode_and_absent_tags_need_no_gain() {
    let malformed = file("flac", &[("replaygain_track_gain", "wrong")]);
    assert_eq!(select(&malformed, Mode::Off, f32::NAN), Ok(None));
    assert_eq!(select(&file("flac", &[]), Mode::Track, -18.0), Ok(None));
    assert_eq!(
        select(&file("flac", &[]), Mode::Track, f32::NAN),
        Err(Error::InvalidTarget)
    );
}

#[test]
fn opus_comment_gain_survives_the_existing_tag_reader() {
    let entry = b"R128_TRACK_GAIN=-573";
    let mut comment = Vec::new();
    comment.extend_from_slice(&0u32.to_le_bytes()); // empty vendor
    comment.extend_from_slice(&1u32.to_le_bytes());
    comment.extend_from_slice(&(entry.len() as u32).to_le_bytes());
    comment.extend_from_slice(entry);

    let mut tags = RawTags::default();
    crate::tags::flac::parse_vorbis_comment(&comment, &mut tags);
    assert_eq!(tags.first("r128_track_gain"), Some("-573"));
}
