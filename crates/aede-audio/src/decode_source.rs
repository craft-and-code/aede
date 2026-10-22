//! A metadata-free view of native FLAC for the playback demuxer.
//!
//! Playback needs STREAMINFO and audio, not artwork or textual metadata. The
//! pinned demuxer's metadata readers can allocate from nested lengths before
//! checking the containing block. Skip those bodies on the original descriptor
//! and expose only a fixed-size synthetic header followed by untouched audio.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSource, MediaSourceStream, ReadBytes, SeekBuffered};
use symphonia::core::probe::{Instantiate, Probe, QueryDescriptor};
use symphonia::default::formats::FlacReader;

use super::DecodeError;

const PREFIX_BYTES: usize = 42;
// Bound even zero-length blocks so hostile metadata cannot monopolize a
// playback worker with endless seeks; artwork bodies remain uncapped.
const MAX_METADATA_BLOCKS: usize = 65_536;
const MAX_LEADING_ID3_BLOCKS: usize = 65_536;

/// An opened FLAC-capable container selected by the protected source probe.
pub struct FlacProbe {
    /// Reader retaining the same descriptor supplied to [`FlacAudioSource::probe`].
    pub format: Box<dyn FormatReader>,
    /// Whether the selected marker was native FLAC rather than Ogg.
    pub native_container: bool,
}

impl FlacAudioSource {
    /// Probe a regular opened file for native or Ogg FLAC without decoding audio.
    ///
    /// Native FLAC uses the same metadata-free source as ordinary shared decoding,
    /// including scanned prefixes and contiguous leading ID3 blocks. Other format
    /// markers return `None` without constructing their reader. This supplies no
    /// PCM or MD5 verdict; the caller selects and configures its codec validator.
    pub fn probe(file: File) -> Result<Option<FlacProbe>, DecodeError> {
        if !file.metadata().map_err(decode_error)?.is_file() {
            return Err(decode_error("FLAC source is not a regular file"));
        }
        let input = prepare(file)?;
        let mut source = MediaSourceStream::new(input, Default::default());
        let file_length = source
            .byte_len()
            .ok_or_else(|| decode_error("FLAC source length is unknown"))?;
        let probe = probe();
        let mut id3_blocks = 0;
        loop {
            let selected = match probe.next(&mut source) {
                Ok(selected) => selected,
                Err(_) => return Ok(None),
            };
            let start = source.pos();
            let marker = source.read_quad_bytes().map_err(decode_error)?;
            source.seek_buffered_rev(4);
            match selected {
                Instantiate::Format(instantiate) => {
                    let native_container = &marker == b"fLaC";
                    if !native_container && &marker != b"OggS" {
                        return Ok(None);
                    }
                    let format = match instantiate(source, &FormatOptions::default()) {
                        Ok(format) => format,
                        // A selected native marker must never fall through to an
                        // ordinary decoder after its protected source was refused.
                        Err(error) if native_container => return Err(decode_error(error)),
                        Err(_) => return Ok(None),
                    };
                    return Ok(Some(FlacProbe {
                        format,
                        native_container,
                    }));
                }
                Instantiate::Metadata(_) => {
                    // Discovery needs only ID3's extent. Do not parse tag bodies
                    // merely to decide which audio reader should own the source.
                    if &marker[..3] != b"ID3" {
                        return Ok(None);
                    }
                    if id3_blocks == MAX_LEADING_ID3_BLOCKS {
                        return Err(decode_error(
                            "leading ID3 metadata exceeds the header-count limit",
                        ));
                    }
                    id3_blocks += 1;
                    let mut header = [0; 10];
                    ReadBytes::read_buf_exact(&mut source, &mut header).map_err(decode_error)?;
                    let end = start
                        .checked_add(id3_block_length(&header))
                        .filter(|&end| end <= file_length)
                        .ok_or_else(|| {
                            decode_error("leading ID3 metadata exceeds the source length")
                        })?;
                    source.seek(SeekFrom::Start(end)).map_err(decode_error)?;
                }
            }
        }
    }
}

/// A seekable STREAMINFO-only native FLAC view followed by original audio bytes.
///
/// Optional metadata is skipped without parsing or allocating its bodies. The
/// opened source is never modified, and reads retain its captured length.
pub struct FlacAudioSource {
    source: Box<dyn MediaSource>,
    prefix: [u8; PREFIX_BYTES],
    audio_offset: u64,
    length: u64,
    position: u64,
}

impl FlacAudioSource {
    /// Inspect native metadata on the same opened regular file.
    ///
    /// A leading ID3v2 block, including its optional footer, is skipped from its
    /// fixed-size header. `None` means its following bytes are not native FLAC.
    /// The resulting virtual file consists of `fLaC`, one final STREAMINFO
    /// block and original audio; it never changes the actual file or metadata.
    pub fn new(mut file: File) -> Result<Option<Self>, DecodeError> {
        let metadata = file.metadata().map_err(decode_error)?;
        if !metadata.is_file() {
            return Err(decode_error("FLAC source is not a regular file"));
        }
        let file_length = metadata.len();
        let start = skip_leading_id3(&mut file)
            .map_err(|error| decode_error(format!("input/output error: {error}")))?;
        if start > file_length {
            return Err(decode_error(
                "leading ID3 metadata exceeds the source length",
            ));
        }
        Self::from_source(Box::new(file), start, file_length)
    }

    fn from_source(
        mut source: Box<dyn MediaSource>,
        start: u64,
        file_length: u64,
    ) -> Result<Option<Self>, DecodeError> {
        source.seek(SeekFrom::Start(start)).map_err(decode_error)?;
        let mut magic = [0; 4];
        match source.read_exact(&mut magic) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(decode_error(error)),
        }
        if &magic != b"fLaC" {
            return Ok(None);
        }

        let mut prefix = [0; PREFIX_BYTES];
        prefix[..8].copy_from_slice(b"fLaC\x80\0\0\x22");
        let mut offset = start
            .checked_add(4)
            .filter(|&end| end <= file_length)
            .ok_or_else(|| decode_error("FLAC marker exceeds the source length"))?;
        let mut first = true;
        let mut blocks = 0;
        loop {
            if blocks == MAX_METADATA_BLOCKS {
                return Err(decode_error("FLAC metadata exceeds the header-count limit"));
            }
            blocks += 1;
            let header_end = offset
                .checked_add(4)
                .filter(|&end| end <= file_length)
                .ok_or_else(|| decode_error("FLAC metadata header is truncated"))?;
            source.seek(SeekFrom::Start(offset)).map_err(decode_error)?;
            let mut header = [0; 4];
            source.read_exact(&mut header).map_err(decode_error)?;
            let kind = header[0] & 0x7f;
            let body_length = u64::from(u32::from_be_bytes([0, header[1], header[2], header[3]]));
            let block_end = header_end
                .checked_add(body_length)
                .filter(|&end| end <= file_length)
                .ok_or_else(|| decode_error("FLAC metadata block exceeds the source length"))?;
            if first {
                if kind != 0 || body_length != 34 {
                    return Err(decode_error(
                        "FLAC must begin with a 34-byte STREAMINFO block",
                    ));
                }
                source.read_exact(&mut prefix[8..]).map_err(decode_error)?;
                first = false;
            } else if kind == 0 {
                return Err(decode_error("FLAC has duplicate STREAMINFO metadata"));
            }
            if kind == 127 {
                return Err(decode_error("FLAC has forbidden metadata type 127"));
            }
            offset = block_end;
            if header[0] & 0x80 != 0 {
                break;
            }
        }
        source.seek(SeekFrom::Start(offset)).map_err(decode_error)?;
        let length = (PREFIX_BYTES as u64)
            .checked_add(file_length - offset)
            .ok_or_else(|| decode_error("FLAC virtual source length is unrepresentable"))?;
        Ok(Some(Self {
            source,
            prefix,
            audio_offset: offset,
            length,
            position: 0,
        }))
    }
}

impl Read for FlacAudioSource {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() || self.position >= self.length {
            return Ok(0);
        }
        if self.position < PREFIX_BYTES as u64 {
            let start = self.position as usize;
            let count = buffer.len().min(PREFIX_BYTES - start);
            buffer[..count].copy_from_slice(&self.prefix[start..start + count]);
            self.position += count as u64;
            return Ok(count);
        }
        let remaining = usize::try_from(self.length - self.position).unwrap_or(usize::MAX);
        let limit = buffer.len().min(remaining);
        let count = self.source.read(&mut buffer[..limit])?;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for FlacAudioSource {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let requested = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        let requested = u64::try_from(requested).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid FLAC virtual seek")
        })?;
        let audio_position = requested.saturating_sub(PREFIX_BYTES as u64);
        let original = self
            .audio_offset
            .checked_add(audio_position)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "FLAC virtual seek exceeds the source range",
                )
            })?;
        self.source.seek(SeekFrom::Start(original))?;
        self.position = requested;
        Ok(requested)
    }
}

impl MediaSource for FlacAudioSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.length)
    }
}

/// Bypass leading ID3 bodies only when they precede a native FLAC stream.
pub(super) fn prepare(mut file: File) -> Result<Box<dyn MediaSource>, DecodeError> {
    let file_length = file.metadata().map_err(decode_error)?.len();
    let Some(start) = leading_id3_flac_start(&mut file, file_length)? else {
        file.seek(SeekFrom::Start(0)).map_err(decode_error)?;
        return Ok(Box::new(file));
    };
    let source = FlacAudioSource::from_source(Box::new(file), start, file_length)?
        .ok_or_else(|| decode_error("FLAC marker was not found after leading ID3 metadata"))?;
    Ok(Box::new(source))
}

fn leading_id3_flac_start(file: &mut File, file_length: u64) -> Result<Option<u64>, DecodeError> {
    let mut start = 0_u64;
    let mut blocks = 0;
    loop {
        file.seek(SeekFrom::Start(start)).map_err(decode_error)?;
        let mut marker = [0; 4];
        match file.read_exact(&mut marker) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(decode_error(error)),
        }
        if &marker == b"fLaC" {
            return Ok((blocks != 0).then_some(start));
        }
        if &marker[..3] != b"ID3" {
            return Ok(None);
        }
        if blocks == MAX_LEADING_ID3_BLOCKS {
            return Err(decode_error(
                "leading ID3 metadata exceeds the header-count limit",
            ));
        }
        blocks += 1;
        let mut header = [0; 10];
        header[..4].copy_from_slice(&marker);
        match file.read_exact(&mut header[4..]) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(decode_error(error)),
        }
        start = start
            .checked_add(id3_block_length(&header))
            .filter(|&end| end <= file_length)
            .ok_or_else(|| decode_error("leading ID3 metadata exceeds the source length"))?;
    }
}

// Preserve the catalogue tag helper's header-only ID3 size/footer semantics
// without introducing an application or tag-parser dependency into decoding.
fn skip_leading_id3(file: &mut File) -> io::Result<u64> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 10];
    match file.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(0),
        Err(error) => return Err(error),
    }
    if &header[..3] != b"ID3" {
        return Ok(0);
    }
    Ok(id3_block_length(&header))
}

fn id3_block_length(header: &[u8; 10]) -> u64 {
    let size = header[6..10]
        .iter()
        .fold(0_u64, |size, byte| (size << 7) | u64::from(byte & 0x7f));
    10 + size + if header[5] & 0x10 != 0 { 10 } else { 0 }
}

/// Retain Symphonia's enabled registry and replace only native FLAC creation.
pub(super) fn probe() -> Probe {
    let mut probe = Probe::default();
    for descriptor in FlacReader::query() {
        let mut descriptor = *descriptor;
        descriptor.inst = Instantiate::Format(flac_reader);
        probe.register(&descriptor);
    }
    symphonia::default::register_enabled_formats(&mut probe);
    probe
}

fn flac_reader(
    source: MediaSourceStream,
    options: &FormatOptions,
) -> Result<Box<dyn FormatReader>, SymError> {
    // The probe may have consumed leading tags or scanned unrelated bytes.
    // Wrap its actual selected marker, preserving those established offsets.
    let start = source.pos();
    let file_length = source
        .byte_len()
        .ok_or(SymError::Unsupported("FLAC source length is unknown"))?;
    let source = FlacAudioSource::from_source(Box::new(source), start, file_length)
        .map_err(|error| SymError::IoError(io::Error::new(io::ErrorKind::InvalidData, error)))?
        .ok_or(SymError::DecodeError(
            "FLAC marker was not found at the probed position",
        ))?;
    let reader = FlacReader::try_new(
        MediaSourceStream::new(Box::new(source), Default::default()),
        options,
    )?;
    Ok(Box::new(reader))
}

fn decode_error(error: impl std::fmt::Display) -> DecodeError {
    DecodeError(error.to_string())
}
