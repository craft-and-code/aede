//! A metadata-free view of native FLAC for the playback demuxer.
//!
//! Playback needs STREAMINFO and audio, not artwork or textual metadata. The
//! pinned demuxer's metadata readers can allocate from nested lengths before
//! checking the containing block. Skip those bodies on the original descriptor
//! and expose only a fixed-size synthetic header followed by untouched audio.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

use flaccompagnon_core::AnalysisError;
use symphonia::core::io::MediaSource;

use super::Error;

const PREFIX_BYTES: usize = 42;
// Bound even zero-length blocks so hostile metadata cannot monopolize a
// playback worker with endless seeks; artwork bodies remain uncapped.
const MAX_METADATA_BLOCKS: usize = 65_536;

pub(super) struct FlacAudioSource {
    file: File,
    prefix: [u8; PREFIX_BYTES],
    audio_offset: u64,
    length: u64,
    position: u64,
}

impl FlacAudioSource {
    /// Inspect native metadata on the same opened file, without reading bodies.
    ///
    /// A leading ID3v2 block is skipped using the existing tag-header helper.
    /// `None` means its following bytes are not a native FLAC stream. The
    /// resulting virtual file consists of `fLaC`, one final STREAMINFO block
    /// and original audio; it never changes the actual file or its metadata.
    pub(super) fn new(mut file: File) -> Result<Option<Self>, Error> {
        let metadata = file.metadata().map_err(decode_error)?;
        if !metadata.is_file() {
            return Err(decode_error("FLAC source is not a regular file"));
        }
        let file_length = metadata.len();
        let start = crate::tags::id3::skip_id3v2(&mut file).map_err(decode_error)?;
        if start > file_length {
            return Err(decode_error(
                "leading ID3 metadata exceeds the source length",
            ));
        }
        file.seek(SeekFrom::Start(start)).map_err(decode_error)?;
        let mut magic = [0; 4];
        match file.read_exact(&mut magic) {
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
            file.seek(SeekFrom::Start(offset)).map_err(decode_error)?;
            let mut header = [0; 4];
            file.read_exact(&mut header).map_err(decode_error)?;
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
                file.read_exact(&mut prefix[8..]).map_err(decode_error)?;
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
        file.seek(SeekFrom::Start(offset)).map_err(decode_error)?;
        let length = (PREFIX_BYTES as u64)
            .checked_add(file_length - offset)
            .ok_or_else(|| decode_error("FLAC virtual source length is unrepresentable"))?;
        Ok(Some(Self {
            file,
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
        let count = self.file.read(&mut buffer[..limit])?;
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
        self.file.seek(SeekFrom::Start(original))?;
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

fn decode_error(error: impl std::fmt::Display) -> Error {
    Error::Decode(AnalysisError::Decode(error.to_string()))
}

#[cfg(test)]
#[path = "flac_source_tests.rs"]
mod tests;
