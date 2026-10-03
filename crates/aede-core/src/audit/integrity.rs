//! Is the file still what it was when it was written?
//!
//! Every other check in this module describes what a file *is*. This one asks
//! whether it is still intact — the question that matters on a library that has
//! been moved from disk to disk for ten years.
//!
//! The answer comes from the checksums the containers already carry, so nothing
//! has to be decoded and no reference copy is needed:
//!
//! - a FLAC frame ends with a **CRC-16** over its own bytes, and its header
//!   carries a **CRC-8**;
//! - an Ogg page carries a **CRC-32** over the whole page.
//!
//! Both catch what actually happens to stored files: a flipped bit, a bad
//! sector, a truncated copy. What they do not catch is a stream re-encoded
//! consistently — for that, FLAC also stores an MD5 of the *decoded* audio in
//! STREAMINFO, and checking it means decoding. That verdict will come with the
//! decoder; what is stored here is shaped to accommodate it without changing.
//!
//! This verifier does not support checksums for MP3, MP4, WAV or AIFF. Their
//! answer is "nothing to check", which is distinct from "not checked yet" and
//! does not establish that the file is healthy.

use std::path::Path;

use super::bits::BitReader;
use super::crc;
use crate::tags::TagError;
use crate::tags::bytes::read_at_most;

/// Sanity bound on how much is read from a single file.
///
/// Unlike the resolution audit, this check is meaningless on a sample: the one
/// frame left out is exactly where the damage will be. It reads the file whole,
/// and this limit only guards against a pathological input.
const MAX_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// What is known about the integrity of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// This verifier has no supported checksum for the container. Repeating
    /// the same check gives no integrity evidence; other checks may exist.
    NothingToCheck,
    /// Every checksum matched.
    Intact,
    /// At least one did not: the file has been damaged since it was written.
    Damaged {
        /// Where it went wrong, in plain words, so a report can name it.
        detail: String,
    },
}

impl Verdict {
    /// Short lowercase name, for a report line.
    pub fn label(&self) -> &'static str {
        match self {
            Verdict::NothingToCheck => "nothing to check",
            Verdict::Intact => "intact",
            Verdict::Damaged { .. } => "damaged",
        }
    }

    /// Key used in the catalog file and in the SQL schema.
    pub fn key(&self) -> &'static str {
        match self {
            Verdict::NothingToCheck => "nothing_to_check",
            Verdict::Intact => "intact",
            Verdict::Damaged { .. } => "damaged",
        }
    }
}

/// The outcome of one verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// What was concluded.
    pub verdict: Verdict,
    /// How it was reached, kept so a stored verdict can be read later knowing
    /// what it was worth: a frame checksum is not an MD5 of the decoded audio.
    pub method: &'static str,
    /// Number of frames or pages verified.
    pub units: usize,
}

impl Report {
    fn nothing() -> Report {
        Report {
            verdict: Verdict::NothingToCheck,
            method: "none",
            units: 0,
        }
    }
}

/// Verifies the checksums a file carries.
///
/// An unreadable file is an error; a readable but damaged one is a verdict, not
/// an error — being damaged is the answer to the question, not a failure to
/// answer it.
/// Supported streams exceeding the 2 GiB read budget are refused before
/// loading their audio, rather than returning a verdict for a prefix.
pub fn check(path: &Path) -> Result<Report, TagError> {
    check_with_limit(path, MAX_BYTES)
}

fn check_with_limit(path: &Path, max_bytes: usize) -> Result<Report, TagError> {
    let mut file = std::fs::File::open(path)?;
    let head = read_at_most(&mut file, 0, 4)?;
    if head.len() < 4 {
        return Err(TagError::UnrecognizedFormat);
    }
    if &head[..4] == b"OggS" {
        return check_ogg(&mut file, max_bytes);
    }
    // A FLAC stream may sit behind an ID3v2 tag, which is where `fLaC` lands.
    let start = crate::tags::id3::skip_id3v2(&mut file)?;
    let signature = read_at_most(&mut file, start, 4)?;
    if signature.len() == 4 && &signature[..] == b"fLaC" {
        return check_flac(&mut file, max_bytes);
    }
    Ok(Report::nothing())
}

/// Walks every frame of a FLAC stream and compares the two checksums it carries
/// with the ones computed from its bytes.
fn check_flac(file: &mut std::fs::File, max_bytes: usize) -> Result<Report, TagError> {
    let (stream_info, audio_start) = super::flac::read_stream_info(file)?;
    let data = read_complete(file, audio_start, max_bytes)?;
    let mut reader = BitReader::new(&data);
    let mut frames = 0usize;
    let mut verified_to = 0usize;

    while let Some(frame) = super::flac::read_frame(&mut reader, &stream_info) {
        let span = &frame.span;
        if span.end > data.len() || span.end < 2 {
            return Ok(damaged(
                frames,
                format!("frame {} runs past the end of the file", frames + 1),
            ));
        }
        if crc::crc8(&data[span.start..span.header_end]) != span.stored_crc8 {
            return Ok(damaged(
                frames,
                format!("frame {}: header checksum mismatch", frames + 1),
            ));
        }
        if crc::crc16(&data[span.start..span.end - 2]) != span.stored_crc16 {
            return Ok(damaged(
                frames,
                format!("frame {}: audio checksum mismatch", frames + 1),
            ));
        }

        frames += 1;
        verified_to = span.end;
        reader.align_to_byte();
        if reader.is_exhausted() {
            break;
        }
    }

    // The walk stops on anything it cannot parse. What proves the file whole is
    // not where the reader gave up — a failed parse consumes bits on its way —
    // but where the last *verified* frame ended: it has to be the end of the
    // file. This is what catches a truncated copy, whose surviving frames are
    // all perfectly valid.
    if !is_end_of_stream(&data, verified_to) {
        return Ok(damaged(
            frames,
            format!("stream unreadable after frame {frames}"),
        ));
    }
    if frames == 0 {
        return Ok(damaged(0, "no readable audio frame".to_string()));
    }

    Ok(Report {
        verdict: Verdict::Intact,
        method: FLAC_METHOD,
        units: frames,
    })
}

/// Walks every Ogg page and compares its declared CRC-32 with the one computed
/// over the page, its own checksum field read as zero.
fn check_ogg(file: &mut std::fs::File, max_bytes: usize) -> Result<Report, TagError> {
    let data = read_complete(file, 0, max_bytes)?;
    let mut offset = 0usize;
    let mut pages = 0usize;

    while offset + 27 <= data.len() {
        if &data[offset..offset + 4] != b"OggS" {
            return Ok(damaged_ogg(
                pages,
                format!("no page header after page {pages}"),
            ));
        }
        let segment_count = data[offset + 26] as usize;
        let table_end = offset + 27 + segment_count;
        if table_end > data.len() {
            return Ok(damaged_ogg(
                pages,
                format!("page {} is truncated", pages + 1),
            ));
        }
        let payload: usize = data[offset + 27..table_end]
            .iter()
            .map(|&length| length as usize)
            .sum();
        let page_end = table_end + payload;
        if page_end > data.len() {
            return Ok(damaged_ogg(
                pages,
                format!("page {} runs past the end of the file", pages + 1),
            ));
        }

        let stored = u32::from_le_bytes([
            data[offset + 22],
            data[offset + 23],
            data[offset + 24],
            data[offset + 25],
        ]);
        // The checksum covers the page including its own field, which is read
        // as zero for the computation.
        let mut page = data[offset..page_end].to_vec();
        page[22..26].fill(0);
        if crc::crc32_ogg(&page) != stored {
            return Ok(damaged_ogg(
                pages,
                format!("page {}: checksum mismatch", pages + 1),
            ));
        }

        pages += 1;
        offset = page_end;
    }

    if !is_end_of_stream(&data, offset) {
        return Ok(damaged_ogg(
            pages,
            format!("trailing bytes after page {pages}"),
        ));
    }
    if pages == 0 {
        return Ok(damaged_ogg(0, "no readable page".to_string()));
    }

    Ok(Report {
        verdict: Verdict::Intact,
        method: OGG_METHOD,
        units: pages,
    })
}

/// A checksum verdict covers the complete stream, never just the part that
/// happened to fit in memory and end at a valid frame or page boundary.
fn read_complete(
    file: &mut std::fs::File,
    start: u64,
    max_bytes: usize,
) -> Result<Vec<u8>, TagError> {
    let remaining = file.metadata()?.len().saturating_sub(start);
    if remaining > max_bytes as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "stream exceeds the checksum verification read limit",
        )
        .into());
    }
    // The budget is a bound, not an allocation target: a small file needs only
    // its measured remaining bytes, even when the caller allows 2 GiB.
    let data = read_at_most(file, start, remaining as usize)?;
    if data.len() as u64 != remaining || file.metadata()?.len().saturating_sub(start) != remaining {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "stream size changed during checksum verification",
        )
        .into());
    }
    Ok(data)
}

/// `true` when nothing but a legitimate trailer follows `offset`.
///
/// An ID3v1 block is 128 bytes at the very end of the file and is not part of
/// the audio stream; a few taggers leave one behind on FLAC and Ogg files.
/// Anything else after the last frame means the file is not what it was.
fn is_end_of_stream(data: &[u8], offset: usize) -> bool {
    match data.len().checked_sub(offset) {
        Some(0) => true,
        Some(128) => &data[offset..offset + 3] == b"TAG",
        _ => false,
    }
}

/// Name of the FLAC method, as stored in the catalog.
pub const FLAC_METHOD: &str = "flac-frame-crc";
/// Name of the Ogg method, as stored in the catalog.
pub const OGG_METHOD: &str = "ogg-page-crc";

fn damaged(units: usize, detail: String) -> Report {
    Report {
        verdict: Verdict::Damaged { detail },
        method: FLAC_METHOD,
        units,
    }
}

fn damaged_ogg(units: usize, detail: String) -> Report {
    Report {
        verdict: Verdict::Damaged { detail },
        method: OGG_METHOD,
        units,
    }
}

#[cfg(test)]
#[path = "integrity_tests.rs"]
mod tests;
