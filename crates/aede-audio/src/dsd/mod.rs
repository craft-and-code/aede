//! Progressive native DSF/DFF decoding, without an external process.
//!
//! Based on Sony DSF 1.01 and Philips DSDIFF 1.5. Only uncompressed DSD64/128
//! mono/stereo is admitted for PCM. Raw bit transport is kept separate.
use std::{
    fmt,
    fs::{self, File},
    io::Read,
    path::Path,
};
mod container;
mod filter;
mod source;
use filter::{BitFilter, ListeningFilter};
pub use source::{RawBlock, RawSource};

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
/// Output clock of the native filtered listening profile, before device adaptation.
pub const LISTENING_SAMPLE_RATE: u32 = 88_200;

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
/// Inspect validated uncompressed DSF/DFF source properties for catalogue use.
/// Unlike [`inspect`], higher clocks and structurally admitted DSF multichannel
/// layouts are retained without enabling PCM decoding. Unsupported compression
/// and unknown DFF channel orders remain explicit errors.
pub fn inspect_source(file: &mut File) -> Result<Info, DsdError> {
    let length = source_length(file)?;
    Ok(container::inspect(file, length)?.info)
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
    source: RawSource,
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
    pub fn from_file(file: File, profile: Profile) -> Result<Self, DsdError> {
        let source = RawSource::from_file(file)?;
        let info = source.info();
        if !supports_pcm_profile(info.sample_rate, info.channels) {
            return Err(DsdError(
                "native DSD supports DSD64/128 mono/stereo only".into(),
            ));
        }
        let channels = info.channels;
        let wide_rate = info.sample_rate / 8;
        let listening =
            (profile == Profile::Listening).then(|| ListeningFilter::new(wide_rate, channels));
        let divisor = if listening.is_some() {
            info.sample_rate / LISTENING_SAMPLE_RATE
        } else {
            8
        };
        let target = info.samples.div_ceil(u64::from(divisor));
        Ok(Self {
            source,
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
        self.source.info()
    }
    /// Derived PCM rate; this is not the original DSD clock.
    pub fn sample_rate(&self) -> u32 {
        if self.listening.is_some() {
            LISTENING_SAMPLE_RATE
        } else {
            self.info().sample_rate / 8
        }
    }
    fn byte_frame(&mut self, output: &mut [f32]) -> Result<(), DsdError> {
        let frame = self.source.next_byte_frame()?;
        for (channel, (filter, sample)) in self.filters.iter_mut().zip(output).enumerate() {
            // Keep the existing PCM edge convention. Raw transport retains
            // valid-bit counts and chooses its own link-level silence padding.
            let byte = if let Some(frame) = &frame {
                let mask = ((1u16 << frame.valid_bits) - 1) as u8;
                frame.bytes[channel] | (0x69 & !mask)
            } else {
                0x69
            };
            *sample = filter.push(byte);
        }
        Ok(())
    }
    fn wide_frame(&mut self, frame: &mut [f32]) -> Result<bool, DsdError> {
        if !self.primed {
            for _ in 0..16 {
                self.byte_frame(frame)?;
            }
            self.primed = true;
        }
        if self.wide_emitted == self.info().samples.div_ceil(8) {
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
        let channels = self.info().channels;
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

#[cfg(test)]
#[path = "../dsd_test_fixtures.rs"]
pub(crate) mod test_fixtures;
