//! Exact integer PCM from the same admitted APE reader used by playback.
use super::{Error, Info, Result, ape};
use std::{
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
};

/// Read-only, bounded APE PCM16 reader for file-handle based clients.
///
/// Admits the shared modern 3.98–4.10 profile, including prefixes up to 1 MiB.
/// Header admission does not decode audio. Blocks contain original interleaved
/// signed little-endian samples, without f32 conversion or processing effects.
/// Any failed operation retires this reader; open a new visit after failure.
pub struct ApePcm16Decoder {
    inner: ape::Decoder,
    skip: usize,
    failed: bool,
}
impl ApePcm16Decoder {
    /// Consume an already opened regular file; never reopen a source pathname.
    /// Cloned OS file handles share a cursor and require external serialization.
    pub fn open(file: File) -> Result<Self> {
        let inner = catch_unwind(AssertUnwindSafe(|| ape::Decoder::open(file)))
            .map_err(|_| Error("APE source probe panicked".into()))??;
        if inner.info.bits != Some(16) {
            return Err(Error("APE PCM16 requires original 16-bit samples".into()));
        }
        Ok(Self {
            inner,
            skip: 0,
            failed: false,
        })
    }
    /// Actual properties retained from the same file handle.
    pub fn info(&self) -> Info {
        self.inner.info
    }
    /// Move to an exact PCM frame, including EOF, using the protected seek table.
    /// Only the containing compressed frame is decoded on the next read.
    pub fn seek(&mut self, frame: u64) -> Result<()> {
        self.visit(|this| {
            let total = this
                .inner
                .info
                .frames
                .ok_or_else(|| Error("unknown APE length".into()))?;
            if frame > total {
                return Err(Error("APE seek exceeds source length".into()));
            }
            this.skip = this.inner.seek(frame)?;
            Ok(())
        })
    }
    /// Decode one checked frame, at most 1,048,576 scalar samples (2 MiB).
    /// Returns `None` at exact EOF; damaged/incomplete frames are errors.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        self.visit(|this| {
            let mut bytes = this.inner.next_bytes()?;
            if bytes.is_empty() {
                return Ok(None);
            }
            let skip = this.skip * this.inner.info.channels * 2;
            if skip > bytes.len() {
                return Err(Error("invalid APE seek pre-roll".into()));
            }
            bytes.drain(..skip);
            this.skip = 0;
            Ok(Some(bytes))
        })
    }
    fn visit<T>(&mut self, run: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.failed {
            return Err(Error("APE decoder is retired".into()));
        }
        let result = catch_unwind(AssertUnwindSafe(|| run(self)))
            .map_err(|_| Error("APE codec panicked".into()))
            .and_then(|value| value);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

#[cfg(test)]
#[path = "ape_pcm16_tests.rs"]
mod tests;
