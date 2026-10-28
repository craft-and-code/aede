//! Additional shared file decoders; original audio/tags are always read-only.
//! The foreign-code boundary is confined to ffi/io and native/*.{c,cpp}.
mod ape;
#[cfg(feature = "ape-pcm16")]
mod ape_pcm16;
mod ffi;
mod io;
mod musepack_seek;
mod ogg;
mod preflight;
mod speex;
mod wavpack_metadata;
pub use crate::decode::DecodeError as Error;
#[cfg(feature = "ape-pcm16")]
pub use ape_pcm16::ApePcm16Decoder;
/// Licence texts for the additional native decoders, retained for client credits.
pub const THIRD_PARTY_NOTICES: &str = include_str!("../../THIRD_PARTY_NOTICES.txt");
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
};
type Result<T> = std::result::Result<T, Error>;

/// Codec actually selected from the input signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// WavPack PCM (integer, float or hybrid base stream).
    WavPack,
    /// Musepack SV7/SV8.
    Musepack,
    /// Ogg/Speex.
    Speex,
    /// Monkey's Audio.
    Ape,
}
impl Kind {
    /// Human-readable codec identifier, independent of the filename.
    pub fn label(self) -> &'static str {
        match self {
            Self::WavPack => "WavPack",
            Self::Musepack => "Musepack",
            Self::Speex => "Speex",
            Self::Ape => "APE",
        }
    }
}
/// Decoded source properties; no playback gain or resampling is applied.
#[derive(Debug, Clone, Copy)]
pub struct Info {
    /// Recognized codec.
    pub kind: Kind,
    /// Actual output frames per second.
    pub sample_rate: u32,
    /// Mono/stereo interleave width.
    pub channels: usize,
    /// Original integer precision when known; lossy/float sources stay unknown.
    pub bits: Option<u32>,
    /// Exact complete-frame count when the container/codec supplies one.
    pub frames: Option<u64>,
}
impl Info {
    fn validate(&self) -> Result<()> {
        if self.sample_rate == 0
            || self.sample_rate > 768000
            || !(1..=2).contains(&self.channels)
            || self.bits.is_some_and(|n| n == 0 || n > 32)
            || self.frames == Some(0)
        {
            return Err(Error("unsupported native source properties".into()));
        }
        Ok(())
    }
}
enum Inner {
    C(ffi::Codec),
    Ape(Box<ape::Decoder>),
    Speex(Box<speex::Decoder>),
}
/// Progressive, finite PCM reader for the additional codecs. Failed/cancelled
/// visits are retired permanently; encoded frames and PCM have bounded budgets.
pub struct Decoder {
    inner: Inner,
    info: Info,
    frames: u64,
    failed: bool,
    finished: bool,
}
pub(crate) enum Probe {
    Native(Decoder),
    Other(File),
}
impl Decoder {
    /// Recognize an admitted regular file by content. Other codec families
    /// return None without running a codec or a subprocess.
    pub fn open(path: &Path) -> Result<Option<Self>> {
        if !fs::metadata(path)?.is_file() {
            return Err(Error("audio source is not a regular file".into()));
        }
        match Self::from_file(File::open(path)?)? {
            Probe::Native(value) => Ok(Some(value)),
            Probe::Other(_) => Ok(None),
        }
    }
    pub(crate) fn from_file(file: File) -> Result<Probe> {
        catch_unwind(AssertUnwindSafe(|| Self::probe(file)))
            .map_err(|_| Error("native source probe panicked".into()))?
    }
    fn probe(mut file: File) -> Result<Probe> {
        if !file.metadata()?.is_file() {
            return Err(Error("audio source is not a regular file".into()));
        }
        let mut magic = [0; 4];
        file.seek(SeekFrom::Start(0))?;
        let count = file.read(&mut magic)?;
        file.seek(SeekFrom::Start(0))?;
        let inner = match (&magic, count) {
            (b"wvpk", 4) => {
                preflight::wavpack(&mut file)?;
                Inner::C(ffi::Codec::open(file, Kind::WavPack)?)
            }
            (b"MPCK", 4) => {
                preflight::musepack(&mut file)?;
                Inner::C(ffi::Codec::open(file, Kind::Musepack)?)
            }
            (m, 4) if &m[..3] == b"MP+" => {
                preflight::musepack(&mut file)?;
                Inner::C(ffi::Codec::open(file, Kind::Musepack)?)
            }
            (m, 4) if m == b"MAC " || (&m[..3] == b"ID3" && ape::recognize(&mut file)?) => {
                Inner::Ape(Box::new(ape::Decoder::open(file)?))
            }
            (b"OggS", 4) if speex::recognize(&mut file)? => {
                Inner::Speex(Box::new(speex::Decoder::open(file)?))
            }
            _ => return Ok(Probe::Other(file)),
        };
        let info = match &inner {
            Inner::C(c) => c.info,
            Inner::Ape(c) => c.info,
            Inner::Speex(c) => c.info,
        };
        info.validate()?;
        Ok(Probe::Native(Self {
            inner,
            info,
            frames: 0,
            failed: false,
            finished: false,
        }))
    }
    /// Retained source properties from the same opened file.
    pub fn info(&self) -> Info {
        self.info
    }
    /// Decode one finite, interleaved block without processing effects.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>> {
        self.next_chunk_admitted(|| Ok(()))
    }
    /// Check ownership before/after a bounded codec call and before publication.
    /// A running foreign codec/OS read is not forcibly interrupted.
    pub fn next_chunk_admitted(
        &mut self,
        mut current: impl FnMut() -> Result<()>,
    ) -> Result<Option<Vec<f32>>> {
        if self.failed {
            return Err(Error("native decoder is retired".into()));
        }
        let result = (|| {
            current()?;
            if self.finished {
                return Ok(None);
            }
            let pcm = catch_unwind(AssertUnwindSafe(|| match &mut self.inner {
                Inner::C(c) => c.next(&mut current),
                Inner::Ape(c) => c.next(),
                Inner::Speex(c) => c.next(&mut current),
            }))
            .map_err(|_| Error("native codec panicked".into()))??;
            current()?;
            if !pcm.len().is_multiple_of(self.info.channels)
                || pcm.len() > 1_048_576
                || pcm.iter().any(|s| !s.is_finite())
            {
                return Err(Error("invalid native PCM block".into()));
            }
            if pcm.is_empty() {
                if self.info.frames.is_some_and(|n| n != self.frames) {
                    return Err(Error("incomplete native audio stream".into()));
                }
                self.finished = true;
                current()?;
                return Ok(None);
            }
            self.frames = self
                .frames
                .checked_add((pcm.len() / self.info.channels) as u64)
                .ok_or_else(|| Error("PCM count overflow".into()))?;
            if self.info.frames.is_some_and(|n| self.frames > n) {
                return Err(Error("native audio exceeds its declared length".into()));
            }
            current()?;
            Ok(Some(pcm))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ape_legacy_tests.rs"]
mod ape_legacy_tests;

#[cfg(test)]
#[path = "codec_conformance_tests.rs"]
mod codec_conformance_tests;
