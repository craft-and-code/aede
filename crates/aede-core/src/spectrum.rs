//! Shared native spectrogram measurements and PNG rendering, with atomic publication.
//! Native PCM and the admitted wideband DSD profile need no external program.
//! A named compatibility decoder remains optional for unsupported codecs only;
//! it never computes the FFT or renders a picture. Source files remain read-only.

use crate::tags::AudioProperties;
use flaccompagnon_core::spectrogram::{
    self as engine, AnalysisOptions, ImageLabels, RenderOptions,
};
use std::path::{Path, PathBuf};
mod cache;
mod legacy;
mod measure;
mod native;

/// Folder written beside the audio, holding one picture per track.
pub const FOLDER: &str = "spectrograms";

/// Plotted graph dimensions; shared renderer margins surround this area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// A `1800x940` plotted graph, matching FlacCompagnon.
    Full,
    /// A `900x470` plotted graph, used unless `--size full` is given.
    Half,
}

impl Size {
    /// Reads what a reader typed after `--size`.
    pub fn parse(text: &str) -> Option<Size> {
        match text.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Size::Full),
            "half" => Some(Size::Half),
            _ => None,
        }
    }

    /// Plot dimensions, excluding the surrounding frame.
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Size::Full => (1800, 940),
            Size::Half => (900, 470),
        }
    }

    /// How this size reads in a message.
    pub fn as_str(self) -> &'static str {
        match self {
            Size::Full => "full",
            Size::Half => "half",
        }
    }
}

/// Where the picture of a file belongs: `<its folder>/spectrograms/<name>.png`.
pub fn picture_for(audio: &Path) -> PathBuf {
    let folder = audio.parent().unwrap_or_else(|| Path::new("."));
    let stem = audio
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("track");
    folder.join(FOLDER).join(format!("{stem}.png"))
}

/// Whether the image lacks the current renderer/source stamp. Historical images
/// remain viewable but regenerate once; source nanoseconds and size are retained.
/// Changing size alone keeps the existing CLI contract: request `--full` to redraw.
pub fn out_of_date(audio: &Path, picture: &Path) -> bool {
    if !audio.is_file() {
        return !picture.is_file();
    }
    !cache::current(audio, picture)
}

/// Legacy ASCII caption formatter retained for Rust API compatibility.
/// New pictures use the shared renderer's actual decoded properties and UTF-8
/// labels. These old hints never define a DSD image's frequency axis.
pub fn caption(properties: &AudioProperties) -> String {
    let depth = match (properties.bit_depth, properties.bitrate_kbps) {
        (Some(bits), _) => format!("{bits}-bit"),
        // A lossy codec has no bit depth at all — printing "float" there, as a
        // tool that only ever sees lossless files can afford to, would state
        // something untrue about every MP3 in the library.
        (None, Some(kbps)) => format!("{kbps} kbps"),
        (None, None) => "unknown depth".to_string(),
    };
    let rate = properties.sample_rate.unwrap_or(0);
    let raw = format!(
        "{rate} Hz | {depth} | {} ch | {} | Nyquist {} Hz",
        properties.channels.unwrap_or(0),
        properties.codec.to_uppercase(),
        rate / 2,
    );
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '|' | '-' | '/' | '.'))
        .collect()
}

/// Compatibility entry point. The decoder path is only consulted for unsupported
/// codecs. Labels now come from decoded properties and optional file metadata;
/// the old preformatted caption argument is retained for source compatibility.
pub fn render(
    ffmpeg: &str,
    audio: &Path,
    picture: &Path,
    size: Size,
    _caption: Option<&str>,
) -> Result<(), String> {
    render_cancellable(audio, picture, size, &|| false, &|| Ok(ffmpeg.to_string()))
}

/// Generate with cooperative cancellation and a lazy optional legacy decoder.
/// The resolver is never called for ordinary native PCM or admitted DSF/DFF.
/// An in-progress OS read or PNG encoding cannot be forcibly interrupted; a
/// cancelled job is checked before publication and preserves the previous image.
pub fn render_cancellable(
    audio: &Path,
    picture: &Path,
    size: Size,
    cancelled: &dyn Fn() -> bool,
    external: &dyn Fn() -> Result<String, String>,
) -> Result<(), String> {
    admit(cancelled)?;
    crate::copy::validate_source(audio)?;
    let identity = cache::Input::read(audio)?;
    if picture
        .canonicalize()
        .is_ok_and(|p| audio.canonicalize().is_ok_and(|source| source == p))
    {
        return Err("spectrogram output must differ from its source".into());
    }
    publish_with(picture, cancelled, |output| {
        let (width, height) = size.dimensions();
        let options = AnalysisOptions {
            width,
            height,
            ..Default::default()
        };
        let tags = crate::tags::read(audio).map_err(|e| e.to_string())?;
        identity.unchanged(audio)?;
        let codec = tags.properties.codec.as_str();
        let legacy_required = matches!(codec, "ape" | "wavpack" | "musepack" | "speex");
        // Keep compatibility for configurations the native reader cannot open.
        // Once ALAC is admitted, a later data failure must not publish a
        // recovered prefix from another decoder as a completed measurement.
        let admitted_alac = if codec == "alac" {
            admit(cancelled)?;
            let admitted = aede_audio::decode::PcmStreamDecoder::open(audio).is_ok();
            admit(cancelled)?;
            identity.unchanged(audio)?;
            admitted
        } else {
            false
        };
        let (data, info, decoder) = if codec == "flac" && tags.properties.bit_depth == Some(32) {
            match native::analyze(audio, &options, cancelled) {
                Ok((data, info)) => (data, info, "native-flac32"),
                Err(error) => {
                    admit(cancelled)?;
                    identity.unchanged(audio)?;
                    let program = external().map_err(|missing| {
                        format!("32-bit FLAC native decoder: {error}; {missing}")
                    })?;
                    let (data, info) = legacy::analyze(audio, &options, &program, cancelled)?;
                    (data, info, "ffmpeg")
                }
            }
        } else if legacy_required {
            admit(cancelled)?;
            let program = external()?;
            let (data, info) = legacy::analyze(audio, &options, &program, cancelled)?;
            (data, info, "ffmpeg")
        } else {
            match engine::analyze_file(audio, &options, None, cancelled) {
                Ok((data, info)) => (data, info, "native"),
                Err(error)
                    if !admitted_alac
                        && !matches!(
                            codec,
                            "dsd"
                                | "pcm"
                                | "pcm_float"
                                | "flac"
                                | "vorbis"
                                | "mp3"
                                | "mp2"
                                | "mp1"
                                | "fl32"
                                | "fl64"
                        ) =>
                {
                    admit(cancelled)?;
                    identity.unchanged(audio)?;
                    let program = external()?;
                    let (data, info) = legacy::analyze(audio, &options, &program, cancelled)
                        .map_err(|fallback| {
                            format!("native decoder: {error}; external decoder: {fallback}")
                        })?;
                    (data, info, "ffmpeg")
                }
                Err(error) => return Err(error.to_string()),
            }
        };
        admit(cancelled)?;
        identity.unchanged(audio)?;
        let labels = ImageLabels {
            generator: "Aède".into(),
            title: tags.first("title").map(str::to_owned).unwrap_or_else(|| {
                audio
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }),
            artist: tags.first("artist").unwrap_or("").to_string(),
            audio_description: engine::audio_description(&info),
        };
        let bytes = engine::render_png(&data, &labels, &RenderOptions::default())
            .map_err(|e| e.to_string())?;
        admit(cancelled)?;
        identity.unchanged(audio)?;
        let bytes = cache::stamp(bytes, identity.clone(), decoder)?;
        std::fs::write(output, bytes).map_err(|e| e.to_string())?;
        admit(cancelled)?;
        identity.unchanged(audio)
    })
}
fn admit(cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    if cancelled() {
        return Err("spectrogram generation cancelled".into());
    }
    Ok(())
}
fn publish_with(
    picture: &Path,
    cancelled: &dyn Fn() -> bool,
    render: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    admit(cancelled)?;
    let output = crate::copy::TemporaryOutput::new(picture)?;
    render(output.path())?;
    admit(cancelled)?;
    output.publish()
}

#[cfg(test)]
#[path = "spectrum_tests.rs"]
mod tests;
