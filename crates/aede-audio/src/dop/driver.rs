//! Control-thread ownership of a native output; source reads stay independent.
use super::{
    Format, Payload,
    state::{Fault, Signals},
};
use crate::queue::PcmConsumer;
use std::{
    sync::{Arc, atomic::Ordering, mpsc::SyncSender},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(super) enum ServiceError {
    Retired,
    Failed(String),
}
impl ServiceError {
    pub fn current(signals: &Signals) -> Result<(), Self> {
        if signals.fault.load(Ordering::Acquire) != 0 {
            return Err(Self::Failed(
                signals
                    .error()
                    .unwrap_or_else(|| "DoP output failed".into()),
            ));
        }
        if signals.cancel.load(Ordering::Acquire) {
            return Err(Self::Retired);
        }
        Ok(())
    }
}
impl From<String> for ServiceError {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}
impl From<&str> for ServiceError {
    fn from(error: &str) -> Self {
        Self::Failed(error.into())
    }
}

pub(super) trait Driver {
    fn service(&mut self) -> Result<(), ServiceError> {
        Ok(())
    }
    fn prepared(&self) -> bool {
        true
    }
    fn verify(&self) -> Result<(), String>;
    fn tail(&self) -> Duration;
    fn close(&mut self) -> Result<(), String>;
}
pub(super) type Factory = Arc<
    dyn Fn(Format, PcmConsumer<Payload>, Arc<Signals>) -> Result<Box<dyn Driver>, String>
        + Send
        + Sync,
>;

/// Retirement inside a native query is a normal stop. Actual I/O errors and
/// source/route faults still propagate, including faults racing cancellation.
fn service(driver: &mut dyn Driver, signals: &Signals) -> Result<bool, String> {
    let result = ServiceError::current(signals)
        .and_then(|()| driver.service())
        .and_then(|()| ServiceError::current(signals));
    match result {
        Ok(()) => Ok(true),
        Err(ServiceError::Retired) => Ok(false),
        Err(ServiceError::Failed(error)) => Err(error),
    }
}

pub(super) fn run(
    factory: Factory,
    format: Format,
    input: PcmConsumer<Payload>,
    signals: Arc<Signals>,
    sender: SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let mut sender = Some(sender);
    let mut driver = match factory(format, input, signals.clone()) {
        Ok(driver) => driver,
        Err(error) => {
            if let Some(sender) = sender.take() {
                let _ = sender.send(Err(error.clone()));
            }
            signals.cancel.store(true, Ordering::Release);
            signals.stopped.store(true, Ordering::Release);
            return Err(error);
        }
    };
    let mut natural = false;
    let outcome = (|| {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !signals.ready.load(Ordering::Acquire) || !driver.prepared() {
            if !service(driver.as_mut(), &signals)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("DoP source did not prefill before its startup deadline".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        driver.verify()?;
        if let Some(sender) = sender.take() {
            sender
                .send(Ok(()))
                .map_err(|_| "DoP preparation owner was retired")?;
        }
        let mut last_verify = Instant::now();
        let started = Instant::now();
        let mut progress = Progress::default();
        let mut drain_since = None;
        loop {
            if !service(driver.as_mut(), &signals)? {
                return Ok(());
            }
            progress.observe(
                started.elapsed(),
                signals.consumed.load(Ordering::Acquire),
                signals.admitted.load(Ordering::Acquire)
                    && !signals.paused.load(Ordering::Acquire)
                    && !signals.source_finished.load(Ordering::Acquire),
            )?;
            if last_verify.elapsed() >= Duration::from_millis(25) {
                driver.verify()?;
                last_verify = Instant::now();
            }
            if signals.source_finished.load(Ordering::Acquire)
                && signals.decoder_done.load(Ordering::Acquire)
            {
                let since = drain_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= driver.tail() {
                    natural = true;
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    signals.cancel.store(true, Ordering::Release);
    let cleanup = driver.close();
    // EOF and the producer's final source check can race. A callback/worker
    // fault observed during native drain/close must never become completion.
    let outcome = if outcome.is_ok() && signals.fault.load(Ordering::Acquire) != 0 {
        Err(signals
            .error()
            .unwrap_or_else(|| "DoP output failed".into()))
    } else {
        outcome
    };
    if cleanup.is_err() {
        signals.fail(Fault::Cleanup);
    }
    let result = match (outcome, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(cleanup)) => Err(format!("DoP device cleanup failed: {cleanup}")),
        (Err(error), Err(cleanup)) => Err(format!("{error}; DoP device cleanup failed: {cleanup}")),
    };
    if let Err(error) = &result {
        signals.fail_message(Fault::Route, error.clone());
        if let Some(sender) = sender.take() {
            let _ = sender.send(Err(error.clone()));
        }
    }
    signals.completed.store(
        natural && result.is_ok() && signals.fault.load(Ordering::Acquire) == 0,
        Ordering::Release,
    );
    signals.stopped.store(true, Ordering::Release);
    result
}

#[derive(Default)]
struct Progress {
    consumed: u64,
    since: Duration,
}
impl Progress {
    fn observe(&mut self, now: Duration, consumed: u64, active: bool) -> Result<(), String> {
        if !active || consumed != self.consumed {
            self.since = now;
            self.consumed = consumed;
        } else if now.saturating_sub(self.since) >= Duration::from_secs(5) {
            return Err("native DoP output stopped consuming original source bits".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "driver_tests.rs"]
mod tests;

#[cfg(target_os = "macos")]
impl Driver for super::coreaudio::Session {
    fn verify(&self) -> Result<(), String> {
        super::coreaudio::Session::verify(self)
    }
    fn tail(&self) -> Duration {
        super::coreaudio::Session::tail(self)
    }
    fn close(&mut self) -> Result<(), String> {
        super::coreaudio::Session::close(self)
    }
}
