//! Bounded validation before artwork is published.
//!
//! The codecs decode pixels; the container guards require a complete, single
//! image and bound their work before allocation. JPEG has no integrity checksum:
//! a changed stream that remains decodable, including some streams repaired
//! with an early EOI marker, cannot be certified as the original image.

use std::io::{self, BufRead, Cursor, Read, Seek, SeekFrom};
use std::time::{Duration, Instant};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_AXIS: usize = 8192;
const MAX_PIXELS: usize = 16_000_000;
const MAX_OUTPUT: usize = 128 * 1024 * 1024;
const MAX_PARTS: usize = 16_384;
const MAX_SCANS: usize = 64;
const TIME_BUDGET: Duration = Duration::from_secs(5);

/// Validates all image rows before any filesystem mutation.
///
/// PNG checks include every chunk CRC, IDAT Adler-32 and the final IEND.
/// Animated PNG is refused rather than validating only its default frame.
/// JPEG uses the codec's strict mode and requires the final EOI. Input size,
/// dimensions, pixel count and scan/chunk counts bound the work. PNG's internal
/// allocation limit is best effort; the output budget is not a total RSS cap.
/// The deadline is cooperative at reads and PNG rows, with a final check before
/// publication; it cannot interrupt a codec's computation between those points.
pub(crate) fn validate(bytes: &[u8], format: &str) -> Result<(), String> {
    validate_with_budget(bytes, format, TIME_BUDGET)
}

fn validate_with_budget(bytes: &[u8], format: &str, budget: Duration) -> Result<(), String> {
    if bytes.len() > MAX_BYTES {
        return Err("image exceeds the 32 MiB download limit".into());
    }
    let started = Instant::now();
    check_time(started, budget)?;
    match format {
        "jpg" => validate_jpeg(bytes, started, budget)?,
        "png" => validate_png(bytes, started, budget)?,
        _ => return Err("unsupported image format".into()),
    }
    check_time(started, budget)
}

fn check_time(started: Instant, budget: Duration) -> Result<(), String> {
    if started.elapsed() >= budget {
        Err("image validation exceeded its time budget".into())
    } else {
        Ok(())
    }
}

fn dimensions(width: usize, height: usize) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > MAX_AXIS
        || height > MAX_AXIS
        || width.checked_mul(height).is_none_or(|n| n > MAX_PIXELS)
    {
        Err("image exceeds the limits of 8192 pixels per axis and 16 million pixels; choose a smaller Cover Art Archive --size if needed".into())
    } else {
        Ok(())
    }
}

fn png_container(bytes: &[u8], started: Instant, budget: Duration) -> Result<(), String> {
    let mut at = 8;
    for part in 0..MAX_PARTS {
        check_time(started, budget)?;
        let header = bytes.get(at..at + 8).ok_or("truncated PNG chunk")?;
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let end = at
            .checked_add(12)
            .and_then(|at| at.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or("truncated PNG chunk")?;
        let kind = &header[4..8];
        if part == 0 {
            if kind != b"IHDR" || length != 13 {
                return Err("PNG must begin with an IHDR chunk".into());
            }
            let data = &bytes[at + 8..end - 4];
            let width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
            let height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
            dimensions(width, height)?;
        }
        if matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err("animated PNG artwork is not supported".into());
        }
        if kind == b"IEND" {
            return if length == 0 && end == bytes.len() {
                Ok(())
            } else {
                Err("invalid PNG end or data after IEND".into())
            };
        }
        at = end;
    }
    Err("PNG exceeds the chunk-count limit".into())
}

fn validate_png(bytes: &[u8], started: Instant, budget: Duration) -> Result<(), String> {
    png_container(bytes, started, budget)?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_checksums(false);
    options.set_skip_ancillary_crc_failures(false);
    let mut decoder =
        png::Decoder::new_with_options(TimedCursor::new(bytes, started, budget), options);
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    // Compressed ancillary metadata is not needed for pixel validation and
    // must not turn a small picture into an unbounded metadata expansion.
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("invalid PNG: {e}"))?;
    let output = reader
        .output_buffer_size()
        .ok_or("PNG output size overflow")?;
    if output > MAX_OUTPUT {
        return Err("PNG exceeds the decoded-output budget".into());
    }
    if reader.info().color_type == png::ColorType::Indexed {
        let info = reader.info();
        let width = info.width as usize;
        let depth = info.bit_depth as usize;
        let palette_bytes = info.palette.as_ref().map_or(0, |p| p.len());
        let entries = palette_bytes / 3;
        if !matches!(depth, 1 | 2 | 4 | 8)
            || palette_bytes % 3 != 0
            || entries == 0
            || entries > (1 << depth)
        {
            return Err("invalid PNG palette size".into());
        }
        if info
            .trns
            .as_ref()
            .is_some_and(|alpha| alpha.len() > entries)
        {
            return Err("PNG transparency exceeds its palette".into());
        }
        // EXPAND fills missing palette entries with black. Validate the raw
        // indices instead, after the codec has reassembled any Adam7 passes.
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(output)
            .map_err(|_| "cannot allocate PNG validation buffer")?;
        indices.resize(output, 0);
        reader
            .next_frame(&mut indices)
            .map_err(|e| format!("invalid PNG: {e}"))?;
        let row_bytes = (width * depth).div_ceil(8);
        let mask = (1u16 << depth) - 1;
        for row in indices.chunks_exact(row_bytes) {
            check_time(started, budget)?;
            for pixel in 0..width {
                let bit = pixel * depth;
                let index = (u16::from(row[bit / 8]) >> (8 - depth - bit % 8)) & mask;
                if usize::from(index) >= entries {
                    return Err("PNG pixel refers to a missing palette entry".into());
                }
            }
        }
    } else {
        while reader
            .next_interlaced_row()
            .map_err(|e| format!("invalid PNG: {e}"))?
            .is_some()
        {
            check_time(started, budget)?;
        }
    }
    reader.finish().map_err(|e| format!("invalid PNG: {e}"))
}

fn jpeg_container(bytes: &[u8], started: Instant, budget: Duration) -> Result<(), String> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err("missing JPEG start".into());
    }
    let mut at = 2;
    let mut scans = 0;
    for _ in 0..MAX_PARTS {
        check_time(started, budget)?;
        if bytes.get(at) != Some(&0xff) {
            return Err("invalid JPEG marker".into());
        }
        while bytes.get(at) == Some(&0xff) {
            at += 1;
        }
        let marker = *bytes.get(at).ok_or("truncated JPEG marker")?;
        at += 1;
        if marker == 0xd9 {
            return if scans > 0 && at == bytes.len() {
                Ok(())
            } else {
                Err("invalid JPEG end or data after EOI".into())
            };
        }
        if matches!(marker, 0x00 | 0xd8 | 0xd0..=0xd7 | 0x01) {
            return Err("unexpected JPEG marker outside a scan".into());
        }
        let length = bytes.get(at..at + 2).ok_or("truncated JPEG segment")?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        let end = at
            .checked_add(length)
            .filter(|end| length >= 2 && *end <= bytes.len())
            .ok_or("truncated JPEG segment")?;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let frame = bytes
                .get(at + 2..end)
                .filter(|f| f.len() >= 6)
                .ok_or("truncated JPEG frame")?;
            let height = usize::from(u16::from_be_bytes([frame[1], frame[2]]));
            let width = usize::from(u16::from_be_bytes([frame[3], frame[4]]));
            dimensions(width, height)?;
            let components = usize::from(frame[5]);
            if !matches!(components, 1 | 3 | 4) || frame.len() != 6 + components * 3 {
                return Err("invalid JPEG component count".into());
            }
            let mut blocks = 0;
            for component in frame[6..].as_chunks::<3>().0 {
                let h = component[1] >> 4;
                let v = component[1] & 15;
                if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                    return Err("invalid JPEG sampling factors".into());
                }
                blocks += usize::from(h) * usize::from(v);
            }
            if blocks > 10 {
                return Err("JPEG exceeds the sampling-block limit".into());
            }
        }
        at = end;
        if marker == 0xda {
            scans += 1;
            if scans > MAX_SCANS {
                return Err("JPEG exceeds the 64-scan limit".into());
            }
            // This only locates entropy boundaries; the codec validates Huffman
            // data and reconstructs pixels. FF00 is stuffing, FFD0..D7 restart.
            loop {
                check_time(started, budget)?;
                let offset = bytes
                    .get(at..)
                    .and_then(|b| b.iter().position(|b| *b == 0xff))
                    .ok_or("JPEG is missing its final EOI marker")?;
                at += offset;
                let mut next = at + 1;
                while bytes.get(next) == Some(&0xff) {
                    next += 1;
                }
                match bytes.get(next) {
                    Some(0x00 | 0xd0..=0xd7) => at = next + 1,
                    Some(_) => break,
                    None => return Err("truncated JPEG entropy data".into()),
                }
            }
        }
    }
    Err("JPEG exceeds the marker-count limit".into())
}

fn validate_jpeg(bytes: &[u8], started: Instant, budget: Duration) -> Result<(), String> {
    use zune_jpeg::zune_core::options::DecoderOptions;
    jpeg_container(bytes, started, budget)?;
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .set_use_unsafe(false)
        .set_max_width(MAX_AXIS)
        .set_max_height(MAX_AXIS)
        .jpeg_set_max_scans(MAX_SCANS);
    let mut decoder =
        zune_jpeg::JpegDecoder::new_with_options(TimedCursor::new(bytes, started, budget), options);
    decoder
        .decode_headers()
        .map_err(|e| format!("invalid JPEG: {e}"))?;
    let size = decoder
        .output_buffer_size()
        .filter(|n| *n <= MAX_OUTPUT)
        .ok_or("JPEG exceeds the decoded-output budget")?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| "cannot allocate JPEG validation buffer")?;
    output.resize(size, 0);
    decoder
        .decode_into(&mut output)
        .map_err(|e| format!("invalid JPEG: {e}"))
}

struct TimedCursor<'a> {
    cursor: Cursor<&'a [u8]>,
    started: Instant,
    budget: Duration,
}

impl<'a> TimedCursor<'a> {
    fn new(bytes: &'a [u8], started: Instant, budget: Duration) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            started,
            budget,
        }
    }
    fn check(&self) -> io::Result<()> {
        check_time(self.started, self.budget)
            .map_err(|e| io::Error::new(io::ErrorKind::TimedOut, e))
    }
}

impl Read for TimedCursor<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        self.cursor.read(buffer)
    }
}

impl BufRead for TimedCursor<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.check()?;
        self.cursor.fill_buf()
    }
    fn consume(&mut self, amount: usize) {
        self.cursor.consume(amount);
    }
}

impl Seek for TimedCursor<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.cursor.seek(position)
    }
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
