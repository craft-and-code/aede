//! Frame-wise APE adapter. Header/seek budgets precede dependency allocation.
use super::{Error, Info, Kind, Result};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
};
pub(super) struct Decoder {
    inner: ape_decoder::ApeDecoder<BufReader<Source>>,
    pub info: Info,
    index: u32,
    count: u32,
    blocks: u32,
    last: u32,
    bits: u32,
}
// The dependency derives its last compressed frame from SeekFrom::End rather
// than the descriptor's audio extent. Hide trailing tags from that calculation
// while retaining the declared terminating data it subtracts from the end.
struct Source {
    file: File,
    length: u64,
}
impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let position = self.file.stream_position()?;
        let count = (bytes.len() as u64).min(self.length.saturating_sub(position)) as usize;
        self.file.read(&mut bytes[..count])
    }
}
impl Seek for Source {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => {
                i128::from(self.file.stream_position()?) + i128::from(offset)
            }
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
        };
        if position < 0 || position > i128::from(self.length) {
            return Err(std::io::Error::other(
                "APE seek outside declared source extent",
            ));
        }
        self.file.seek(SeekFrom::Start(position as u64))
    }
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
    pub fn open(mut file: File) -> Result<Self> {
        let length = file.metadata()?.len();
        let mut descriptor = [0; 52];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut descriptor)?;
        let version = half(&descriptor, 4)?;
        let d = u64::from(word(&descriptor, 8)?);
        let h = u64::from(word(&descriptor, 12)?);
        let seek = u64::from(word(&descriptor, 16)?);
        let wrapper = u64::from(word(&descriptor, 20)?);
        let data = u64::from(word(&descriptor, 24)?) + (u64::from(word(&descriptor, 28)?) << 32);
        let ending = u64::from(word(&descriptor, 32)?);
        if &descriptor[..4] != b"MAC "
            || !(3980..=4100).contains(&version)
            || !(52..=4096).contains(&d)
            || !(24..=4096).contains(&h)
            || seek > 4 * 1024 * 1024
            || wrapper > 1024 * 1024
        {
            return Err(Error("APE descriptor exceeds the admitted profile".into()));
        }
        let start = d
            .checked_add(h)
            .and_then(|v| v.checked_add(seek))
            .and_then(|v| v.checked_add(wrapper))
            .ok_or_else(|| Error("APE header extent overflow".into()))?;
        let end = start
            .checked_add(data)
            .filter(|v| v.checked_add(ending).is_some_and(|n| n <= length))
            .ok_or_else(|| Error("APE data exceeds source".into()))?;
        file.seek(SeekFrom::Start(d))?;
        let mut header = [0; 24];
        file.read_exact(&mut header)?;
        let blocks = word(&header, 4)?;
        let last = word(&header, 8)?;
        let count = word(&header, 12)?;
        let bits = u32::from(half(&header, 16)?);
        let channels = usize::from(half(&header, 18)?);
        let rate = word(&header, 20)?;
        if count == 0
            || seek != u64::from(count) * 4
            || !(1..=2).contains(&channels)
            || ![8, 16, 24, 32].contains(&bits)
            || blocks == 0
            || u64::from(blocks) * channels as u64 > 1_048_576
            || last == 0
            || last > blocks
        {
            return Err(Error("APE frame layout exceeds native budget".into()));
        }
        let frames = u64::from(count - 1) * u64::from(blocks) + u64::from(last);
        let info = Info {
            kind: Kind::Ape,
            sample_rate: rate,
            channels,
            bits: Some(bits),
            frames: Some(frames),
        };
        info.validate()?;
        file.seek(SeekFrom::Start(d + h))?;
        let mut positions = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mut b = [0; 4];
            file.read_exact(&mut b)?;
            positions.push(u64::from(u32::from_le_bytes(b)));
        }
        for (i, &at) in positions.iter().enumerate() {
            let next = positions.get(i + 1).copied().unwrap_or(end);
            if at < start || next <= at || next > end || next - at > 16 * 1024 * 1024 {
                return Err(Error("invalid APE seek/frame extent".into()));
            }
        }
        file.seek(SeekFrom::Start(0))?;
        let source = Source {
            file,
            length: end + ending,
        };
        let inner = ape_decoder::ApeDecoder::new(BufReader::with_capacity(65536, source))
            .map_err(|e| Error(e.to_string()))?;
        if inner.info().is_big_endian
            || inner.info().is_floating_point
            || inner.info().is_signed_8bit
        {
            return Err(Error(
                "APE requires ordinary little-endian integer PCM".into(),
            ));
        }
        if inner.info().sample_rate != rate
            || inner.info().channels as usize != channels
            || u32::from(inner.info().bits_per_sample) != bits
            || inner.info().total_samples != frames
            || inner.total_frames() != count
        {
            return Err(Error(
                "APE properties disagree with protected header".into(),
            ));
        }
        Ok(Self {
            inner,
            info,
            index: 0,
            count,
            blocks,
            last,
            bits,
        })
    }
    pub fn next(&mut self) -> Result<Vec<f32>> {
        if self.index == self.count {
            return Ok(Vec::new());
        }
        let frames = if self.index + 1 == self.count {
            self.last
        } else {
            self.blocks
        };
        let bytes = self
            .inner
            .decode_frame(self.index)
            .map_err(|e| Error(e.to_string()))?;
        let width = self.bits as usize / 8;
        if bytes.len() != frames as usize * self.info.channels * width {
            return Err(Error("APE decoded frame has the wrong length".into()));
        }
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
        self.index += 1;
        Ok(pcm)
    }
}
