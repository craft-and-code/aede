//! Frame-wise APE adapter. Header/seek budgets precede dependency allocation.
use super::{Error, Info, Kind, Result};
#[path = "ape_layout.rs"]
mod layout;
#[path = "ape_legacy.rs"]
mod legacy;
#[path = "ape_sdk.rs"]
mod sdk;
#[path = "ape_source.rs"]
mod source;
#[path = "ape_tags.rs"]
mod tags;
use source::Source;
use std::{
    fs::File,
    io::{BufReader, Seek, SeekFrom},
};
pub(super) struct Decoder {
    inner: Backend,
    pub info: Info,
    index: u32,
    count: u32,
    blocks: u32,
    last: u32,
    bits: u32,
}
enum Backend {
    Current(Box<ape_decoder::ApeDecoder<BufReader<Source>>>),
    Legacy(sdk::Codec),
}
fn word(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| Error("truncated APE word".into()))?
            .try_into()
            .map_err(|_| Error("invalid APE word".into()))?,
    ))
}
fn half(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or_else(|| Error("truncated APE field".into()))?
            .try_into()
            .map_err(|_| Error("invalid APE field".into()))?,
    ))
}
impl Decoder {
    #[cfg(feature = "ape-pcm16")]
    pub(super) fn seek(&mut self, frame: u64) -> Result<usize> {
        let total = self
            .info
            .frames
            .ok_or_else(|| Error("unknown APE length".into()))?;
        if frame > total {
            return Err(Error("APE seek exceeds source length".into()));
        }
        self.index = if frame == total {
            self.count
        } else {
            (frame / u64::from(self.blocks)) as u32
        };
        Ok(if frame == total {
            0
        } else {
            (frame % u64::from(self.blocks)) as usize
        })
    }
    pub fn open(file: File) -> Result<Self> {
        let mut file = Source::open(file)?;
        let metadata = layout::read(&mut file)?;
        let layout::Layout {
            version,
            info,
            blocks,
            last,
            count,
            ..
        } = metadata;
        file.seek(SeekFrom::Start(0))?;
        let inner = if version < 3990 {
            Backend::Legacy(sdk::Codec::open(file, info, version, blocks, last, count)?)
        } else {
            let current = ape_decoder::ApeDecoder::new(BufReader::with_capacity(65536, file))
                .map_err(|e| Error(e.to_string()))?;
            if current.info().is_big_endian
                || current.info().is_floating_point
                || current.info().is_signed_8bit
            {
                return Err(Error(
                    "APE requires ordinary little-endian integer PCM".into(),
                ));
            }
            if current.info().sample_rate != info.sample_rate
                || current.info().channels as usize != info.channels
                || Some(u32::from(current.info().bits_per_sample)) != info.bits
                || Some(current.info().total_samples) != info.frames
                || current.total_frames() != count
            {
                return Err(Error(
                    "APE properties disagree with protected header".into(),
                ));
            }
            Backend::Current(Box::new(current))
        };
        Ok(Self {
            inner,
            info,
            index: 0,
            count,
            blocks,
            last,
            bits: metadata.bits,
        })
    }
    pub(super) fn next_bytes(&mut self) -> Result<Vec<u8>> {
        if self.index == self.count {
            return Ok(Vec::new());
        }
        let frames = if self.index + 1 == self.count {
            self.last
        } else {
            self.blocks
        };
        let width = self.bits as usize / 8;
        let bytes = match &mut self.inner {
            Backend::Current(decoder) => decoder
                .decode_frame(self.index)
                .map_err(|e| Error(e.to_string()))?,
            Backend::Legacy(decoder) => decoder.frame(
                u64::from(self.index) * u64::from(self.blocks),
                frames,
                frames as usize * self.info.channels * width,
            )?,
        };
        if bytes.len() != frames as usize * self.info.channels * width {
            return Err(Error("APE decoded frame has the wrong length".into()));
        }
        self.index += 1;
        Ok(bytes)
    }
    pub fn next(&mut self) -> Result<Vec<f32>> {
        let bytes = self.next_bytes()?;
        let width = self.bits as usize / 8;
        let scale = 2.0f64.powi(self.bits as i32 - 1);
        let mut pcm = Vec::with_capacity(bytes.len() / width);
        for sample in bytes.chunks_exact(width) {
            let integer = match width {
                1 => i32::from(sample[0]) - 128,
                2 => i32::from(i16::from_le_bytes([sample[0], sample[1]])),
                3 => (i32::from_le_bytes([sample[0], sample[1], sample[2], 0]) << 8) >> 8,
                4 => i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]),
                _ => return Err(Error("invalid APE sample width".into())),
            };
            pcm.push((f64::from(integer) / scale) as f32);
        }
        Ok(pcm)
    }
}

// Automatic probing admits only a proper leading ID3 tag followed by padding
// and APE magic. It must not steal ID3-prefixed MP3/FLAC inputs by scanning their
// encoded payload. The explicit file-handle API keeps its broader prefix scan.
pub(super) fn recognize(file: &mut File) -> Result<bool> {
    use std::io::Read;
    file.seek(SeekFrom::Start(0))?;
    let result = (|| {
        let mut header = [0; 10];
        if file.read_exact(&mut header).is_err()
            || &header[..3] != b"ID3"
            || !(2..=4).contains(&header[3])
            || header[6..].iter().any(|b| b & 0x80 != 0)
        {
            return Ok(false);
        }
        let start = header[6..]
            .iter()
            .fold(0u64, |n, b| (n << 7) | u64::from(*b))
            + 10
            + if header[3] == 4 && header[5] & 0x10 != 0 {
                10
            } else {
                0
            };
        if start > 1024 * 1024 {
            return Ok(false);
        }
        file.seek(SeekFrom::Start(start))?;
        let mut input = BufReader::with_capacity(65536, &mut *file);
        let mut byte = [0];
        for _ in start..=1024 * 1024 {
            if input.read(&mut byte)? == 0 {
                return Ok(false);
            }
            if byte[0] != 0 {
                let mut rest = [0; 3];
                return Ok(byte[0] == b'M'
                    && input.read_exact(&mut rest).is_ok()
                    && &rest == b"AC ");
            }
        }
        Ok(false)
    })();
    file.seek(SeekFrom::Start(0))?;
    result
}
