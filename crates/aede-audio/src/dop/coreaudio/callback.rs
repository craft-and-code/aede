//! Owned HAL callback registrations. Unsafe access is confined to C ABI data
//! and the one render state guarded against reentry; no callback performs I/O.
use super::{
    api::{self, BufferList, Property},
    context::{Context, REGISTRY},
    device::{self, Prepared},
};
use crate::dop::Payload;
use crate::{
    dop::{
        Format,
        render::Renderer,
        state::{Fault, Signals},
    },
    queue::PcmConsumer,
};
use std::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) struct Client {
    pub(super) signals: Arc<Signals>,
    busy: AtomicBool,
    renderer: UnsafeCell<Renderer>,
    frame_bytes: usize,
    maximum_bytes: usize,
}
impl Client {
    pub(super) fn new(renderer: Renderer, signals: Arc<Signals>, frame_limit: usize) -> Self {
        let frame_bytes = renderer.frame_bytes();
        Self {
            signals,
            busy: AtomicBool::new(false),
            renderer: UnsafeCell::new(renderer),
            frame_bytes,
            maximum_bytes: frame_limit * frame_bytes,
        }
    }
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

unsafe extern "C" fn render(
    _device: u32,
    _now: *const c_void,
    _input: *const BufferList,
    _input_time: *const c_void,
    output: *mut BufferList,
    _output_time: *const c_void,
    opaque: *mut c_void,
) -> i32 {
    REGISTRY
        .visit(opaque, |client| {
            // SAFETY: HAL supplies output storage for this callback; Context
            // protects the client for the complete closure lifetime.
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                render_client(client, output)
            })) {
                Ok(status) => status,
                Err(_) => {
                    client.signals.fail(Fault::Worker);
                    // SAFETY: the same HAL-owned output buffer; a failed render
                    // cannot unwind through C or publish further source bits.
                    unsafe { clear_output(output) }
                }
            }
        })
        .unwrap_or_else(|| {
            // SAFETY: even a late callback only receives HAL-owned output storage.
            unsafe { clear_output(output) }
        })
}

unsafe fn clear_output(output: *mut BufferList) -> i32 {
    if output.is_null() {
        return -50;
    }
    // SAFETY: the HAL callback contract supplies one valid buffer list.
    let list = unsafe { &mut *output };
    if list.count != 1 {
        return -50;
    }
    let buffer = &mut list.first;
    if buffer.size == 0 {
        return 0;
    }
    if buffer.data.is_null() || buffer.size as usize > 16384 * 8 {
        return -50;
    }
    // SAFETY: bounded writable HAL storage; zero is silent in every admitted
    // integer format and prevents retired tokens from writing programme bits.
    unsafe { ptr::write_bytes(buffer.data.cast::<u8>(), 0, buffer.size as usize) };
    0
}

unsafe fn render_client(client: &Client, output: *mut BufferList) -> i32 {
    if client
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        client.signals.fail(Fault::Reentrant);
        return -50;
    }
    let _busy = Busy(&client.busy);
    if output.is_null() {
        client.signals.fail(Fault::Buffer);
        return -50;
    }
    // SAFETY: HAL vends a valid output AudioBufferList for this callback. The
    // prepared route has exactly one interleaved stream; never walk a guessed
    // flexible-array extent when that observation changes.
    let list = unsafe { &mut *output };
    if list.count != 1 {
        client.signals.fail(Fault::Buffer);
        return -50;
    }
    let buffer = &mut list.first;
    let size = buffer.size as usize;
    if size == 0 {
        return 0;
    }
    if buffer.data.is_null()
        || size > client.maximum_bytes
        || buffer.channels != 2
        || !size.is_multiple_of(client.frame_bytes)
    {
        client.signals.fail(Fault::Buffer);
        if !buffer.data.is_null() && size <= client.maximum_bytes {
            // SAFETY: this HAL buffer owns `size` writable bytes; bounded above.
            unsafe { ptr::write_bytes(buffer.data.cast::<u8>(), 0, size) };
        }
        return -50;
    }
    // SAFETY: HAL owns one exclusive writable buffer of exactly `size` bytes.
    // Its capacity and complete channel-frame alignment were checked above.
    let bytes = unsafe { std::slice::from_raw_parts_mut(buffer.data.cast::<u8>(), size) };
    if client.signals.cancel.load(Ordering::Acquire) {
        bytes.fill(0);
        return 0;
    }
    // SAFETY: the nonblocking busy guard grants this call unique render access.
    // No control-thread operation touches Renderer until all callbacks retire.
    unsafe { &mut *client.renderer.get() }.render(bytes);
    0
}

unsafe extern "C" fn changed(
    _object: u32,
    _count: u32,
    _addresses: *const Property,
    opaque: *mut c_void,
) -> i32 {
    REGISTRY.visit(opaque, |client| {
        if !client.signals.cancel.load(Ordering::Acquire) {
            client.signals.changed.store(true, Ordering::Release);
            client.signals.fail(Fault::Route);
        }
    });
    0
}

pub(in crate::dop) struct Session {
    prepared: Option<Prepared>,
    context: Option<Context<'static>>,
    signals: Arc<Signals>,
    io: *mut c_void,
    observed: usize,
    started: bool,
    closed: bool,
}
impl Session {
    pub fn open(
        uid: &str,
        format: Format,
        input: PcmConsumer<Payload>,
        signals: Arc<Signals>,
    ) -> Result<Self, String> {
        let Prepared {
            lease,
            plan,
            frame_limit,
            tail,
            watched,
        } = device::prepare(uid, format, &|| signals.current())?;
        let renderer = Renderer::new(input, format, plan.carrier, signals.clone());
        let client = Box::new(Client::new(renderer, signals.clone(), frame_limit));
        let (lease, context) = lease.checked(Context::new(&REGISTRY, client))?;
        let prepared = Prepared {
            lease,
            plan,
            frame_limit,
            tail,
            watched,
        };
        let mut session = Self {
            prepared: Some(prepared),
            context: Some(context),
            signals,
            io: ptr::null_mut(),
            observed: 0,
            started: false,
            closed: false,
        };
        if let Err(error) = session.register() {
            return match session.close() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; {cleanup}")),
            };
        }
        Ok(session)
    }
    fn pointer(&self) -> Result<*mut c_void, String> {
        self.context
            .as_ref()
            .map(Context::token)
            .ok_or_else(|| "DoP callback state is closed".into())
    }
    fn register(&mut self) -> Result<(), String> {
        let pointer = self.pointer()?;
        let prepared = self.prepared.as_ref().ok_or("DoP device is closed")?;
        for &(id, address) in &prepared.watched {
            prepared.lease.verify()?;
            // SAFETY: static C callback, copied address and generation-checked token.
            // Each successful registration is counted for symmetric removal.
            api::status(
                unsafe {
                    api::AudioObjectAddPropertyListener(id, &address, Some(changed), pointer)
                },
                "register route listener",
            )?;
            self.observed += 1;
        }
        let id = prepared.lease.backend().device;
        // SAFETY: the IOProc has the declared HAL ABI and an opaque token; output
        // stores one opaque registration, owned until successful destruction.
        api::status(
            unsafe { api::AudioDeviceCreateIOProcID(id, Some(render), pointer, &mut self.io) },
            "create DoP IOProc",
        )?;
        if self.io.is_null() {
            return Err("CoreAudio returned a null DoP IOProc".into());
        }
        prepared.lease.verify()?;
        self.started = true;
        // SAFETY: this IOProc belongs to this device; callback storage is alive.
        api::status(
            unsafe { api::AudioDeviceStart(id, self.io) },
            "start DoP IOProc",
        )?;
        prepared.lease.verify()
    }
    pub fn verify(&self) -> Result<(), String> {
        let prepared = self.prepared.as_ref().ok_or("DoP device is closed")?;
        prepared.lease.verify()?;
        if api::get::<u32>(prepared.lease.backend().device, Property::global(b"fsiz"))? as usize
            != prepared.frame_limit
        {
            return Err("CoreAudio changed the DoP hardware buffer size".into());
        }
        Ok(())
    }
    pub fn tail(&self) -> Duration {
        self.prepared.as_ref().map_or(Duration::ZERO, |p| p.tail)
    }
    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let Some(mut prepared) = self.prepared.take() else {
            return Ok(());
        };
        let id = prepared.lease.backend().device;
        let pointer = self.pointer()?;
        let mut errors = Vec::new();
        self.signals.cancel.store(true, Ordering::Release);
        if let Some(context) = self.context.as_ref() {
            context.detach();
        }
        // A poisoned preparation lock still has to allow retirement. The
        // callback token is already detached and no source bits can be sent.
        let _control = match device::control_guard() {
            Ok(guard) => Some(guard),
            Err(error) => {
                errors.push(error);
                None
            }
        };
        if self.started {
            // SAFETY: retained device/registration; stop precedes destruction.
            if let Err(error) = api::status(
                unsafe { api::AudioDeviceStop(id, self.io) },
                "stop DoP IOProc",
            ) {
                errors.push(error);
            }
        }
        let destroyed = if self.io.is_null() {
            true
        } else {
            // SAFETY: only this Session owns and destroys this registration.
            match api::status(
                unsafe { api::AudioDeviceDestroyIOProcID(id, self.io) },
                "destroy DoP IOProc",
            ) {
                Ok(()) => true,
                Err(error) => {
                    errors.push(error);
                    false
                }
            }
        };
        let mut detached = true;
        for &(object, address) in prepared.watched[..self.observed].iter().rev() {
            // SAFETY: the same callback/token/address as registration. Context
            // has detached new visitors; counted old visitors remain protected.
            if let Err(error) = api::status(
                unsafe {
                    api::AudioObjectRemovePropertyListener(object, &address, Some(changed), pointer)
                },
                "remove DoP route listener",
            ) {
                errors.push(error);
                detached = false;
            }
        }
        let idle = self.context.as_mut().is_none_or(Context::retire);
        if destroyed && idle {
            if let Err(error) = prepared.lease.restore() {
                errors.push(error);
            }
        } else {
            errors.push(
                "DoP IOProc retirement is unconfirmed; its lease is retained until process exit"
                    .into(),
            );
            std::mem::forget(prepared.lease);
        }
        if (!destroyed || !detached || !idle)
            && let Err(error) = device::retire(id)
        {
            errors.push(error);
        }
        self.io = ptr::null_mut();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

#[cfg(test)]
#[path = "callback_tests.rs"]
mod tests;
impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("DoP HAL cleanup failed: {error}");
        }
    }
}
