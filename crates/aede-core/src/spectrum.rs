//! Shared native spectrogram measurements and PNG rendering, with atomic publication.
//! Native PCM and the admitted wideband DSD profile need no external program.
//! A named compatibility decoder remains optional for unsupported codecs only;
//! it never computes the FFT or renders a picture. Source files remain read-only.

use crate::tags::AudioProperties;
pub use engine::requires_dsd_decoder;
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
    /// Standard 1200×627 plot; `half` retains its legacy API spelling.
    Half,
}

impl Size {
    /// Parse `standard` or `full`; `half` remains a compatibility alias.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "standard" | "half" => Some(Self::Half),
            _ => None,
        }
    }
    /// Plot dimensions, excluding the surrounding frame.
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Full => engine::PlotSize::Large,
            Self::Half => engine::PlotSize::Standard,
        }
        .dimensions()
    }
    /// Human-readable preferred size, independent of legacy API names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Half => "standard",
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

/// Generate an image with an optional compatibility decoder and catalogue title.
/// Actual decoded properties define the labels and spectral axes.
pub fn render<'a>(
    ffmpeg: impl Into<Option<&'a str>>,
    audio: &Path,
    picture: &Path,
    size: Size,
    title: Option<&str>,
) -> Result<(), String> {
    let ffmpeg = ffmpeg.into();
    render_cancellable_with_title(audio, picture, size, title, &|| false, &|| {
        ffmpeg
            .map(str::to_owned)
            .ok_or_else(|| crate::ffmpeg::missing("decoding this spectrogram format"))
    })
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
    render_cancellable_with_title(audio, picture, size, None, cancelled, external)
}

/// Generate with the same source/decoder safeguards and an optional catalogue title.
pub fn render_cancellable_with_title(
    audio: &Path,
    picture: &Path,
    size: Size,
    title: Option<&str>,
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
        // Keep compatibility for configurations the native reader cannot open.
        // Once a native codec is admitted, a later data failure must not publish a
        // recovered prefix from another decoder as a completed measurement.
        let admitted_native = if matches!(
            codec,
            "alac" | "aac" | "ape" | "wavpack" | "musepack" | "speex"
        ) {
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
        } else {
            match engine::analyze_file(audio, &options, None, cancelled) {
                Ok((data, info)) => (data, info, "native"),
                Err(error)
                    if !admitted_native
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
        let mut labels = image_labels(audio, title.or_else(|| tags.first("title")), &info);
        labels.artist = tags.first("artist").unwrap_or("").to_string();
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
fn image_labels(
    audio: &Path,
    title: Option<&str>,
    info: &flaccompagnon_core::BasicInfo,
) -> ImageLabels {
    ImageLabels {
        generator: "Aède".into(),
        title: title
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                audio
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            }),
        artist: String::new(),
        audio_description: engine::audio_description(info),
    }
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
    if !picture
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err("Spectrogram destination must use a .png extension".into());
    }
    let output = crate::copy::TemporaryOutput::new(picture)?;
    render(output.path())?;
    admit(cancelled)?;
    output.publish()
}

#[cfg(test)]
#[path = "spectrum_tests.rs"]
mod tests;
