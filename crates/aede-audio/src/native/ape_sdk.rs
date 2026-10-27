//! Owned SDK boundary for legacy frames; callbacks retain the opened file view.
use super::super::io::{Abi, Reader};
use super::{Error, Info, Result, Source};
use std::{ffi::c_void, ptr::NonNull};
#[repr(C)]
#[derive(Default)]
struct Properties {
    rate: u32,
    channels: u32,
    bits: u32,
    version: u32,
    blocks: u32,
    last: u32,
    count: u32,
    frames: i64,
}
unsafe extern "C" {
    fn aede_ape_open(reader: *mut Abi, info: *mut Properties) -> *mut c_void;
    fn aede_ape_frame(
        handle: *mut c_void,
        start: u64,
        frames: u32,
        out: *mut u8,
        capacity: u32,
    ) -> i32;
    fn aede_ape_close(handle: *mut c_void);
}
pub(super) struct Codec {
    handle: NonNull<c_void>,
    reader: Reader,
}
// SAFETY: the synchronous SDK has one decoding thread and uniquely owned
// context/callback boxes. No pointer is shared; closing precedes Reader drop.
unsafe impl Send for Codec {}
impl Codec {
    pub fn open(
        file: Source,
        info: Info,
        version: u16,
        blocks: u32,
        last: u32,
        count: u32,
    ) -> Result<Self> {
        let length =
            i64::try_from(file.length).map_err(|_| Error("APE source too large".into()))?;
        let mut reader = Reader::from_stream(file, length)?;
        let mut actual = Properties::default();
        // SAFETY: the SDK borrows this stable ABI until close and writes only
        // this Properties value. Its C++ boundary catches every exception.
        let raw = unsafe { aede_ape_open(reader.abi.as_mut(), &mut actual) };
        let handle = NonNull::new(raw).ok_or_else(|| Error("legacy APE SDK open failed".into()))?;
        let codec = Self { handle, reader };
        codec.reader.checked()?;
        if actual.rate != info.sample_rate
            || actual.channels as usize != info.channels
            || Some(actual.bits) != info.bits
            || u32::from(version) != actual.version
            || actual.blocks != blocks
            || actual.last != last
            || actual.count != count
            || u64::try_from(actual.frames).ok() != info.frames
        {
            return Err(Error(
                "legacy APE properties disagree with protected header".into(),
            ));
        }
        Ok(codec)
    }
    pub fn frame(&mut self, start: u64, frames: u32, bytes: usize) -> Result<Vec<u8>> {
        if bytes > 4 * 1024 * 1024 {
            return Err(Error("legacy APE PCM exceeds frame budget".into()));
        }
        let mut pcm = vec![0; bytes];
        // SAFETY: the exclusively owned handle has a protected integer format.
        // The shim independently checks capacity/alignment/count and stages a
        // complete codec frame, including CRC, before this buffer is published.
        let count = unsafe {
            aede_ape_frame(
                self.handle.as_ptr(),
                start,
                frames,
                pcm.as_mut_ptr(),
                bytes as u32,
            )
        };
        self.reader.checked()?;
        if usize::try_from(count).ok() != Some(bytes) {
            return Err(Error("legacy APE frame is damaged or incomplete".into()));
        }
        Ok(pcm)
    }
}
impl Drop for Codec {
    fn drop(&mut self) {
        // SAFETY: one owner closes the handle before releasing callback storage.
        unsafe { aede_ape_close(self.handle.as_ptr()) };
    }
}
