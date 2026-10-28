//! Ogg/Speex coded PCM, with standard libspeex enhancement, without application DSP.
use super::{Error, Info, Kind, Result, ffi, ogg};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
pub(super) fn recognize(file: &mut File) -> Result<bool> {
    let mut head = [0; 27];
    file.seek(SeekFrom::Start(0))?;
    let read = file.read(&mut head)?;
    let mut marker = [0; 8];
    let matches = if read == 27 && head[26] != 0 {
        file.seek(SeekFrom::Start(27 + u64::from(head[26])))?;
        file.read(&mut marker)? == 8 && &marker == b"Speex   "
    } else {
        false
    };
    file.seek(SeekFrom::Start(0))?;
    Ok(matches)
}
fn word(bytes: &[u8], at: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| Error("truncated Speex header".into()))?
            .try_into()
            .map_err(|_| Error("invalid Speex header".into()))?,
    ))
}
pub(super) struct Decoder {
    reader: ogg::Reader,
    codec: ffi::Speex,
    pub info: Info,
    frames_per_packet: usize,
    decoded: u64,
    last_granule: u64,
    ended: bool,
}
impl Decoder {
    pub fn open(file: File) -> Result<Self> {
        let mut reader = ogg::Reader::new(file);
        let header = reader
            .next(&mut || Ok(()))?
            .ok_or_else(|| Error("missing Speex header".into()))?;
        let h = &header.data;
        let rate = word(h, 36)?;
        let mode = word(h, 40)?;
        let channels = word(h, 48)?;
        let frames = word(h, 64)?;
        let extra = word(h, 68)?;
        if h.len() < 80
            || !h.starts_with(b"Speex   ")
            || word(h, 28)? != 1
            || word(h, 32)? as usize != h.len()
            || !(0..=2).contains(&mode)
            || !(1..=2).contains(&channels)
            || !(1..=16).contains(&frames)
            || !(0..=16).contains(&extra)
            || rate != 8000 * (1 << mode)
            || header.eos
            || header.granule != Some(0)
        {
            return Err(Error("unsupported Speex identification header".into()));
        }
        let codec = ffi::Speex::open(mode, channels as usize)?;
        if word(h, 56)? as usize != codec.size || word(h, 44)? != codec.version {
            return Err(Error("Speex codec/header mismatch".into()));
        }
        for _ in 0..=extra {
            let metadata = reader
                .next(&mut || Ok(()))?
                .ok_or_else(|| Error("missing Speex metadata header".into()))?;
            if metadata.eos {
                return Err(Error("Speex stream ends in its metadata".into()));
            }
        }
        let info = Info {
            kind: Kind::Speex,
            sample_rate: rate as u32,
            channels: channels as usize,
            bits: None,
            frames: None,
        };
        Ok(Self {
            reader,
            codec,
            info,
            frames_per_packet: frames as usize,
            decoded: 0,
            last_granule: 0,
            ended: false,
        })
    }
    pub fn next(&mut self, current: &mut impl FnMut() -> Result<()>) -> Result<Vec<f32>> {
        let Some(packet) = self.reader.next(current)? else {
            if !self.ended {
                return Err(Error("Speex is missing its end packet".into()));
            }
            return Ok(Vec::new());
        };
        current()?;
        let pcm = self
            .codec
            .decode(&packet.data, self.frames_per_packet, packet.eos)?;
        self.decoded = self
            .decoded
            .checked_add((pcm.len() / self.info.channels) as u64)
            .ok_or_else(|| Error("Speex frame count overflow".into()))?;
        if let Some(granule) = packet.granule {
            if granule < self.last_granule || granule > self.decoded {
                return Err(Error("invalid Speex granule timeline".into()));
            }
            self.last_granule = granule;
        }
        self.ended = packet.eos;
        // Speex granules do not alone establish the original priming/tail
        // window. Preserve the reference decoder's coded PCM; never guess it.
        Ok(pcm)
    }
}
