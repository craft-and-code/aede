//! Vorbis granule trimming around the pinned Symphonia Ogg demuxer.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use aede_dsp::ChannelLayout;
use flaccompagnon_core::AnalysisError;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{CODEC_TYPE_VORBIS, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::Error;

pub(super) struct VorbisStream {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    spec: SignalSpec,
    buffer: Option<SampleBuffer<f32>>,
    skip_frames: u64,
    remaining_frames: u64,
    discarded_tail: usize,
    last_packet_frames: usize,
    pub sample_rate: u32,
    pub channels: u16,
    pub layout: ChannelLayout,
}

impl VorbisStream {
    pub fn open(path: &Path) -> Result<Option<Self>, Error> {
        let mut file = File::open(path).map_err(decode_error)?;
        let Some(initial) = first_audio_page(&mut file)? else {
            return Ok(None);
        };
        file.seek(SeekFrom::Start(0)).map_err(decode_error)?;
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions {
                    enable_gapless: true,
                    ..Default::default()
                },
                &MetadataOptions::default(),
            )
            .map_err(decode_error)?;
        let track = probed.format.default_track().ok_or(Error::InvalidFormat)?;
        let params = &track.codec_params;
        if track.id != initial.serial || params.codec != CODEC_TYPE_VORBIS {
            return Err(decode_error("multiplexed Vorbis playback is unsupported"));
        }
        let sample_rate = params.sample_rate.ok_or(Error::InvalidFormat)?;
        let speaker_mask = params.channels.ok_or(Error::InvalidFormat)?;
        let channels = u16::try_from(speaker_mask.count()).map_err(|_| Error::InvalidFormat)?;
        let layout =
            ChannelLayout::from_mask(speaker_mask.bits()).map_err(|_| Error::InvalidFormat)?;
        // n_frames is populated from a validated EOS page by the Ogg reader.
        // Without that bound, exact completion cannot be established.
        let remaining_frames = params
            .n_frames
            .ok_or_else(|| decode_error("Vorbis stream has no valid end-of-stream granule"))?;
        // An EOS first audio page can describe tail padding as `delay` in
        // Symphonia 0.5.5. A non-EOS first page instead describes a real crop.
        let skip_frames = if initial.is_final {
            0
        } else {
            u64::from(params.delay.unwrap_or(0))
        };
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(decode_error)?;
        Ok(Some(Self {
            format: probed.format,
            decoder,
            track_id: initial.serial,
            spec: SignalSpec::new(sample_rate, speaker_mask),
            buffer: None,
            skip_frames,
            remaining_frames,
            discarded_tail: 0,
            last_packet_frames: 0,
            sample_rate,
            channels,
            layout,
        }))
    }

    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, Error> {
        loop {
            let mut packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymError::IoError(error))
                    if error.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    if self.remaining_frames != 0 || self.skip_frames != 0 {
                        return Err(decode_error(
                            "Vorbis ended before its declared playable frames",
                        ));
                    }
                    if self.discarded_tail > self.last_packet_frames {
                        return Err(decode_error(
                            "Vorbis final granule trims more than its final packet",
                        ));
                    }
                    return Ok(None);
                }
                Err(SymError::ResetRequired) => {
                    return Err(decode_error("chained Vorbis playback is unsupported"));
                }
                Err(error) => return Err(decode_error(error)),
            };
            if packet.track_id() != self.track_id {
                return Err(decode_error("multiplexed Vorbis playback is unsupported"));
            }
            // The initial page was buffered before Symphonia probed its bounds.
            // Apply both bounds ourselves so no packet is missed or trimmed twice.
            packet.trim_start = 0;
            packet.trim_end = 0;
            let decoded = self.decoder.decode(&packet).map_err(decode_error)?;
            if *decoded.spec() != self.spec {
                return Err(Error::InvalidFormat);
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
            let channels = usize::from(self.channels);
            let frames = buffer.samples().len() / channels;
            self.last_packet_frames = frames;
            let skip = self.skip_frames.min(frames as u64) as usize;
            self.skip_frames -= skip as u64;
            let take = self.remaining_frames.min((frames - skip) as u64) as usize;
            self.remaining_frames -= take as u64;
            self.discarded_tail = self.discarded_tail.saturating_add(frames - skip - take);
            if take != 0 {
                return Ok(Some(
                    buffer.samples()[skip * channels..(skip + take) * channels].to_vec(),
                ));
            }
            // Continue through the physical EOF even after the PCM bound: a
            // chained stream or decoder error must not become successful EOF.
        }
    }
}

struct InitialPage {
    serial: u32,
    is_final: bool,
}

/// Inspect bounded header pages on the same handle that the decoder will own.
/// The page flag is needed to distinguish a prefix crop from a one-page tail.
fn first_audio_page(file: &mut File) -> Result<Option<InitialPage>, Error> {
    const MAX_HEADER_BYTES: usize = 8 * 1024 * 1024;
    let mut serial = None;
    let mut headers = 0;
    let mut total_bytes = 0;
    loop {
        let mut header = [0u8; 27];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if serial.is_none() && error.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(error) => return Err(decode_error(error)),
        }
        if header.get(..4) != Some(b"OggS") {
            if serial.is_none() {
                return Ok(None);
            }
            return Err(decode_error("invalid Vorbis Ogg page capture"));
        }
        let segment_count = usize::from(*header.get(26).ok_or(Error::InvalidFormat)?);
        let mut lacing = vec![0; segment_count];
        file.read_exact(&mut lacing).map_err(decode_error)?;
        let body_len = lacing
            .iter()
            .map(|&length| usize::from(length))
            .sum::<usize>();
        let mut body = vec![0; body_len];
        file.read_exact(&mut body).map_err(decode_error)?;
        let mut page = Vec::with_capacity(header.len() + segment_count + body_len);
        page.extend_from_slice(&header);
        page.extend_from_slice(&lacing);
        page.extend_from_slice(&body);
        total_bytes += page.len();
        if total_bytes > MAX_HEADER_BYTES {
            return Err(decode_error(
                "Vorbis headers exceed the 8 MiB playback limit",
            ));
        }
        let flags = *header.get(5).ok_or(Error::InvalidFormat)?;
        if serial.is_none() && body.get(..7) != Some(b"\x01vorbis") {
            return Ok(None);
        }
        let expected_crc = read_u32(&header, 22)?;
        page.get_mut(22..26).ok_or(Error::InvalidFormat)?.fill(0);
        if header.get(4) != Some(&0)
            || flags & !7 != 0
            || crate::audit::crc::crc32_ogg(&page) != expected_crc
        {
            return Err(decode_error("invalid Vorbis Ogg header page or checksum"));
        }
        let page_serial = read_u32(&header, 14)?;
        let granule: [u8; 8] = header
            .get(6..14)
            .ok_or(Error::InvalidFormat)?
            .try_into()
            .map_err(|_| Error::InvalidFormat)?;
        if u64::from_le_bytes(granule) > i64::MAX as u64
            && lacing.iter().any(|&length| length < 255)
        {
            return Err(decode_error(
                "Vorbis completed packets have an invalid granule",
            ));
        }
        let selected = *serial.get_or_insert(page_serial);
        if selected != page_serial {
            return Err(decode_error("multiplexed Vorbis playback is unsupported"));
        }
        for length in lacing {
            if headers == 3 {
                return Ok(Some(InitialPage {
                    serial: selected,
                    is_final: flags & 4 != 0,
                }));
            }
            if length < 255 {
                headers += 1;
            }
        }
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let value = bytes.get(offset..offset + 4).ok_or(Error::InvalidFormat)?;
    let value: [u8; 4] = value.try_into().map_err(|_| Error::InvalidFormat)?;
    Ok(u32::from_le_bytes(value))
}

fn decode_error(error: impl std::fmt::Display) -> Error {
    Error::Decode(AnalysisError::Decode(error.to_string()))
}
