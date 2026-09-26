//! Selection of a playback gain from local audio metadata.
//!
//! This module chooses a gain; `aede-dsp` applies it to decoded samples.
//! The decoder must have applied the mandatory Opus ID
//! header output gain before a selected R128 tag gain is applied.

use std::fmt;

use crate::model::AudioFile;

const REPLAYGAIN_REFERENCE_LUFS: f32 = -18.0;
const OPUS_R128_REFERENCE_LUFS: f32 = -23.0;

/// Which loudness metadata to use for playback.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    /// Leave the decoded signal's level alone.
    Off,
    /// Prefer the gain measured for an individual track.
    #[default]
    Track,
    /// Prefer the gain measured for a whole album.
    Album,
}

/// Origin and scope of the selected gain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// Gain measured for one track, from a ReplayGain tag.
    ReplayGainTrack,
    /// Gain measured for an album, from a ReplayGain tag.
    ReplayGainAlbum,
    /// Gain measured for one Opus track, from an R128 tag.
    OpusR128Track,
    /// Gain measured for an Opus album, from an R128 tag.
    OpusR128Album,
}

/// The gain sent to the DSP and any corresponding peak measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    /// Gain to apply after decoding, in dB, adjusted to `target_lufs`.
    pub gain_db: f32,
    /// Which tag supplied the gain.
    pub source: Source,
    /// Linear peak supplied by ReplayGain, before applying `gain_db`.
    /// Opus R128 has no corresponding peak tag.
    pub source_peak: Option<f32>,
}

/// Invalid metadata is reported rather than replaced by another tag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The requested loudness target is not finite or is outside the usable range.
    InvalidTarget,
    /// Several distinct values claim the same gain or peak.
    RepeatedTag(&'static str),
    /// A selected gain or peak is malformed.
    InvalidTag(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget => f.write_str("loudness target must be between -100 and 0 LUFS"),
            Self::RepeatedTag(key) => write!(f, "audio file has conflicting {key} values"),
            Self::InvalidTag(key) => write!(f, "audio file has an invalid {key} value"),
        }
    }
}

impl std::error::Error for Error {}

/// Choose a gain for a file without altering its tags or audio.
///
/// `target_lufs` is explicit so a future player can choose its listening
/// reference. ReplayGain uses a nominal -18 LUFS reference; Opus R128 uses
/// -23 LUFS. On Opus, R128 wins over ReplayGain when both are present. Within
/// a scheme, the other scope is used only when the requested scope is absent.
/// A malformed selected tag is an error, not a reason to fall back silently.
///
/// This returns `None` when normalization is off or no supported tag exists.
/// A returned gain can produce samples above full scale; the output adapter
/// must handle them when it converts floating-point PCM for the audio device.
pub fn select(file: &AudioFile, mode: Mode, target_lufs: f32) -> Result<Option<Selection>, Error> {
    if mode == Mode::Off {
        return Ok(None);
    }
    if !target_lufs.is_finite() || !(-100.0..=0.0).contains(&target_lufs) {
        return Err(Error::InvalidTarget);
    }

    let scopes = match mode {
        Mode::Track => [true, false],
        Mode::Album => [false, true],
        Mode::Off => return Ok(None),
    };
    if file.properties.codec == "opus" {
        for track in scopes {
            let key = if track {
                "r128_track_gain"
            } else {
                "r128_album_gain"
            };
            if let Some(raw) = one_value(file, key)? {
                let gain_db = parse_r128(raw).ok_or(Error::InvalidTag(key))?;
                return Ok(Some(Selection {
                    gain_db: gain_db + target_lufs - OPUS_R128_REFERENCE_LUFS,
                    source: if track {
                        Source::OpusR128Track
                    } else {
                        Source::OpusR128Album
                    },
                    source_peak: None,
                }));
            }
        }
    }

    for track in scopes {
        let key = if track {
            "replaygain_track_gain"
        } else {
            "replaygain_album_gain"
        };
        if let Some(raw) = one_value(file, key)? {
            let gain_db = parse_db(raw).ok_or(Error::InvalidTag(key))?;
            let peak_key = if track {
                "replaygain_track_peak"
            } else {
                "replaygain_album_peak"
            };
            let source_peak = one_value(file, peak_key)?
                .map(|peak| parse_peak(peak).ok_or(Error::InvalidTag(peak_key)))
                .transpose()?;
            return Ok(Some(Selection {
                gain_db: gain_db + target_lufs - REPLAYGAIN_REFERENCE_LUFS,
                source: if track {
                    Source::ReplayGainTrack
                } else {
                    Source::ReplayGainAlbum
                },
                source_peak,
            }));
        }
    }
    Ok(None)
}

fn one_value<'a>(file: &'a AudioFile, key: &'static str) -> Result<Option<&'a str>, Error> {
    let Some(values) = file.tags.get(key) else {
        return Ok(None);
    };
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value)),
        _ => Err(Error::RepeatedTag(key)),
    }
}

fn parse_db(raw: &str) -> Option<f32> {
    let value = raw.trim();
    let number = if value.to_ascii_lowercase().ends_with("db") {
        value.get(..value.len() - 2)?.trim_end()
    } else {
        value
    };
    let gain = number.parse::<f32>().ok()?;
    (gain.is_finite() && (-128.0..=128.0).contains(&gain)).then_some(gain)
}

fn parse_r128(raw: &str) -> Option<f32> {
    if raw.is_empty() || raw.len() > 6 {
        return None;
    }
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fixed = raw.parse::<i16>().ok()?;
    Some(f32::from(fixed) / 256.0)
}

fn parse_peak(raw: &str) -> Option<f32> {
    let peak = raw.parse::<f32>().ok()?;
    (peak.is_finite() && peak >= 0.0).then_some(peak)
}

#[cfg(test)]
#[path = "normalization_tests.rs"]
mod tests;
