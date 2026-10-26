//! Progressive native DSF/DFF decoding, without an external process.
//!
//! Based on Sony DSF 1.01 and Philips DSDIFF 1.5. Only uncompressed DSD64/128
//! mono/stereo is admitted. Conversion produces PCM, never native DSD/DoP.
use std::{
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};
mod container;
mod filter;
use filter::{BitFilter, ListeningFilter};

/// Invalid, unsupported, cancelled or unreadable DSD input.
#[derive(Debug)]
pub struct DsdError(pub String);
impl fmt::Display for DsdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DsdError {}
impl From<std::io::Error> for DsdError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
/// Exact source properties, distinct from the derived PCM rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    /// Container name from its signature.
    pub container: &'static str,
    /// One-bit samples per channel per second.
    pub sample_rate: u32,
    /// Source channel count; native PCM admission is checked separately.
    pub channels: usize,
    /// Valid one-bit samples per channel, excluding DSF block padding.
    pub samples: u64,
    /// Optional ID3 header offset, bounded by the enclosing container.
    pub metadata_offset: Option<u64>,
    /// Available bytes starting at that ID3 header.
    pub metadata_bytes: u64,
}
/// A measurement keeps ultrasound; listening removes shaped ultrasonic noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Wideband PCM at source rate / 8. No playback processing.
    Measurement,
    /// Low-pass filtered 88.2 kHz PCM, before optional device adaptation.
    Listening,
}

/// Whether the shared PCM profiles admit this uncompressed DSD rate/channel pair.
/// This is a capability check, not validation of a source's container or payload.
pub const fn supports_pcm_profile(sample_rate: u32, channels: usize) -> bool {
    matches!(sample_rate, 2_822_400 | 5_644_800) && matches!(channels, 1 | 2)
}

fn source_length(file: &File) -> Result<u64, DsdError> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(DsdError("DSD source is not a regular file".into()));
    }
    Ok(metadata.len())
}
/// Inspect a source admitted by the native PCM profile on an existing descriptor.
pub fn inspect(file: &mut File) -> Result<Info, DsdError> {
    let length = source_length(file)?;
    Ok(container::parse(file, length)?.info)
}
/// Validate DSF structure and audio/metadata extents without admitting a decoder.
/// This also supports structural inspection of higher rates and DSF multichannel
/// layouts used by explicit compatibility callers; native PCM remains limited
/// by [`supports_pcm_profile`]. No audio or tag bytes are modified.
pub fn inspect_dsf(file: &mut File) -> Result<Info, DsdError> {
    let length = source_length(file)?;
    Ok(container::dsf(file, length)?.info)
}
/// Read a DSDIFF 1.x form prefix and its required first format-version chunk.
/// Return the declared form end, leaving the reader after its 32-byte prefix.
/// Later chunks and the actual file extent remain the caller's responsibility.
/// Minor additions remain readable; other main versions and reserved flags fail.
pub fn read_dff_header(reader: &mut impl Read) -> Result<u64, DsdError> {
    let mut header = [0; 32];
    reader.read_exact(&mut header)?;
    let number = |at| u64::from_be_bytes(std::array::from_fn(|i| header[at + i]));
    if &header[..4] != b"FRM8"
        || &header[12..16] != b"DSD "
        || &header[16..20] != b"FVER"
        || number(20) != 4
    {
        return Err(DsdError("invalid DFF form/version header".into()));
    }
    // DSDIFF 1.5 §3.1: main version, minor additions, then two zero reserved bytes.
    if header[28] != 1 || header[30..32] != [0, 0] {
        return Err(DsdError("unsupported DFF version".into()));
    }
    number(4)
        .checked_add(12)
        .filter(|end| *end >= 32)
        .ok_or_else(|| DsdError("invalid DFF form size".into()))
}
/// Native bounded reader. Drop retires it immediately; no process/thread is owned.
pub struct Decoder {
    file: File,
    layout: container::Layout,
    buffer: Vec<u8>,
    at: usize,
    loaded: bool,
    buffered_frames: usize,
    byte_index: u64,
    filters: Vec<BitFilter>,
    primed: bool,
    wide_emitted: u64,
    listening: Option<ListeningFilter>,
    emitted: u64,
    target: u64,
    failed: bool,
}
impl Decoder {
    /// Open only a regular local file. Format is selected by signature.
    pub fn open(path: &Path, profile: Profile) -> Result<Self, DsdError> {
        if !fs::metadata(path)?.is_file() {
            return Err(DsdError("DSD source is not a regular file".into()));
        }
        Self::from_file(File::open(path)?, profile)
    }
    /// Use a previously opened source; container parsing and audio share that descriptor.
    pub fn from_file(mut file: File, profile: Profile) -> Result<Self, DsdError> {
        let length = source_length(&file)?;
        let layout = container::parse(&mut file, length)?;
        file.seek(SeekFrom::Start(layout.offset))?;
        let channels = layout.info.channels;
        let wide_rate = layout.info.sample_rate / 8;
        let listening =
            (profile == Profile::Listening).then(|| ListeningFilter::new(wide_rate, channels));
        let divisor = if listening.is_some() {
            layout.info.sample_rate / 88_200
        } else {
            8
        };
        let target = layout.info.samples.div_ceil(u64::from(divisor));
        Ok(Self {
            buffer: vec![0; 4096 * channels],
            file,
            layout,
            at: 0,
            loaded: false,
            buffered_frames: 0,
            byte_index: 0,
            filters: (0..channels).map(|_| BitFilter::new()).collect(),
            primed: false,
            wide_emitted: 0,
            listening,
            emitted: 0,
            target,
            failed: false,
        })
    }
    /// Genuine one-bit source properties, not derived output precision.
    pub fn info(&self) -> Info {
        self.layout.info
    }
    /// Derived PCM rate; this is not the original DSD clock.
    pub fn sample_rate(&self) -> u32 {
        if self.listening.is_some() {
            88_200
        } else {
            self.layout.info.sample_rate / 8
        }
    }
    fn byte_frame(&mut self, output: &mut [f32]) -> Result<(), DsdError> {
        let byte_count = self.layout.info.samples.div_ceil(8);
        let real = self.byte_index < byte_count;
        if real && (!self.loaded || self.at == self.buffered_frames) {
            self.buffered_frames = if self.layout.block == 1 {
                (byte_count - self.byte_index).min(4096) as usize
            } else {
                4096
            };
            self.file
                .read_exact(&mut self.buffer[..self.buffered_frames * self.layout.info.channels])?;
            self.at = 0;
            self.loaded = true;
        }
        for (channel, (filter, sample)) in self.filters.iter_mut().zip(output).enumerate() {
            let mut byte = if real {
                self.buffer[if self.layout.block == 1 {
                    self.at * self.layout.info.channels + channel
                } else {
                    channel * self.layout.block + self.at
                }]
            } else {
                0x69
            };
            if real && !self.layout.lsb {
                byte = byte.reverse_bits();
            }
            if real
                && self.byte_index + 1 == byte_count
                && !self.layout.info.samples.is_multiple_of(8)
            {
                let mask = (1u16 << (self.layout.info.samples % 8)) as u8 - 1;
                byte = (byte & mask) | (0x69 & !mask);
            }
            *sample = filter.push(byte);
        }
        if real {
            self.at += 1;
        }
        self.byte_index += 1;
        Ok(())
    }
    fn wide_frame(&mut self, frame: &mut [f32]) -> Result<bool, DsdError> {
        if !self.primed {
            for _ in 0..16 {
                self.byte_frame(frame)?;
            }
            self.primed = true;
        }
        if self.wide_emitted == self.layout.info.samples.div_ceil(8) {
            return Ok(false);
        }
        self.byte_frame(frame)?;
        self.wide_emitted += 1;
        Ok(true)
    }
    /// At most 2,048 complete finite PCM frames. Admission is checked before/after
    /// every input/filter step; an already running filesystem read cannot be interrupted.
    /// Any read/admission error permanently retires this reader.
    pub fn next_chunk(
        &mut self,
        mut current: impl FnMut() -> Result<(), DsdError>,
    ) -> Result<Option<Vec<f32>>, DsdError> {
        if self.failed {
            return Err(DsdError("DSD decoder is retired".into()));
        }
        let result = self.read_chunk(&mut current);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn read_chunk(
        &mut self,
        current: &mut impl FnMut() -> Result<(), DsdError>,
    ) -> Result<Option<Vec<f32>>, DsdError> {
        current()?;
        if self.emitted == self.target {
            return Ok(None);
        }
        let channels = self.layout.info.channels;
        let mut output = Vec::with_capacity(2048 * channels);
        let mut wide = [0.0; 2];
        let mut audible = [0.0; 2];
        while output.len() < 2048 * channels && self.emitted < self.target {
            current()?;
            if !self.wide_frame(&mut wide[..channels])? {
                wide.fill(0.0);
            }
            current()?;
            let frame = if let Some(filter) = self.listening.as_mut() {
                if !filter.push(&wide[..channels], &mut audible[..channels]) {
                    continue;
                }
                &audible[..channels]
            } else {
                &wide[..channels]
            };
            output.extend_from_slice(frame);
            self.emitted += 1;
        }
        current()?;
        Ok(Some(output))
    }
}
#[cfg(test)]
#[path = "../dsd_tests.rs"]
mod tests;
