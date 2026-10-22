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

/// A local decode failure, independent of any application's analysis errors.
#[derive(Debug)]
pub struct DecodeError(pub String);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DecodeError {}

/// Packet-at-a-time PCM reader. No device, analyzer or complete-track allocation.
pub struct PcmStreamDecoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    scratch: Option<SampleBuffer<f32>>,
    capacity: u64,
    failed: bool,
    /// Source frames per second.
    pub sample_rate: u32,
    /// Interleaved samples per frame.
    pub channels: usize,
    /// Speaker mask from the same probed header, when available.
    pub channel_mask: Option<u32>,
}
impl PcmStreamDecoder {
    /// Open a local regular file and probe its default audio track with gapless trimming.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        // Opening a FIFO can wait for a writer before descriptor metadata is available.
        if !fs::metadata(path)
            .map_err(|e| DecodeError(e.to_string()))?
            .is_file()
        {
            return Err(DecodeError("audio source is not a regular file".into()));
        }
        let file = File::open(path).map_err(|e| DecodeError(e.to_string()))?;
        if !file
            .metadata()
            .map_err(|e| DecodeError(e.to_string()))?
            .is_file()
        {
            return Err(DecodeError("audio source is not a regular file".into()));
        }
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
        let sample_rate = track
            .codec_params
            .sample_rate
            .filter(|v| *v > 0)
            .ok_or_else(|| DecodeError("missing sample rate".into()))?;
        let channels = track
            .codec_params
            .channels
            .map(|v| v.count())
            .filter(|v| *v > 0 && *v <= 32)
            .ok_or_else(|| DecodeError("missing or unsupported channels".into()))?;
        let channel_mask = track.codec_params.channels.map(|v| v.bits());
        let track_id = track.id;
        let decoder = catch_unwind(AssertUnwindSafe(|| {
            symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())
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
            sample_rate,
            channels,
            channel_mask,
        })
    }
    /// Yield one complete finite PCM packet; retain scratch between calls.
    /// As in the original preview path, recoverable corrupt packets are skipped.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, DecodeError> {
        self.next_chunk_admitted(|| Ok(()))
    }
    /// Retire packet reads (including skipped corrupt/non-audio packets) between I/O stages.
    /// An already running regular-file read is not forcibly interrupted.
    pub fn next_chunk_admitted(
        &mut self,
        mut current: impl FnMut() -> Result<(), DecodeError>,
    ) -> Result<Option<Vec<f32>>, DecodeError> {
        if self.failed {
            return Err(DecodeError(
                "audio decoder cannot continue after a panic".into(),
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
                    return Ok(Some(samples.to_vec()));
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
