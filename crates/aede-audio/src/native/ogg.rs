//! Bounded Ogg packet reader, from RFC 3533 (page framing and CRC polynomial).
use super::{Error, Result};
use std::{collections::VecDeque, fs::File, io::Read};
pub(super) struct Packet {
    pub data: Vec<u8>,
    pub granule: Option<u64>,
    pub eos: bool,
}
pub(super) struct Reader {
    file: File,
    serial: Option<u32>,
    sequence: u32,
    partial: Vec<u8>,
    queue: VecDeque<Packet>,
    ended: bool,
    pages: u32,
}
impl Reader {
    pub fn new(file: File) -> Self {
        Self {
            file,
            serial: None,
            sequence: 0,
            partial: Vec::new(),
            queue: VecDeque::new(),
            ended: false,
            pages: 0,
        }
    }
    pub fn next(&mut self, current: &mut impl FnMut() -> Result<()>) -> Result<Option<Packet>> {
        loop {
            current()?;
            if let Some(packet) = self.queue.pop_front() {
                return Ok(Some(packet));
            }
            if self.ended {
                return Ok(None);
            }
            self.page()?;
            current()?;
        }
    }
    fn page(&mut self) -> Result<()> {
        if self.pages >= 65536 {
            return Err(Error("Ogg page budget exceeded".into()));
        }
        self.pages += 1;
        let mut header = [0; 27];
        self.file
            .read_exact(&mut header)
            .map_err(|e| Error(format!("incomplete Ogg stream: {e}")))?;
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !7 != 0 {
            return Err(Error("invalid Ogg page header".into()));
        }
        let serial = word(&header, 14)?;
        let sequence = word(&header, 18)?;
        let flags = header[5];
        if self.serial.is_none() {
            if flags != 2 || sequence != 0 {
                return Err(Error(
                    "Ogg stream must begin with a complete BOS page".into(),
                ));
            }
            self.serial = Some(serial);
        } else if self.serial != Some(serial) || flags & 2 != 0 {
            return Err(Error(
                "multiplexed/chained Speex is outside the native profile".into(),
            ));
        }
        if sequence != self.sequence || (flags & 1 != 0) != !self.partial.is_empty() {
            return Err(Error("discontinuous Ogg packet sequence".into()));
        }
        self.sequence = self.sequence.wrapping_add(1);
        let mut laces = vec![0; header[26] as usize];
        self.file.read_exact(&mut laces)?;
        let size: usize = laces.iter().map(|v| usize::from(*v)).sum();
        let mut body = vec![0; size];
        self.file.read_exact(&mut body)?;
        let expected = word(&header, 22)?;
        if crc(header
            .iter()
            .enumerate()
            .map(|(i, b)| if (22..26).contains(&i) { 0 } else { *b })
            .chain(laces.iter().copied())
            .chain(body.iter().copied()))
            != expected
        {
            return Err(Error("Ogg page CRC mismatch".into()));
        }
        let mut at = 0;
        for &lace in &laces {
            let end = at + usize::from(lace);
            if self.partial.len() + usize::from(lace) > 1024 * 1024 {
                return Err(Error("Ogg packet exceeds native budget".into()));
            }
            self.partial.extend_from_slice(&body[at..end]);
            at = end;
            if lace < 255 {
                self.queue.push_back(Packet {
                    data: std::mem::take(&mut self.partial),
                    granule: None,
                    eos: false,
                });
            }
        }
        let granule = u64::from_le_bytes(
            header[6..14]
                .try_into()
                .map_err(|_| Error("invalid Ogg granule".into()))?,
        );
        if let Some(last) = self.queue.back_mut() {
            last.granule = (granule != u64::MAX).then_some(granule);
            last.eos = flags & 4 != 0;
        }
        if flags & 4 != 0 {
            if !self.partial.is_empty() || self.queue.is_empty() || granule == u64::MAX {
                return Err(Error("incomplete Ogg end page".into()));
            }
            let mut trailing = [0];
            if self.file.read(&mut trailing)? != 0 {
                return Err(Error(
                    "trailing/chained Ogg data is outside the native profile".into(),
                ));
            }
            self.ended = true;
        }
        Ok(())
    }
}
fn word(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| Error("truncated Ogg field".into()))?
            .try_into()
            .map_err(|_| Error("invalid Ogg field".into()))?,
    ))
}
fn crc(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut value = 0u32;
    for b in bytes {
        value ^= u32::from(b) << 24;
        for _ in 0..8 {
            value = if value & 0x80000000 != 0 {
                (value << 1) ^ 0x04c11db7
            } else {
                value << 1
            };
        }
    }
    value
}
