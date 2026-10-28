//! Fixed callback tokens avoid relying on HAL listener-removal quiescence.
//! HAL retains an opaque integer token, never a pointer to freed Rust storage.
use super::callback::Client;
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

struct Slot {
    used: AtomicBool,
    generation: AtomicUsize,
    active: AtomicUsize,
    pointer: AtomicPtr<Client>,
}
impl Slot {
    const fn new() -> Self {
        Self {
            used: AtomicBool::new(false),
            generation: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            pointer: AtomicPtr::new(ptr::null_mut()),
        }
    }
}
pub(super) struct Registry {
    slots: [Slot; 64],
}
impl Registry {
    pub const fn new() -> Self {
        Self {
            slots: [const { Slot::new() }; 64],
        }
    }
    pub fn visit<R>(&self, token: *mut c_void, body: impl FnOnce(&Client) -> R) -> Option<R> {
        let bits = token.addr();
        let index = bits & 63;
        let generation = bits >> 6;
        if generation == 0 {
            return None;
        }
        let slot = &self.slots[index];
        if slot.generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        slot.active.fetch_add(1, Ordering::SeqCst);
        struct Active<'a>(&'a AtomicUsize);
        impl Drop for Active<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _active = Active(&slot.active);
        if slot.generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        let pointer = slot.pointer.load(Ordering::SeqCst);
        if pointer.is_null() || slot.generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        // SAFETY: Context owns the stable Box. Retirement first swaps the
        // pointer to null, then waits for every visitor counted before this
        // load. SeqCst ordering closes the load/free race. A late old token
        // sees null or a different generation, never the replacement client.
        Some(body(unsafe { &*pointer }))
    }
}

pub(super) static REGISTRY: Registry = Registry::new();
pub(super) struct Context<'a> {
    registry: &'a Registry,
    slot: usize,
    generation: usize,
    client: Option<Box<Client>>,
    retired: Option<bool>,
}
impl<'a> Context<'a> {
    pub fn new(registry: &'a Registry, mut client: Box<Client>) -> Result<Self, String> {
        for (index, slot) in registry.slots.iter().enumerate() {
            if slot
                .used
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }
            let old = slot.generation.load(Ordering::SeqCst);
            // Exhausted tokens stay retired: no wrap can alias an old HAL token.
            if old == usize::MAX >> 6 {
                continue;
            }
            let generation = old + 1;
            slot.generation.store(generation, Ordering::SeqCst);
            slot.pointer.store(&mut *client, Ordering::SeqCst);
            return Ok(Self {
                registry,
                slot: index,
                generation,
                client: Some(client),
                retired: None,
            });
        }
        Err("the bounded CoreAudio DoP callback registry is exhausted".into())
    }
    pub fn token(&self) -> *mut c_void {
        // HAL treats clientData as an opaque value. No pointer derived here is
        // dereferenced; its address encodes an index and a unique generation.
        ptr::without_provenance_mut((self.generation << 6) | self.slot)
    }
    pub fn detach(&self) {
        self.registry.slots[self.slot]
            .pointer
            .swap(ptr::null_mut(), Ordering::SeqCst);
    }
    pub fn retire(&mut self) -> bool {
        if let Some(retired) = self.retired {
            return retired;
        }
        self.detach();
        let slot = &self.registry.slots[self.slot];
        let deadline = Instant::now() + Duration::from_millis(100);
        while slot.active.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let idle = slot.active.load(Ordering::SeqCst) == 0;
        if idle {
            self.client.take();
            slot.used.store(false, Ordering::SeqCst);
        } else if let Some(client) = self.client.take() {
            // An actually stalled callback still borrows it. Keep one bounded
            // allocation and one registry slot until process exit, never UAF.
            let _retained_until_exit = Box::into_raw(client);
        }
        self.retired = Some(idle);
        idle
    }
}
impl Drop for Context<'_> {
    fn drop(&mut self) {
        self.retire();
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
