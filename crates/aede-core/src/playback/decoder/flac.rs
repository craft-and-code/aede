//! Progressive FLAC decoding with verification of the original integer PCM.
//!
//! The FlacCompagnon preview API does not expose decoder verification options.
//! Use the already shared Symphonia codec here: its validator hashes samples
//! before conversion to float, with FLAC's signed little-endian interleaving.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use aede_dsp::ChannelLayout;
use flaccompagnon_core::AnalysisError;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{CODEC_TYPE_FLAC, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::{Error, FlacMd5Status};

pub(super) struct FlacStream {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    spec: SignalSpec,
    buffer: Option<SampleBuffer<f32>>,
    declared_frames: Option<u64>,
    decoded_frames: u64,
    bits_per_sample: u32,
    native_container: bool,
    status: FlacMd5Status,
    finished: bool,
    failure: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub layout: ChannelLayout,
}

impl FlacStream {
    pub fn open(path: &Path) -> Result<Option<Self>, Error> {
        catch_unwind(AssertUnwindSafe(|| Self::open_inner(path)))
            .unwrap_or_else(|_| Err(decode_error("FLAC decoder rejected a malformed header")))
    }

    fn open_inner(path: &Path) -> Result<Option<Self>, Error> {
        let mut file = File::open(path).map_err(decode_error)?;
        let mut magic = [0; 4];
        match file.read_exact(&mut magic) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(decode_error(error)),
        }
        let native_container = &magic == b"fLaC" || &magic[..3] == b"ID3";
        if !native_container && &magic != b"OggS" {
            return Ok(None);
        }
        file.seek(SeekFrom::Start(0)).map_err(decode_error)?;
        let input: Box<dyn MediaSource> = if native_container {
            match super::flac_source::FlacAudioSource::new(file)? {
                Some(source) => Box::new(source),
                None => return Ok(None),
            }
        } else {
            Box::new(file)
        };
        let source = MediaSourceStream::new(input, Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            hint.with_extension(extension);
        }
        let probed = match symphonia::default::get_probe().format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        ) {
            Ok(probed) => probed,
            // Other ID3/Ogg codecs retain their existing playback path.
            Err(error) if &magic == b"fLaC" => return Err(decode_error(error)),
            Err(_) => return Ok(None),
        };
        let track = probed.format.default_track().ok_or(Error::InvalidFormat)?;
        if track.codec_params.codec != CODEC_TYPE_FLAC {
            return Ok(None);
        }
        if probed.format.tracks().len() != 1 {
            return Err(decode_error("multiplexed FLAC playback is unsupported"));
        }
        // Native and Ogg readers supply the 34-byte STREAMINFO as extra data.
        // An all-zero signature is absent: avoid meaningless hash work.
        let has_signature = track
            .codec_params
            .extra_data
            .as_deref()
            .and_then(|info| info.get(18..34))
            .is_some_and(|digest| digest.iter().any(|&byte| byte != 0));
        let decoder = symphonia::default::get_codecs()
            .make(
                &track.codec_params,
                &DecoderOptions {
                    verify: has_signature,
                },
            )
            .map_err(decode_error)?;
        let params = decoder.codec_params();
        let sample_rate = params.sample_rate.ok_or(Error::InvalidFormat)?;
        let mask = params.channels.ok_or(Error::InvalidFormat)?;
        let channels = u16::try_from(mask.count()).map_err(|_| Error::InvalidFormat)?;
        let layout = ChannelLayout::from_mask(mask.bits()).map_err(|_| Error::InvalidFormat)?;
        let bits_per_sample = params.bits_per_sample.ok_or(Error::InvalidFormat)?;
        if sample_rate == 0 || channels == 0 || !(4..=32).contains(&bits_per_sample) {
            return Err(Error::InvalidFormat);
        }
        let declared_frames = params.n_frames.filter(|&frames| frames != 0);
        let track_id = track.id;
        Ok(Some(Self {
            format: probed.format,
            decoder,
            track_id,
            spec: SignalSpec::new(sample_rate, mask),
            buffer: None,
            declared_frames,
            decoded_frames: 0,
            bits_per_sample,
            native_container,
            status: if has_signature {
                FlacMd5Status::Pending
            } else {
                FlacMd5Status::NoSignature
            },
            finished: false,
            failure: None,
            sample_rate,
            channels,
            layout,
        }))
    }

    pub fn md5_status(&self) -> FlacMd5Status {
        self.status
    }

    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, Error> {
        if self.status == FlacMd5Status::Mismatch {
            return Err(Error::FlacMd5Mismatch);
        }
        if let Some(failure) = &self.failure {
            return Err(decode_error(failure));
        }
        if self.finished {
            return Ok(None);
        }
        // The pinned codec has unchecked arithmetic/assertions on malformed
        // frames. Contain those failures here and never expose clean EOF after
        // a partially advanced decoder has failed. No panic recovery resumes it.
        let result =
            catch_unwind(AssertUnwindSafe(|| self.next_chunk_inner())).unwrap_or_else(|_| {
                Err(decode_error(
                    "FLAC decoder rejected an unsupported or malformed frame",
                ))
            });
        if let Err(error) = &result {
            self.failure = Some(error.to_string());
        }
        result
    }

    fn next_chunk_inner(&mut self) -> Result<Option<Vec<f32>>, Error> {
        let packet = match self.format.next_packet() {
            Ok(packet) => packet,
            Err(SymError::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                return self.finish();
            }
            Err(error) => return Err(decode_error(error)),
        };
        if packet.track_id() != self.track_id {
            return Err(decode_error("multiplexed FLAC playback is unsupported"));
        }
        if self.native_container && packet.ts != self.decoded_frames {
            return Err(decode_error(
                "FLAC frames are missing, repeated or out of order",
            ));
        }
        let header = *packet.data.get(3).ok_or(Error::IncompleteFrame)?;
        let frame_bits = match (header >> 1) & 7 {
            0 => self.bits_per_sample,
            1 => 8,
            2 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            7 => 32,
            _ => return Err(Error::InvalidFormat),
        };
        let assignment = header >> 4;
        let frame_channels = match assignment {
            0..=7 => u16::from(assignment) + 1,
            8..=10 => 2,
            _ => return Err(Error::InvalidFormat),
        };
        if frame_bits != self.bits_per_sample
            || frame_channels != self.channels
            || frame_sample_rate(&packet.data, self.sample_rate) != Some(self.sample_rate)
        {
            return Err(Error::InvalidFormat);
        }
        // A correlated 32-bit stereo frame needs a 33-bit side channel, which
        // Symphonia 0.5.5 cannot decode. Independent 32-bit channels remain valid.
        if frame_bits == 32 && assignment >= 8 {
            return Err(decode_error(
                "32-bit correlated FLAC stereo is unsupported by the playback decoder",
            ));
        }
        let decoded = self.decoder.decode(&packet).map_err(decode_error)?;
        if *decoded.spec() != self.spec {
            return Err(Error::InvalidFormat);
        }
        let frames = decoded.frames() as u64;
        if frames == 0 || packet.dur != frames || packet.trim_start != 0 || packet.trim_end != 0 {
            return Err(decode_error(
                "FLAC packet disagrees with its decoded frame count",
            ));
        }
        self.decoded_frames = self
            .decoded_frames
            .checked_add(frames)
            .ok_or(Error::InvalidFormat)?;
        if self
            .declared_frames
            .is_some_and(|expected| self.decoded_frames > expected)
        {
            return Err(decode_error("FLAC exceeds its declared source-frame count"));
        }
        let capacity = decoded.capacity() * usize::from(self.channels);
        if self
            .buffer
            .as_ref()
            .is_none_or(|buffer| buffer.capacity() < capacity)
        {
            self.buffer = Some(SampleBuffer::new(decoded.capacity() as u64, self.spec));
        }
        let buffer = self.buffer.as_mut().ok_or(Error::InvalidBuffer)?;
        buffer.copy_interleaved_ref(decoded);
        Ok(Some(buffer.samples().to_vec()))
    }

    fn finish(&mut self) -> Result<Option<Vec<f32>>, Error> {
        if self
            .declared_frames
            .is_some_and(|expected| self.decoded_frames != expected)
        {
            return Err(decode_error(
                "FLAC ended before its declared source-frame count",
            ));
        }
        if self.status == FlacMd5Status::Pending {
            match self.decoder.finalize().verify_ok {
                Some(true) => self.status = FlacMd5Status::Verified,
                Some(false) => {
                    self.status = FlacMd5Status::Mismatch;
                    return Err(Error::FlacMd5Mismatch);
                }
                None => {
                    return Err(decode_error(
                        "FLAC decoder did not provide MD5 verification",
                    ));
                }
            }
        }
        self.finished = true;
        Ok(None)
    }
}

// Ogg's FLAC mapper checks frame CRCs, but unlike the native reader it does
// not check format declarations against STREAMINFO. Inspect only the small
// header; the validated UTF-8 counter and block-size suffix precede the rate.
fn frame_sample_rate(data: &[u8], stream_rate: u32) -> Option<u32> {
    let codes = [
        0, 88_200, 176_400, 192_000, 8_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 96_000,
    ];
    let code = data.get(2)? & 15;
    match code {
        0 => Some(stream_rate),
        1..=11 => Some(codes[usize::from(code)]),
        12..=14 => {
            let counter_bytes = match data.get(4)?.leading_ones() {
                0 => 1,
                size @ 2..=7 => size as usize,
                _ => return None,
            };
            let block_bytes = match data[2] >> 4 {
                6 => 1,
                7 => 2,
                _ => 0,
            };
            let offset = 4 + counter_bytes + block_bytes;
            if code == 12 {
                Some(u32::from(*data.get(offset)?) * 1000)
            } else {
                let bytes: [u8; 2] = data.get(offset..offset + 2)?.try_into().ok()?;
                Some(u32::from(u16::from_be_bytes(bytes)) * if code == 14 { 10 } else { 1 })
            }
        }
        _ => None,
    }
}

fn decode_error(error: impl std::fmt::Display) -> Error {
    Error::Decode(AnalysisError::Decode(error.to_string()))
}

#[cfg(test)]
#[path = "flac_tests.rs"]
mod tests;
