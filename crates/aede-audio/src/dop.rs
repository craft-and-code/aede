//! DoP 1.1 bit transport. The carrier contains DSD bits, never decoded PCM.
//! Frame layout follows <https://dsd-guide.com/dop-open-standard> (figure 1).
#[cfg(all(feature = "dop-alsa", target_os = "linux"))]
#[path = "dop/alsa.rs"]
mod alsa;
#[cfg(all(feature = "dop-output", target_os = "macos"))]
#[path = "dop/coreaudio/mod.rs"]
mod coreaudio;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/driver.rs"]
mod driver;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/player.rs"]
pub mod output;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/platform.rs"]
mod platform;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/push.rs"]
mod push;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/render.rs"]
mod render;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/state.rs"]
mod state;
#[cfg(all(feature = "dop-output", target_os = "windows"))]
#[path = "dop/wasapi.rs"]
mod wasapi;
#[cfg(all(
    feature = "dop-output",
    any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", feature = "dop-alsa"),
        test
    )
))]
#[path = "dop/worker.rs"]
mod worker;
use std::{fs::File, path::Path};

use crate::dsd::{DsdError, Info, RawSource};

/// Sixteen chronological original DSD bits, oldest at bit 15 (DoP slot t0).
/// This type deliberately has no float/sample-conversion or volume operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payload(pub u16);

#[cfg(feature = "queue")]
impl crate::queue::QueueSample for Payload {
    fn valid(self) -> bool {
        true
    }
}

/// Source and link clocks are distinct; one carrier frame holds 16 DSD bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    source_rate: u32,
    channels: usize,
}
impl Format {
    /// Admit the initial raw DSD64/128 mono/stereo transport profile.
    pub fn new(source_rate: u32, channels: usize) -> Result<Self, DsdError> {
        if !matches!(source_rate, 2_822_400 | 5_644_800) || !matches!(channels, 1 | 2) {
            return Err(DsdError("DoP requires raw DSD64/128 mono/stereo".into()));
        }
        Ok(Self {
            source_rate,
            channels,
        })
    }
    /// Genuine DSD one-bit clock, excluding transport framing.
    pub fn source_rate(self) -> u32 {
        self.source_rate
    }
    /// Exact link frame rate: 176.4 kHz for DSD64, 352.8 kHz for DSD128.
    pub fn carrier_rate(self) -> u32 {
        self.source_rate / 16
    }
    /// Source channel count and ordering, without mixing or duplication.
    pub fn channels(self) -> usize {
        self.channels
    }
}

/// A bounded run of source words. The last word may contain declared silence
/// padding; `valid_samples` retains the exact original one-bit length.
pub struct Block {
    /// Carrier-frame position in the original source, before link pre-roll.
    pub first_frame: u64,
    /// Original DSD one-bit samples per channel, excluding any tail padding.
    pub valid_samples: u64,
    /// Canonical channel-interleaved words; marker bytes are added at output.
    pub payload: Vec<Payload>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: std::time::SystemTime,
}
impl Stamp {
    fn read(file: &File) -> Result<Self, DsdError> {
        let metadata = file.metadata()?;
        Ok(Self {
            length: metadata.len(),
            modified: metadata.modified()?,
        })
    }
}

/// Bounded original-bit reader with no PCM decoder, filter or processing path.
pub struct Reader {
    source: RawSource,
    format: Format,
    failed: bool,
    check: File,
    stamp: Stamp,
}
impl Reader {
    /// Open and retain a regular local source, selected by its content.
    pub fn open(path: &Path) -> Result<Self, DsdError> {
        Self::from_file(File::open(path)?)
    }
    /// Reuse the same retained descriptor for source inspection and transport.
    pub fn from_file(file: File) -> Result<Self, DsdError> {
        let stamp = Stamp::read(&file)?;
        let check = file.try_clone()?;
        let source = RawSource::from_file(file)?;
        let info = source.info();
        let reader = Self {
            format: Format::new(info.sample_rate, info.channels)?,
            source,
            failed: false,
            check,
            stamp,
        };
        reader.unchanged()?;
        Ok(reader)
    }
    /// Original container facts, independent of carrier framing.
    pub fn info(&self) -> Info {
        self.source.info()
    }
    /// Exact source/link association admitted before opening a device.
    pub fn format(&self) -> Format {
        self.format
    }
    /// Source carrier frames, including at most 15 final silence bits per channel.
    pub fn frames(&self) -> u64 {
        self.info().samples.div_ceil(16)
    }
    /// Seek in carrier frames on the original retained file; marker phase lives
    /// in the output encoder and is not reset by a source seek.
    pub fn seek(&mut self, frame: u64) -> Result<(), DsdError> {
        if self.failed {
            return Err(DsdError("DoP source is retired".into()));
        }
        if frame > self.frames() {
            return Err(DsdError("DoP seek exceeds source length".into()));
        }
        if let Err(error) = self.unchanged() {
            self.failed = true;
            return Err(error);
        }
        let sample = if frame == self.frames() {
            self.info().samples
        } else {
            frame
                .checked_mul(16)
                .ok_or_else(|| DsdError("DoP seek overflow".into()))?
        };
        self.source.seek(sample)
    }
    /// At most 2,048 words per channel. Checks admission before and after each
    /// source visit; an error permanently prevents publication or reset.
    pub fn next_chunk(
        &mut self,
        mut current: impl FnMut() -> Result<(), DsdError>,
    ) -> Result<Option<Block>, DsdError> {
        if self.failed {
            return Err(DsdError("DoP source is retired".into()));
        }
        let result = (|| {
            current()?;
            self.unchanged()?;
            let Some(block) = self.source.next_chunk(&mut current)? else {
                self.unchanged()?;
                return Ok(None);
            };
            let channels = self.format.channels();
            let mut payload =
                Vec::with_capacity(block.bytes.len().div_ceil(2 * channels) * channels);
            for (frame, bytes) in block.bytes.chunks(2 * channels).enumerate() {
                current()?;
                let valid = block
                    .valid_samples
                    .saturating_sub(frame as u64 * 16)
                    .min(16) as u8;
                for channel in 0..channels {
                    let first = padded(bytes.get(channel).copied(), valid.min(8));
                    let second = padded(
                        bytes.get(channels + channel).copied(),
                        valid.saturating_sub(8),
                    );
                    // Figure 1 puts oldest t0 at bit 15, whereas canonical raw
                    // DSD bytes put the oldest bit at bit 0. Reverse each byte
                    // and retain chronological byte order before LE storage.
                    payload.push(Payload(u16::from_be_bytes([
                        first.reverse_bits(),
                        second.reverse_bits(),
                    ])));
                }
            }
            current()?;
            self.unchanged()?;
            Ok(Some(Block {
                first_frame: block.first_sample / 16,
                valid_samples: block.valid_samples,
                payload,
            }))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn unchanged(&self) -> Result<(), DsdError> {
        if Stamp::read(&self.check)? != self.stamp {
            return Err(DsdError(
                "the retained DSD source changed during DoP playback".into(),
            ));
        }
        Ok(())
    }
}

fn padded(byte: Option<u8>, valid: u8) -> u8 {
    let mask = ((1u16 << valid) - 1) as u8;
    // Balanced DoP silence 0x69 (MSB first) is 0x96 in canonical LSB-first bytes.
    (byte.unwrap_or(0) & mask) | (0x96 & !mask)
}

/// Exact integer storage exposed by a validated exclusive device interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    /// Three little-endian bytes: later DSD byte, earlier DSD byte, marker.
    Packed24Le,
    /// Signed 24 valid low bits in a sign-extended 32-bit little-endian word.
    Low24In32Le,
    /// Signed 24 valid high bits in a 32-bit little-endian word; low byte zero.
    High24In32Le,
}
impl Carrier {
    /// Storage bytes per channel, excluding no marker or payload bits.
    pub fn bytes(self) -> usize {
        if self == Self::Packed24Le { 3 } else { 4 }
    }
}

/// Callback-local marker phase. Silence and pauses keep the same phase as music.
#[derive(Clone)]
pub struct Encoder {
    format: Format,
    carrier: Carrier,
    marker: u8,
}
impl Encoder {
    /// Start one new link with marker 0x05; retain it across callback boundaries.
    pub fn new(format: Format, carrier: Carrier) -> Self {
        Self {
            format,
            carrier,
            marker: 0x05,
        }
    }
    /// Exact output frame width; integer-only and suitable for preallocated I/O.
    pub fn frame_bytes(&self) -> usize {
        self.format.channels() * self.carrier.bytes()
    }
    /// Write one whole frame without allocation, conversion, gain or locking.
    /// Invalid lengths leave both output and marker phase unchanged.
    pub fn write_frame(&mut self, payload: &[Payload], output: &mut [u8]) -> Result<(), DsdError> {
        if payload.len() != self.format.channels() || output.len() != self.frame_bytes() {
            return Err(DsdError(
                "DoP requires one complete ordered output frame".into(),
            ));
        }
        for (sample, bytes) in payload
            .iter()
            .zip(output.chunks_exact_mut(self.carrier.bytes()))
        {
            let word = (u32::from(self.marker) << 16) | u32::from(sample.0);
            match self.carrier {
                Carrier::Packed24Le => bytes.copy_from_slice(&word.to_le_bytes()[..3]),
                Carrier::Low24In32Le => {
                    bytes.copy_from_slice(&(((word << 8) as i32) >> 8).to_le_bytes())
                }
                Carrier::High24In32Le => bytes.copy_from_slice(&(word << 8).to_le_bytes()),
            }
        }
        self.marker ^= 0xff;
        Ok(())
    }
    /// Emit balanced DSD silence with valid alternating markers, never PCM zero.
    pub fn write_silence(&mut self, output: &mut [u8]) -> Result<(), DsdError> {
        self.write_frame(&[Payload(0x6969); 2][..self.format.channels()], output)
    }
    #[cfg(all(
        feature = "dop-output",
        any(
            target_os = "windows",
            all(target_os = "linux", feature = "dop-alsa"),
            test
        )
    ))]
    fn advance(&mut self, accepted: usize) {
        if !accepted.is_multiple_of(2) {
            self.marker ^= 0xff;
        }
    }
}

#[cfg(test)]
#[path = "dop_tests.rs"]
mod tests;
