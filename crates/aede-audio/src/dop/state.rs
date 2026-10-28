//! Atomic DoP lifetime/transport observations. Rendering never takes the error lock.
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Fault {
    Source = 1,
    Underrun,
    Buffer,
    Route,
    #[cfg(target_os = "macos")]
    Reentrant = 5,
    Cleanup = 6,
    Worker = 7,
}
impl Fault {
    pub fn text(code: u8) -> &'static str {
        match code {
            1 => "DoP source failed before complete delivery",
            2 => "DoP source queue underrun; output stopped without PCM fallback",
            3 => "the native DoP output buffer is invalid",
            4 => "the exclusive DoP route changed or disappeared",
            5 => "CoreAudio reentered the DoP render callback",
            6 => "native DoP cleanup failed; restart the process before reusing the DAC",
            _ => "DoP worker failed",
        }
    }
}

pub(super) struct Signals {
    pub cancel: AtomicBool,
    pub admitted: AtomicBool,
    pub paused: AtomicBool,
    pub muted: AtomicBool,
    pub ready: AtomicBool,
    pub decoder_done: AtomicBool,
    pub source_finished: AtomicBool,
    pub completed: AtomicBool,
    pub stopped: AtomicBool,
    #[cfg(target_os = "macos")]
    pub changed: AtomicBool,
    pub fault: AtomicU8,
    pub consumed: AtomicU64,
    pub expected: u64,
    error: Mutex<Option<String>>,
}
impl Signals {
    pub fn new(expected: u64, paused: bool, muted: bool) -> Self {
        Self {
            cancel: AtomicBool::new(false),
            admitted: AtomicBool::new(false),
            paused: AtomicBool::new(paused),
            muted: AtomicBool::new(muted),
            ready: AtomicBool::new(false),
            decoder_done: AtomicBool::new(false),
            source_finished: AtomicBool::new(false),
            completed: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            changed: AtomicBool::new(false),
            fault: AtomicU8::new(0),
            consumed: AtomicU64::new(0),
            expected,
            error: Mutex::new(None),
        }
    }
    pub fn fail(&self, fault: Fault) {
        let _ = self
            .fault
            .compare_exchange(0, fault as u8, Ordering::AcqRel, Ordering::Acquire);
    }
    pub fn fail_message(&self, fault: Fault, error: String) {
        match self.error.lock() {
            Ok(mut slot) => {
                if slot.is_none() {
                    *slot = Some(error);
                }
            }
            Err(_) => self.fail(Fault::Worker),
        }
        self.fail(fault);
    }
    pub fn current(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Acquire) {
            return Err("DoP preparation was retired".into());
        }
        let code = self.fault.load(Ordering::Acquire);
        if code == 0 {
            Ok(())
        } else {
            Err(Fault::text(code).into())
        }
    }
    pub fn error(&self) -> Option<String> {
        let code = self.fault.load(Ordering::Acquire);
        if code == 0 {
            return None;
        }
        match self.error.lock() {
            Ok(slot) => slot.clone().or_else(|| Some(Fault::text(code).into())),
            Err(_) => Some("DoP error state is poisoned".into()),
        }
    }
}
