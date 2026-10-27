//! Read-only, descriptor-owned callbacks. No foreign code receives a filename.
use super::{Error, Result};
use std::{
    ffi::c_void,
    fs::File,
    io::{Read, Seek, SeekFrom},
    panic::{AssertUnwindSafe, catch_unwind},
};

pub(super) struct Input {
    pub file: File,
    pub length: i64,
    pub error: Option<String>,
}
#[repr(C)]
pub(super) struct Abi {
    user: *mut c_void,
    length: i64,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> i32,
    tell: unsafe extern "C" fn(*mut c_void) -> i64,
    seek: unsafe extern "C" fn(*mut c_void, i64) -> i32,
}
pub(super) struct Reader {
    pub input: Box<Input>,
    pub abi: Box<Abi>,
}
// The pointers address this owner's stable boxes. Only an exclusive decoder
// worker calls them; the value is movable but is deliberately not Sync/Clone.
unsafe impl Send for Reader {}
impl Reader {
    pub fn new(file: File) -> Result<Self> {
        let length =
            i64::try_from(file.metadata()?.len()).map_err(|_| Error("source too large".into()))?;
        let mut input = Box::new(Input {
            file,
            length,
            error: None,
        });
        let abi = Box::new(Abi {
            user: std::ptr::from_mut(input.as_mut()).cast(),
            length,
            read,
            tell,
            seek,
        });
        Ok(Self { input, abi })
    }
    pub fn checked(&self) -> Result<()> {
        match &self.input.error {
            Some(e) => Err(Error(e.clone())),
            None => Ok(()),
        }
    }
}
fn guarded<T: Copy>(
    user: *mut c_void,
    failed: T,
    visit: impl FnOnce(&mut Input) -> std::io::Result<T>,
) -> T {
    if user.is_null() {
        return failed;
    }
    // SAFETY: only the above ABI constructor supplies this live boxed Input;
    // callbacks are synchronous, exclusive, and never retained after Drop.
    let input = unsafe { &mut *user.cast::<Input>() };
    if input.error.is_some() {
        return failed;
    }
    match catch_unwind(AssertUnwindSafe(|| visit(input))) {
        Ok(Ok(value)) => value,
        Ok(Err(e)) => {
            input.error = Some(e.to_string());
            failed
        }
        Err(_) => {
            input.error = Some("source callback panicked".into());
            failed
        }
    }
}
unsafe extern "C" fn read(user: *mut c_void, destination: *mut c_void, count: i32) -> i32 {
    guarded(user, 0, |input| {
        if count == 0 {
            return Ok(0);
        }
        if destination.is_null() || !(1..=16 * 1024 * 1024).contains(&count) {
            return Err(std::io::Error::other("invalid native read size"));
        }
        let at = input.file.stream_position()?;
        let available = (input.length as u64).saturating_sub(at).min(count as u64) as usize;
        // SAFETY: the reference API provides a writable count-byte destination.
        // Restrict access to the snapshotted regular-file extent and read budget.
        let buffer = unsafe { std::slice::from_raw_parts_mut(destination.cast::<u8>(), available) };
        Ok(input.file.read(buffer)? as i32)
    })
}
unsafe extern "C" fn tell(user: *mut c_void) -> i64 {
    guarded(user, -1, |input| {
        i64::try_from(input.file.stream_position()?).map_err(std::io::Error::other)
    })
}
unsafe extern "C" fn seek(user: *mut c_void, position: i64) -> i32 {
    guarded(user, 0, |input| {
        if position < 0 || position > input.length {
            return Err(std::io::Error::other("native seek outside source"));
        }
        input.file.seek(SeekFrom::Start(position as u64))?;
        Ok(1)
    })
}
