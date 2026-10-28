//! Approved system FFI boundary. Layouts/signatures come from Apple's SDK
//! AudioHardware.h/CoreAudioBaseTypes.h and CoreFoundation CFString.h.
use super::format::{Description, Ranged};
use std::{ffi::c_void, mem::size_of, ptr};

pub(super) const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
pub(super) const OUTPUT: u32 = u32::from_be_bytes(*b"outp");
pub(super) const INPUT: u32 = u32::from_be_bytes(*b"inpt");

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Property {
    pub selector: u32,
    pub scope: u32,
    pub element: u32,
}
impl Property {
    pub fn global(selector: &[u8; 4]) -> Self {
        Self {
            selector: u32::from_be_bytes(*selector),
            scope: GLOBAL,
            element: 0,
        }
    }
    pub fn output(selector: &[u8; 4], element: u32) -> Self {
        Self {
            selector: u32::from_be_bytes(*selector),
            scope: OUTPUT,
            element,
        }
    }
}

#[repr(C)]
pub(super) struct AudioBuffer {
    pub channels: u32,
    pub size: u32,
    pub data: *mut c_void,
}
#[repr(C)]
pub(super) struct BufferList {
    pub count: u32,
    pub first: AudioBuffer,
}
pub(super) type IoProc = unsafe extern "C" fn(
    u32,
    *const c_void,
    *const BufferList,
    *const c_void,
    *mut BufferList,
    *const c_void,
    *mut c_void,
) -> i32;
pub(super) type Listener = unsafe extern "C" fn(u32, u32, *const Property, *mut c_void) -> i32;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(id: u32, address: *const Property) -> u8;
    fn AudioObjectGetPropertyDataSize(
        id: u32,
        address: *const Property,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const Property,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        id: u32,
        address: *const Property,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
    pub(super) fn AudioDeviceCreateIOProcID(
        id: u32,
        callback: Option<IoProc>,
        data: *mut c_void,
        proc_id: *mut *mut c_void,
    ) -> i32;
    pub(super) fn AudioDeviceDestroyIOProcID(id: u32, proc_id: *mut c_void) -> i32;
    pub(super) fn AudioDeviceStart(id: u32, proc_id: *mut c_void) -> i32;
    pub(super) fn AudioDeviceStop(id: u32, proc_id: *mut c_void) -> i32;
    pub(super) fn AudioObjectAddPropertyListener(
        id: u32,
        address: *const Property,
        callback: Option<Listener>,
        data: *mut c_void,
    ) -> i32;
    pub(super) fn AudioObjectRemovePropertyListener(
        id: u32,
        address: *const Property,
        callback: Option<Listener>,
        data: *mut c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringGetLength(value: *const c_void) -> isize;
    fn CFStringGetCharacters(value: *const c_void, range: CfRange, buffer: *mut u16);
    fn CFRelease(value: *const c_void);
}

#[repr(C)]
struct CfRange {
    location: isize,
    length: isize,
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u32 {}
    impl Sealed for i32 {}
    impl Sealed for usize {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
    impl Sealed for super::Description {}
    impl Sealed for super::Ranged {}
}
pub(super) trait Value: sealed::Sealed + Default + Copy {}
impl<T: sealed::Sealed + Default + Copy> Value for T {}

pub(super) fn status(code: i32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("CoreAudio {operation} failed (OSStatus {code})"))
    }
}
pub(super) fn has(id: u32, property: Property) -> bool {
    // SAFETY: property is one initialized C-layout address; HAL retains neither pointer.
    unsafe { AudioObjectHasProperty(id, &property) != 0 }
}
fn bytes(id: u32, property: Property) -> Result<u32, String> {
    let mut bytes = 0;
    // SAFETY: the output points to one initialized UInt32; no qualifier is supplied.
    status(
        unsafe { AudioObjectGetPropertyDataSize(id, &property, 0, ptr::null(), &mut bytes) },
        "query property size",
    )?;
    Ok(bytes)
}
pub(super) fn get<T: Value>(id: u32, property: Property) -> Result<T, String> {
    let mut size = size_of::<T>() as u32;
    if bytes(id, property)? != size {
        return Err("CoreAudio scalar property has an unexpected C size".into());
    }
    let mut value = T::default();
    // SAFETY: Value is sealed to all-bit-pattern-valid C PODs. The matching
    // property selector/type is chosen internally; HAL receives exactly T's
    // writable capacity and must reject a larger property value.
    status(
        unsafe {
            AudioObjectGetPropertyData(
                id,
                &property,
                0,
                ptr::null(),
                &mut size,
                (&mut value as *mut T).cast(),
            )
        },
        "read property",
    )?;
    if size as usize != size_of::<T>() {
        return Err("CoreAudio scalar property changed size".into());
    }
    Ok(value)
}
pub(super) fn list<T: Value>(id: u32, property: Property) -> Result<Vec<T>, String> {
    let mut size = bytes(id, property)?;
    if !(size as usize).is_multiple_of(size_of::<T>()) || size as usize / size_of::<T>() > 1024 {
        return Err("CoreAudio property array exceeds its bounded C layout".into());
    }
    if size == 0 {
        return Ok(Vec::new());
    }
    let capacity = size;
    let mut values = vec![T::default(); size as usize / size_of::<T>()];
    // SAFETY: initialized POD vector, with a byte capacity checked above. HAL
    // owns no returned Rust storage and may not write beyond the supplied size.
    status(
        unsafe {
            AudioObjectGetPropertyData(
                id,
                &property,
                0,
                ptr::null(),
                &mut size,
                values.as_mut_ptr().cast(),
            )
        },
        "read array",
    )?;
    if size > capacity || !(size as usize).is_multiple_of(size_of::<T>()) {
        return Err("CoreAudio array size changed".into());
    }
    values.truncate(size as usize / size_of::<T>());
    Ok(values)
}
pub(super) fn set<T: Value>(id: u32, property: Property, mut value: T) -> Result<(), String> {
    // SAFETY: one initialized, correctly sized POD. Use a writable local copy:
    // the HAL hog-mode contract may write the resulting PID into this argument.
    status(
        unsafe {
            AudioObjectSetPropertyData(
                id,
                &property,
                0,
                ptr::null(),
                size_of::<T>() as u32,
                (&mut value as *mut T).cast(),
            )
        },
        "set property",
    )
}

struct CfString(usize);
impl Drop for CfString {
    fn drop(&mut self) {
        // SAFETY: these name/UID properties vend a caller-owned CFStringRef.
        // A non-null reference is released once, after conversion has ended.
        unsafe { CFRelease(self.0 as *const c_void) };
    }
}
pub(super) fn string(id: u32, property: Property) -> Result<String, String> {
    let raw = get::<usize>(id, property)?;
    if raw == 0 {
        return Err("CoreAudio string property is null".into());
    }
    let owned = CfString(raw);
    // SAFETY: the internally selected property is documented as CFStringRef;
    // `owned` holds that live reference until the last CFString call completes.
    let length = unsafe { CFStringGetLength(owned.0 as *const c_void) };
    if !(0..=4096).contains(&length) {
        return Err("CoreAudio string exceeds its metadata budget".into());
    }
    let mut characters = vec![0u16; length as usize];
    // SAFETY: CFString's complete UTF-16 range and exactly that many writable
    // UniChars. Reading exact length also preserves embedded NULs, so a UID
    // cannot silently become a prefix of another device's identity.
    if length > 0 {
        unsafe {
            CFStringGetCharacters(
                owned.0 as *const c_void,
                CfRange {
                    location: 0,
                    length,
                },
                characters.as_mut_ptr(),
            )
        };
    }
    String::from_utf16(&characters).map_err(|error| error.to_string())
}

pub(super) fn ids() -> Result<Vec<u32>, String> {
    list(1, Property::global(b"dev#"))
}
pub(super) fn resolve(uid: &str) -> Result<u32, String> {
    for id in ids()? {
        if string(id, Property::global(b"uid "))? == uid {
            return Ok(id);
        }
    }
    Err("the explicitly selected CoreAudio device is unavailable".into())
}
