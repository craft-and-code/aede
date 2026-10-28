//! Transactional device changes. No I/O callback or source decoding lives here.
use super::format::{Description, Plan};
use std::time::{Duration, Instant};

pub(super) trait Backend {
    fn hog(&self) -> Result<i32, String>;
    fn set_hog(&mut self, value: i32) -> Result<(), String>;
    fn rate(&self) -> Result<f64, String>;
    fn set_rate(&mut self, value: f64) -> Result<(), String>;
    fn physical(&self) -> Result<Description, String>;
    fn set_physical(&mut self, value: Description) -> Result<(), String>;
    fn virtual_format(&self) -> Result<Description, String>;
    fn set_virtual(&mut self, value: Description) -> Result<(), String>;
    fn health(&self) -> Result<(), String>;
}

#[derive(Clone, Copy)]
struct Saved {
    rate: f64,
    physical: Description,
    virtual_format: Description,
}

pub(super) struct Lease<B: Backend> {
    backend: B,
    saved: Saved,
    plan: Plan,
    pid: i32,
    owned: bool,
    acquired: bool,
    prepared: bool,
}

impl<B: Backend> Lease<B> {
    pub fn acquire(
        backend: B,
        plan: Plan,
        pid: i32,
        current: &impl Fn() -> Result<(), String>,
    ) -> Result<Self, String> {
        current()?;
        backend.health()?;
        if pid <= 0 || backend.hog()? != -1 {
            return Err("the selected DAC is already owned exclusively; DoP will not take another session's lease".into());
        }
        let saved = Saved {
            rate: backend.rate()?,
            physical: backend.physical()?,
            virtual_format: backend.virtual_format()?,
        };
        let mut lease = Self {
            backend,
            saved,
            plan,
            pid,
            owned: true,
            acquired: false,
            prepared: false,
        };
        let result = (|| {
            current()?;
            // Mark ownership before setting it: a failed HAL call may still
            // have taken effect and must be checked during rollback.
            lease.backend.set_hog(pid)?;
            if lease.backend.hog()? != pid {
                return Err("CoreAudio did not grant this process exclusive ownership".into());
            }
            lease.acquired = true;
            current()?;
            lease.backend.set_rate(plan.physical.rate)?;
            current()?;
            lease.backend.set_physical(plan.physical)?;
            current()?;
            lease.backend.set_virtual(plan.virtual_format)?;
            wait(
                || {
                    current()?;
                    lease.backend.health()?;
                    if lease.backend.hog()? != pid {
                        return Err("exclusive DoP ownership changed".into());
                    }
                    Ok(())
                },
                || lease.matches(),
            )?;
            lease.prepared = true;
            current()
        })();
        match result {
            Ok(()) => Ok(lease),
            Err(error) => match lease.restore() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; device rollback failed: {cleanup}")),
            },
        }
    }

    /// Keep post-acquisition setup and the device lease in one ownership scope.
    pub fn checked<T>(mut self, result: Result<T, String>) -> Result<(Self, T), String> {
        match result {
            Ok(value) => Ok((self, value)),
            Err(error) => match self.restore() {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; device rollback failed: {cleanup}")),
            },
        }
    }

    fn matches(&self) -> Result<bool, String> {
        Ok(self.backend.rate()? == self.plan.physical.rate
            && self.backend.physical()? == self.plan.physical
            && self.backend.virtual_format()? == self.plan.virtual_format)
    }

    pub fn verify(&self) -> Result<(), String> {
        self.backend.health()?;
        if !self.owned || self.backend.hog()? != self.pid {
            return Err("exclusive DoP ownership changed".into());
        }
        if !self.matches()? {
            return Err("the selected DAC's DoP stream format changed".into());
        }
        Ok(())
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn restore(&mut self) -> Result<(), String> {
        if !self.owned {
            return Ok(());
        }
        self.owned = false;
        let owner = self.backend.hog()?;
        if owner != self.pid {
            return if owner == -1 && !self.acquired {
                Ok(())
            } else {
                Err("DoP lost its device lease; another owner's settings were preserved".into())
            };
        }
        let mut errors = Vec::new();
        // Once a prepared stream changes underneath us, do not overwrite the
        // newly selected formats. Failed preparation still needs full rollback
        // because changing nominal rate can implicitly change both formats.
        let restore_formats = if !self.prepared {
            true
        } else {
            match self.matches() {
                Ok(same) => same,
                Err(error) => {
                    errors.push(error);
                    false
                }
            }
        };
        if restore_formats {
            if let Err(error) = self.backend.set_rate(self.saved.rate) {
                errors.push(error);
            }
            if let Err(error) = self.backend.set_physical(self.saved.physical) {
                errors.push(error);
            }
            if let Err(error) = self.backend.set_virtual(self.saved.virtual_format) {
                errors.push(error);
            }
            let result = wait(
                || Ok(()),
                || {
                    Ok(self.backend.rate()? == self.saved.rate
                        && self.backend.physical()? == self.saved.physical
                        && self.backend.virtual_format()? == self.saved.virtual_format)
                },
            );
            if let Err(error) = result {
                errors.push(error);
            }
        } else {
            errors.push("DoP format changed externally; its new settings were preserved".into());
        }
        match self.backend.hog() {
            Ok(owner) if owner == self.pid => {
                if let Err(error) = self.backend.set_hog(-1) {
                    errors.push(error);
                }
                match self.backend.hog() {
                    Ok(-1) => {}
                    Ok(_) => errors.push("CoreAudio did not release the DoP lease".into()),
                    Err(error) => errors.push(error),
                }
            }
            Ok(_) => errors.push("DoP ownership changed during restoration".into()),
            Err(error) => errors.push(error),
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl<B: Backend> Drop for Lease<B> {
    fn drop(&mut self) {
        // Explicit close returns failures. This fallback also surfaces an
        // unexpected early exit instead of silently losing cleanup evidence.
        if let Err(error) = self.restore() {
            eprintln!("DoP device restoration failed: {error}");
        }
    }
}

fn wait(
    mut current: impl FnMut() -> Result<(), String>,
    mut matches: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        current()?;
        if matches()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("CoreAudio format readback did not settle before its deadline".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(test)]
#[path = "lease_tests.rs"]
mod tests;
