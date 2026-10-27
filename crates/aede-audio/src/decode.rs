//! Progressive local Symphonia decoding, extracted from FlacCompagnon's preview path.
use std::{
    fmt,
    fs::{self, File},
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, DecoderOptions},
    errors::Error as SymError,
    formats::{FormatOptions, FormatReader},
    io::MediaSourceStream,
    meta::MetadataOptions,
    probe::Hint,
};

#[path = "decode_source.rs"]
mod source;
pub use source::{FlacAudioSource, FlacProbe};
#[path = "decode_alac.rs"]
mod alac;
pub use alac::{AlacConfiguration, declared_pcm_frames, read_alac_configuration};
#[path = "decode_guard.rs"]
mod guard;
#[path = "decode_opus.rs"]
mod opus;

/// Resolve ALAC rate/layout/precision when the demuxer omits its channel layout.
/// MP4 hints may omit channels or substitute a lower rate. Other codec
/// parameters, source bytes, frame counts and timeline hints remain unchanged.
pub fn codec_parameters(
    parameters: &symphonia::core::codecs::CodecParameters,
) -> Result<symphonia::core::codecs::CodecParameters, SymError> {
    alac::parameters(parameters)
}

/// Construct a shared native codec, including the admitted mono/stereo Opus profile.
/// Containers retain packet ownership; this applies no application DSP or volume.
/// Opus callers use gapless demuxing and require `finalize().verify_ok == Some(true)`.
/// ALAC callers reject `Some(false)` at EOF: known media ticks must all be covered,
/// independently of PCM-clock rounding. This is completion, not a source checksum.
/// Random seeks require reopening and progressively decoding source pre-roll.
/// Unexpected native-codec unwinding retires the returned codec permanently;
/// packet calls then return `ResetRequired`, with no stale decoded output.
pub fn make_decoder(
    parameters: &symphonia::core::codecs::CodecParameters,
    options: &DecoderOptions,
) -> Result<Box<dyn Decoder>, SymError> {
    let inner: Box<dyn Decoder> = catch_unwind(AssertUnwindSafe(|| {
        if parameters.codec == symphonia::core::codecs::CODEC_TYPE_OPUS {
            Ok(Box::new(opus::OpusDecoder::try_new(parameters, options)?) as Box<dyn Decoder>)
        } else {
            symphonia::default::get_codecs().make(&codec_parameters(parameters)?, options)
        }
    }))
    .map_err(|_| SymError::DecodeError("audio codec construction panicked"))??;
    Ok(Box::new(guard::GuardedDecoder::new(inner)?))
}

/// A local decode failure, independent of any application's analysis errors.
#[derive(Debug)]
pub struct DecodeError(pub String);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DecodeError {}

/// Packet-at-a-time PCM reader. DSD uses the wideband measurement profile;
/// device callers should use [`Self::open_for_playback`] for ultrasonic filtering.
pub struct PcmStreamDecoder {
    source: StreamSource,
    /// Derived PCM frames per second (not the original DSD bit clock).
    pub sample_rate: u32,
    /// Interleaved channels per frame.
    pub channels: usize,
    /// Speaker positions when established by the source.
    pub channel_mask: Option<u32>,
}
enum StreamSource {
    Pcm(SymphoniaDecoder),
    Dsd(crate::dsd::Decoder),
}
impl PcmStreamDecoder {
    /// Open a regular source, preserving the wideband DSD measurement profile.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        Self::open_profile(path, crate::dsd::Profile::Measurement)
    }
    /// Open with the DSD listening low-pass. PCM sources remain unchanged.
    pub fn open_for_playback(path: &Path) -> Result<Self, DecodeError> {
        Self::open_profile(path, crate::dsd::Profile::Listening)
    }
    fn open_profile(path: &Path, profile: crate::dsd::Profile) -> Result<Self, DecodeError> {
        use std::io::{Read, Seek, SeekFrom};
        if !fs::metadata(path)
            .map_err(|e| DecodeError(e.to_string()))?
            .is_file()
        {
            return Err(DecodeError("audio source is not a regular file".into()));
        }
        let mut file = File::open(path).map_err(|e| DecodeError(e.to_string()))?;
        if !file
            .metadata()
            .map_err(|e| DecodeError(e.to_string()))?
            .is_file()
        {
            return Err(DecodeError("audio source is not a regular file".into()));
        }
        let mut magic = [0; 4];
        let has_magic = match file.read_exact(&mut magic) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => false,
            Err(error) => return Err(DecodeError(error.to_string())),
        };
        if has_magic && (&magic == b"DSD " || &magic == b"FRM8") {
            let dsd = crate::dsd::Decoder::from_file(file, profile)
                .map_err(|e| DecodeError(e.to_string()))?;
            let info = dsd.info();
            return Ok(Self {
                sample_rate: dsd.sample_rate(),
                channels: info.channels,
                channel_mask: Some(if info.channels == 1 { 4 } else { 3 }),
                source: StreamSource::Dsd(dsd),
            });
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|e| DecodeError(e.to_string()))?;
        let pcm = SymphoniaDecoder::from_file(path, file)?;
        Ok(Self {
            sample_rate: pcm.sample_rate,
            channels: pcm.channels,
            channel_mask: pcm.channel_mask,
            source: StreamSource::Pcm(pcm),
        })
    }
    /// Yield one finite complete PCM block.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, DecodeError> {
        self.next_chunk_admitted(|| Ok(()))
    }
    /// Recheck caller ownership during progressive reads, including DSD filtering.
    pub fn next_chunk_admitted(
        &mut self,
        mut current: impl FnMut() -> Result<(), DecodeError>,
    ) -> Result<Option<Vec<f32>>, DecodeError> {
        match &mut self.source {
            StreamSource::Pcm(pcm) => {
                let result = pcm.next_chunk_admitted(current);
                if pcm.strict_codec.is_some() && result.is_err() {
                    pcm.failed = true;
                }
                result
            }
            StreamSource::Dsd(dsd) => dsd
                .next_chunk(|| current().map_err(|e| crate::dsd::DsdError(e.0)))
                .map_err(|e| DecodeError(e.to_string())),
        }
    }
}

/// Packet-at-a-time PCM reader. No device, analyzer or complete-track allocation.
struct SymphoniaDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    scratch: Option<SampleBuffer<f32>>,
    capacity: u64,
    failed: bool,
    strict_codec: Option<&'static str>,
    expected_frames: Option<u64>,
    decoded_frames: u64,
    /// Source frames per second.
    pub sample_rate: u32,
    /// Interleaved samples per frame.
    pub channels: usize,
    /// Speaker mask from the same probed header, when available.
    pub channel_mask: Option<u32>,
}
impl SymphoniaDecoder {
    /// Open a local regular file and probe its default audio track with gapless trimming.
    fn from_file(path: &Path, file: File) -> Result<Self, DecodeError> {
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(extension);
        }
        let input = source::prepare(file)?;
        let probed = catch_unwind(AssertUnwindSafe(|| {
            source::probe().format(
                &hint,
                MediaSourceStream::new(input, Default::default()),
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
        }))
        .map_err(|_| DecodeError("audio probe panicked".into()))?
        .map_err(|e| DecodeError(format!("probe failed: {e}")))?;
        let format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| DecodeError("no default track".into()))?;
        let parameters = codec_parameters(&track.codec_params)
            .map_err(|e| DecodeError(format!("invalid codec configuration: {e}")))?;
        let sample_rate = parameters
            .sample_rate
            .filter(|v| *v > 0)
            .ok_or_else(|| DecodeError("missing sample rate".into()))?;
        let channels = parameters
            .channels
            .map(|v| v.count())
            .filter(|v| *v > 0 && *v <= 32)
            .ok_or_else(|| DecodeError("missing or unsupported channels".into()))?;
        let channel_mask = parameters.channels.map(|v| v.bits());
        let track_id = track.id;
        let strict_codec = match parameters.codec {
            symphonia::core::codecs::CODEC_TYPE_OPUS => Some("Opus"),
            symphonia::core::codecs::CODEC_TYPE_ALAC => Some("ALAC"),
            _ => None,
        };
        let expected_frames = if strict_codec == Some("ALAC") {
            declared_pcm_frames(&parameters).map_err(|e| DecodeError(e.to_string()))?
        } else {
            None
        };
        let decoder = catch_unwind(AssertUnwindSafe(|| {
            make_decoder(&parameters, &DecoderOptions::default())
        }))
        .map_err(|_| DecodeError("audio decoder construction panicked".into()))?
        .map_err(|e| DecodeError(format!("no decoder: {e}")))?;
        Ok(Self {
            format,
            decoder,
            track_id,
            scratch: None,
            capacity: 0,
            failed: false,
            strict_codec,
            expected_frames,
            decoded_frames: 0,
            sample_rate,
            channels,
            channel_mask,
        })
    }
    /// Retire packet reads (including skipped corrupt/non-audio packets) between I/O stages.
    /// An already running regular-file read is not forcibly interrupted.
    pub fn next_chunk_admitted(
        &mut self,
        mut current: impl FnMut() -> Result<(), DecodeError>,
    ) -> Result<Option<Vec<f32>>, DecodeError> {
        if self.failed {
            return Err(DecodeError(
                "audio decoder cannot continue after a failure".into(),
            ));
        }
        loop {
            current()?;
            // A panicked reader or codec may have mutated its internal state. Retire it
            // permanently rather than retrying or publishing a misleading EOF.
            let packet = match catch_unwind(AssertUnwindSafe(|| self.format.next_packet())) {
                Ok(result) => result,
                Err(_) => {
                    self.failed = true;
                    return Err(DecodeError("audio packet reader panicked".into()));
                }
            };
            let packet = match packet {
                Ok(packet) => packet,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    current()?;
                    if self.strict_codec == Some("Opus")
                        && self.decoder.finalize().verify_ok != Some(true)
                    {
                        self.failed = true;
                        return Err(DecodeError("incomplete Opus stream".into()));
                    }
                    if self.strict_codec == Some("ALAC")
                        && self.decoder.finalize().verify_ok == Some(false)
                    {
                        self.failed = true;
                        return Err(DecodeError("incomplete ALAC packet timeline".into()));
                    }
                    if self
                        .expected_frames
                        .is_some_and(|n| n != self.decoded_frames)
                    {
                        self.failed = true;
                        return Err(DecodeError("incomplete ALAC stream".into()));
                    }
                    return Ok(None);
                }
                Err(e) => return Err(DecodeError(format!("packet error: {e}"))),
            };
            current()?;
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoder = self.decoder.as_mut();
            let decoded = match catch_unwind(AssertUnwindSafe(move || {
                // Consume the borrowed codec so its decoded buffer can leave this closure.
                let active_decoder = decoder;
                active_decoder.decode(&packet)
            })) {
                Ok(result) => result,
                Err(_) => {
                    self.failed = true;
                    return Err(DecodeError("audio codec panicked".into()));
                }
            };
            match decoded {
                Ok(decoded) => {
                    current()?;
                    if decoded.spec().rate != self.sample_rate
                        || decoded.spec().channels.count() != self.channels
                    {
                        return Err(DecodeError("stream changed during decoding".into()));
                    }
                    let capacity = decoded.capacity() as u64;
                    if capacity > 1_048_576 / self.channels as u64 {
                        return Err(DecodeError("decoded packet exceeds PCM budget".into()));
                    }
                    if self.scratch.is_none() || capacity > self.capacity {
                        self.scratch = Some(SampleBuffer::new(capacity, *decoded.spec()));
                        self.capacity = capacity;
                    }
                    let scratch = self
                        .scratch
                        .as_mut()
                        .ok_or_else(|| DecodeError("missing PCM scratch".into()))?;
                    scratch.copy_interleaved_ref(decoded);
                    let samples = scratch.samples();
                    if !samples.len().is_multiple_of(self.channels)
                        || samples.iter().any(|v| !v.is_finite())
                    {
                        return Err(DecodeError("incomplete or non-finite PCM sample".into()));
                    }
                    if self.strict_codec.is_some() && samples.is_empty() {
                        continue;
                    }
                    if self.strict_codec.is_some() {
                        self.decoded_frames = self
                            .decoded_frames
                            .checked_add((samples.len() / self.channels) as u64)
                            .ok_or_else(|| DecodeError("audio frame count overflow".into()))?;
                        if self
                            .expected_frames
                            .is_some_and(|n| self.decoded_frames > n)
                        {
                            return Err(DecodeError(
                                "ALAC exceeds declared PCM frame count".into(),
                            ));
                        }
                    }
                    return Ok(Some(samples.to_vec()));
                }
                Err(SymError::ResetRequired) => {
                    self.failed = true;
                    return Err(DecodeError(
                        "audio codec requires a new decoder after reset or panic".into(),
                    ));
                }
                Err(error) if self.strict_codec.is_some() => {
                    self.failed = true;
                    return Err(DecodeError(format!(
                        "{} packet error: {error}",
                        self.strict_codec.unwrap_or("native codec")
                    )));
                }
                Err(SymError::DecodeError(_)) => continue,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None);
                }
                Err(e) => return Err(DecodeError(format!("decode error: {e}"))),
            }
        }
    }
}
#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decode_guard_tests.rs"]
mod guard_tests;
