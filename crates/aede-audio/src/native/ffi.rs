//! Approved foreign calls, owning handles and their synchronous buffer contracts.
use super::io::{Abi, Reader};
use super::{Error, Info, Kind, Result};
use std::{ffi::c_void, fs::File, ptr::NonNull, sync::Mutex};
// libmpcdec r475 lazily initializes legacy shared lookup tables. Serialize its
// complete API boundary, including creation/destruction, rather than race them.
static MPC: Mutex<()> = Mutex::new(());
#[repr(C)]
#[derive(Default)]
struct Properties {
    rate: u32,
    channels: u32,
    bits: u32,
    floating: u32,
    lossless: u32,
    frames: i64,
}
unsafe extern "C" {
    fn aede_wv_open(reader: *mut Abi, info: *mut Properties) -> *mut c_void;
    fn aede_wv_decode(handle: *mut c_void, out: *mut i32, frames: u32) -> i32;
    fn aede_wv_close(handle: *mut c_void);
    fn aede_mpc_open(reader: *mut Abi, info: *mut Properties) -> *mut c_void;
    fn aede_mpc_decode(handle: *mut c_void, out: *mut f32, capacity: u32) -> i32;
    fn aede_mpc_close(handle: *mut c_void);
    fn aede_spx_open(
        mode: i32,
        channels: i32,
        size: *mut i32,
        lookahead: *mut i32,
        version: *mut i32,
    ) -> *mut c_void;
    fn aede_spx_decode(
        handle: *mut c_void,
        packet: *const u8,
        bytes: i32,
        frames: i32,
        terminal: i32,
        out: *mut f32,
        capacity: i32,
    ) -> i32;
    fn aede_spx_close(handle: *mut c_void);
}
pub(super) struct Codec {
    handle: NonNull<c_void>,
    reader: Reader,
    pub info: Info,
    floating: bool,
    precision: u32,
    raw: Vec<i32>,
}
// Each handle is exclusively owned, never Sync, and closes before its Reader.
// WavPack contexts have no shared codec state; all Musepack calls take MPC.
unsafe impl Send for Codec {}
impl Codec {
    pub fn open(file: File, kind: Kind) -> Result<Self> {
        let mut reader = Reader::new(file)?;
        let mut properties = Properties::default();
        let guard = if kind == Kind::Musepack {
            Some(
                MPC.lock()
                    .map_err(|_| Error("Musepack lock poisoned".into()))?,
            )
        } else {
            None
        };
        // SAFETY: both boxes have stable addresses, valid for the entire foreign
        // handle lifetime. The API initializes Properties without retaining it.
        let raw = unsafe {
            match kind {
                Kind::WavPack => aede_wv_open(reader.abi.as_mut(), &mut properties),
                Kind::Musepack => aede_mpc_open(reader.abi.as_mut(), &mut properties),
                _ => return Err(Error("wrong native codec factory".into())),
            }
        };
        drop(guard);
        let handle = NonNull::new(raw)
            .ok_or_else(|| Error(format!("{} native open failed", kind.label())))?;
        let bits =
            (properties.floating == 0 && properties.lossless != 0).then_some(properties.bits);
        let info = Info {
            kind,
            sample_rate: properties.rate,
            channels: properties.channels as usize,
            bits,
            frames: u64::try_from(properties.frames).ok(),
        };
        let codec = Self {
            handle,
            reader,
            info,
            floating: properties.floating != 0,
            precision: properties.bits,
            raw: Vec::new(),
        };
        codec.reader.checked()?;
        codec.info.validate()?;
        if kind == Kind::WavPack && !(1..=32).contains(&codec.precision) {
            return Err(Error("invalid WavPack precision".into()));
        }
        if codec.info.frames.is_none() {
            return Err(Error("native profile requires a known frame count".into()));
        }
        Ok(codec)
    }
    pub fn next(&mut self, current: &mut impl FnMut() -> Result<()>) -> Result<Vec<f32>> {
        if self.info.kind == Kind::Musepack {
            let mut pcm = vec![0.0; 4608];
            let mut empty = 0usize;
            let frames = loop {
                current()?;
                let guard = MPC
                    .lock()
                    .map_err(|_| Error("Musepack lock poisoned".into()))?;
                // SAFETY: MPC_DECODER_BUFFER_LENGTH writable floats, exclusively
                // owned handle; zero-output pre-roll is distinct from EOF.
                let frames = unsafe {
                    aede_mpc_decode(self.handle.as_ptr(), pcm.as_mut_ptr(), pcm.len() as u32)
                };
                drop(guard);
                self.reader.checked()?;
                current()?;
                if frames != -2 {
                    break frames;
                }
                empty += 1;
                if empty > 65536 {
                    return Err(Error("Musepack empty-frame budget exceeded".into()));
                }
            };
            self.reader.checked()?;
            let count = usize::try_from(frames)
                .ok()
                .and_then(|n| n.checked_mul(self.info.channels))
                .filter(|n| *n <= pcm.len())
                .ok_or_else(|| Error("Musepack packet failed".into()))?;
            pcm.truncate(count);
            return Ok(pcm);
        }
        self.raw.resize(4096 * self.info.channels, 0);
        // SAFETY: requested frames * admitted channel count equals raw.len().
        let frames = unsafe { aede_wv_decode(self.handle.as_ptr(), self.raw.as_mut_ptr(), 4096) };
        self.reader.checked()?;
        let count = usize::try_from(frames)
            .ok()
            .and_then(|n| n.checked_mul(self.info.channels))
            .filter(|n| *n <= self.raw.len())
            .ok_or_else(|| Error("WavPack packet/CRC failed".into()))?;
        let scale = 2.0f64.powi(self.precision as i32 - 1);
        Ok(self.raw[..count]
            .iter()
            .map(|v| {
                if self.floating {
                    f32::from_bits(*v as u32)
                } else {
                    (f64::from(*v) / scale) as f32
                }
            })
            .collect())
    }
}
impl Drop for Codec {
    fn drop(&mut self) {
        if self.info.kind == Kind::Musepack {
            if let Ok(_lock) = MPC.lock() {
                // SAFETY: uniquely owned live handle; reader is still alive.
                unsafe { aede_mpc_close(self.handle.as_ptr()) };
            }
        } else {
            // SAFETY: uniquely owned live handle; reader is still alive.
            unsafe { aede_wv_close(self.handle.as_ptr()) };
        }
    }
}
pub(super) struct Speex {
    handle: NonNull<c_void>,
    pub size: usize,
    pub version: i32,
    channels: usize,
}
// Independent decoder/bits/stereo state, only accessed by its owning worker.
unsafe impl Send for Speex {}
impl Speex {
    pub fn open(mode: i32, channels: usize) -> Result<Self> {
        let (mut size, mut lookahead, mut version) = (0, 0, 0);
        // SAFETY: mode/channel admission occurs before the call; all out values
        // point to live local i32 storage and are not retained.
        let raw = unsafe {
            aede_spx_open(
                mode,
                channels as i32,
                &mut size,
                &mut lookahead,
                &mut version,
            )
        };
        let handle = NonNull::new(raw).ok_or_else(|| Error("Speex native open failed".into()))?;
        let codec = Self {
            handle,
            size: usize::try_from(size).unwrap_or(0),
            version,
            channels,
        };
        if codec.size == 0 || codec.size > 640 {
            return Err(Error("invalid Speex frame size".into()));
        }
        Ok(codec)
    }
    pub fn decode(&mut self, packet: &[u8], frames: usize, terminal: bool) -> Result<Vec<f32>> {
        if !(1..=16).contains(&frames) || packet.is_empty() || packet.len() > 65536 {
            return Err(Error("invalid Speex packet budget".into()));
        }
        let mut pcm = vec![0.0; self.size * frames * self.channels];
        // SAFETY: immutable packet stays live during this synchronous call;
        // output capacity covers exactly the admitted frames * channels.
        let count = unsafe {
            aede_spx_decode(
                self.handle.as_ptr(),
                packet.as_ptr(),
                packet.len() as i32,
                frames as i32,
                i32::from(terminal),
                pcm.as_mut_ptr(),
                pcm.len() as i32,
            )
        };
        let count = usize::try_from(count)
            .ok()
            .filter(|count| {
                *count > 0
                    && *count <= self.size * frames
                    && count.is_multiple_of(self.size)
                    && (terminal || *count == self.size * frames)
            })
            .ok_or_else(|| Error("Speex packet failed".into()))?;
        pcm.truncate(count * self.channels);
        for sample in &mut pcm {
            *sample /= 32768.0;
        }
        Ok(pcm)
    }
}
impl Drop for Speex {
    fn drop(&mut self) {
        // SAFETY: this is the only owner and no callback retains the handle.
        unsafe { aede_spx_close(self.handle.as_ptr()) };
    }
}
