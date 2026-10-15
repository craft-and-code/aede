//! Bounded PCM reservations retain transmitted prebuffers independently of history.
use super::*;
use aede_core::parental::LeaseGrant;
use std::sync::atomic::AtomicU64;

struct Held {
    grant: LeaseGrant,
    base_ms: u64,
    sent_base_ms: u64,
}
pub(crate) struct Session {
    owner: String,
    captured: auth::Principal,
    held: BlockingMutex<Option<Held>>,
    observed_ms: AtomicU64,
    sent_ms: AtomicU64,
    started: std::sync::OnceLock<Instant>,
    failure: BlockingMutex<Option<&'static str>>,
    last_checked: BlockingMutex<Option<Instant>>,
}
impl Session {
    pub(crate) fn observe(&self, ms: u64) {
        self.observed_ms.fetch_max(ms, Ordering::Relaxed);
    }
    pub(crate) fn sent(&self, ms: u64) {
        self.sent_ms.fetch_max(ms, Ordering::Relaxed);
    }
    /// Reserve at most one second ahead of real time and never exceed this grant.
    /// A client's rapid acknowledgements cannot turn the controlled transport into a download.
    pub(crate) fn available(&self, sent: u64, rate: u32) -> u64 {
        let Some(started) = self.started.get() else {
            return 0;
        };
        let clock_ms = u64::try_from(started.elapsed().as_millis())
            .unwrap_or(u64::MAX)
            .saturating_add(1000);
        let allowed = self
            .held
            .lock()
            .ok()
            .and_then(|held| {
                held.as_ref().map(|held| {
                    held.sent_base_ms
                        .saturating_add(u64::from(held.grant.granted_seconds) * 1000)
                })
            })
            .unwrap_or(0);
        let grant_frames = allowed.saturating_mul(u64::from(rate)) / 1000;
        let available =
            (clock_ms.min(allowed).saturating_mul(u64::from(rate)) / 1000).saturating_sub(sent);
        // A fast-ACK client must not cause thousands of one-frame socket writes.
        // Accumulate about twenty milliseconds, except the exact end of a grant.
        let quantum = (u64::from(rate) / 50)
            .max(1)
            .min(grant_frames.saturating_sub(sent));
        if available < quantum { 0 } else { available }
    }
    pub(crate) fn failure(&self) -> Option<&'static str> {
        self.failure.lock().ok().and_then(|f| *f)
    }
    pub(crate) async fn check(self: &Arc<Self>, state: &ApiState) -> bool {
        let fresh = self
            .last_checked
            .lock()
            .is_ok_and(|checked| checked.is_some_and(|at| at.elapsed() < Duration::from_secs(1)));
        let publication_matches = state.parental_scope.as_ref().is_some_and(|scope| {
            scope.publication == state.parental.publication.load(Ordering::Acquire)
        });
        let unexpired = self.held.lock().is_ok_and(|held| {
            held.as_ref()
                .is_none_or(|held| aede_core::clock::now_seconds() < held.grant.expires_at)
        });
        // Growing device ACKs can arrive much more often than the authorization tick.
        // Keep policy reads off that cadence; admin changes and expiry bypass this cache.
        if fresh && publication_matches && unexpired {
            return true;
        }
        let held = self.clone();
        let for_work = state.clone();
        let result = tokio::task::spawn_blocking(move || held.refresh(&for_work, false))
            .await
            .unwrap_or_else(|_| Err(unavailable()));
        if let Err(error) = result {
            if let Ok(mut reason) = self.failure.lock() {
                *reason = Some(error.code);
            }
            false
        } else {
            if let Ok(mut checked) = self.last_checked.lock() {
                *checked = Some(Instant::now());
            }
            true
        }
    }
    fn refresh(&self, state: &ApiState, close: bool) -> Result<(), ApiError> {
        self.refresh_with_clock(state, close, aede_core::clock::now_seconds)
    }
    fn refresh_with_clock(
        &self,
        state: &ApiState,
        close: bool,
        mut clock: impl FnMut() -> u64,
    ) -> Result<(), ApiError> {
        if !close {
            finish(state)?;
        }
        let now = clock();
        if !close {
            let data = load(state)?;
            data.check_schedule(&self.owner, now, &Media::Music { track: None })
                .map_err(|reason| refused(&reason))?;
            let held = self.held.lock().map_err(|_| unavailable())?;
            if held.as_ref().is_none_or(|held| now < held.grant.expires_at) {
                return Ok(());
            }
        }
        if self.held.lock().map_err(|_| unavailable())?.is_none() {
            return Ok(());
        }
        let _lock = match lock(state) {
            Ok(lock) => lock,
            Err(_)
                if !close
                    && self.held.lock().is_ok_and(|held| {
                        held.as_ref()
                            .is_some_and(|held| now < held.grant.expires_at)
                    }) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !close {
            auth::recheck(state, &self.captured)?;
        }
        let mut data = load(state)?;
        let now = clock();
        let mut held = self.held.lock().map_err(|_| unavailable())?;
        let Some(held) = held.as_mut() else {
            return Ok(());
        };
        let acknowledged = self
            .observed_ms
            .load(Ordering::Relaxed)
            .saturating_sub(held.base_ms);
        // A client already possesses transmitted PCM even without acknowledging it.
        // Preserve that short prebuffer on close instead of refunding repeated
        // receive/disconnect cycles inside the whole-second reservation clock.
        let received = if close {
            self.sent_ms
                .load(Ordering::Relaxed)
                .saturating_sub(held.sent_base_ms)
        } else {
            0
        };
        let consumed = acknowledged
            .max(received)
            .min(u64::from(held.grant.granted_seconds) * 1000);
        if close {
            data.settle(&self.owner, &held.grant.lease_id, consumed, now, true)
                .map_err(|_| unavailable())?;
        } else {
            data.check_schedule(&self.owner, now, &Media::Music { track: None })
                .map_err(|reason| refused(&reason))?;
            held.grant = data
                .renew(&self.owner, &held.grant.lease_id, consumed, 10, now)
                .map_err(|reason| refused(&reason))?;
            // All previously sent PCM belongs to the expired, fully charged
            // grant. A delayed ACK must not spend part of this new one again.
            let sent_ms = self.sent_ms.load(Ordering::Relaxed);
            held.base_ms = sent_ms;
            held.sent_base_ms = sent_ms;
        }
        save(state, &data)
    }
    pub(crate) async fn close(self: &Arc<Self>, state: &ApiState) -> Result<(), ApiError> {
        let held = self.clone();
        let for_work = state.clone();
        tokio::task::spawn_blocking(move || held.refresh(&for_work, true))
            .await
            .unwrap_or_else(|_| Err(unavailable()))
    }
    pub(crate) async fn activate(self: &Arc<Self>, state: &ApiState) -> Result<(), ApiError> {
        let session = self.clone();
        let state = state.clone();
        tokio::task::spawn_blocking(move || {
            let _lock = lock(&state)?;
            finish(&state)?;
            auth::recheck(&state, &session.captured)?;
            let mut held = session.held.lock().map_err(|_| unavailable())?;
            if held.is_some() {
                return Ok(());
            }
            let mut data = load(&state)?;
            let id = aede_core::accounts::random_token().map_err(|_| unavailable())?;
            let grant = data
                .reserve(
                    &session.owner,
                    &id,
                    Media::Music { track: None },
                    10,
                    aede_core::clock::now_seconds(),
                )
                .map_err(|reason| refused(&reason))?;
            save(&state, &data)?;
            *held = Some(Held {
                grant,
                base_ms: 0,
                sent_base_ms: 0,
            });
            let _ = session.started.set(Instant::now());
            if let Ok(mut checked) = session.last_checked.lock() {
                *checked = Some(Instant::now());
            }
            Ok(())
        })
        .await
        .map_err(|_| unavailable())?
    }
}
pub(crate) async fn begin(
    state: &ApiState,
    captured: &auth::Principal,
) -> Result<Option<Arc<Session>>, ApiError> {
    if !supervised(state) {
        return Ok(None);
    }
    let owner = captured.owner.clone();
    Ok(Some(Arc::new(Session {
        owner,
        captured: captured.clone(),
        held: BlockingMutex::new(None),
        observed_ms: AtomicU64::new(0),
        sent_ms: AtomicU64::new(0),
        started: std::sync::OnceLock::new(),
        failure: BlockingMutex::new(None),
        last_checked: BlockingMutex::new(None),
    })))
}

pub(crate) fn failure(state: &ApiState) -> Option<crate::playback_api::StreamFailure> {
    state
        .parental_pcm
        .as_ref()
        .and_then(|session| session.failure())
        .map(crate::playback_api::StreamFailure::parental)
}

#[cfg(test)]
#[path = "pcm_tests.rs"]
mod tests;
