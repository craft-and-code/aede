//! Serialized, owner-scoped checkpoint publication and active profile leases.

use super::*;
use tokio::sync::oneshot;

pub(super) struct Snapshot {
    pub(super) state: PlaybackState,
    pub(super) expected_revision: u64,
    pub(super) reply: Option<oneshot::Sender<Result<(), StreamFailure>>>,
}

pub(super) struct ProfileLease {
    registry: Arc<std::sync::Mutex<BTreeSet<(String, String)>>>,
    key: (String, String),
}

impl ProfileLease {
    pub(super) fn acquire(
        state: &ApiState,
        owner: &str,
        profile: &str,
    ) -> Result<Self, StreamFailure> {
        let key = (owner.to_owned(), profile.to_owned());
        let mut guard = state
            .player_profiles
            .lock()
            .map_err(|_| StreamFailure::STATE_FAILED)?;
        if !guard.insert(key.clone()) {
            return Err(StreamFailure::STATE_CONFLICT);
        }
        Ok(Self {
            registry: state.player_profiles.clone(),
            key,
        })
    }
}

impl Drop for ProfileLease {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.remove(&self.key);
        }
    }
}

fn authenticated_owner(
    state: &ApiState,
    captured: &auth::Principal,
) -> Result<String, StreamFailure> {
    let accounts = auth::load_accounts(state)
        .map_err(|_| StreamFailure::STATE_FAILED)?
        .ok_or(StreamFailure::AUTHENTICATION_EXPIRED)?;
    let principal = auth::principal(
        state,
        &accounts,
        auth::session_token(captured),
        false,
        Instant::now(),
    )
    .map_err(|_| StreamFailure::AUTHENTICATION_EXPIRED)?;
    auth::require_mutation(&principal).map_err(|_| StreamFailure::AUTHENTICATION_EXPIRED)?;
    Ok(principal.owner)
}

pub(super) fn read_profile(
    state: &ApiState,
    captured: &auth::Principal,
    profile: &str,
) -> Result<Option<PlaybackState>, StreamFailure> {
    let _guard = lock_for_history(&state.data_dir).map_err(|_| StreamFailure::STATE_FAILED)?;
    let owner = authenticated_owner(state, captured)?;
    let data = user::load(&user::user_path(&state.data_dir))
        .map_err(|_| StreamFailure::STATE_FAILED)?
        .unwrap_or_default();
    Ok(data.playback_state(&owner, profile).cloned())
}

pub(super) fn persist_snapshot(
    state: &ApiState,
    captured: &auth::Principal,
    snapshot: PlaybackState,
    expected: Option<(&str, u64)>,
) -> Result<(), StreamFailure> {
    let _guard = lock_for_history(&state.data_dir).map_err(|_| StreamFailure::STATE_FAILED)?;
    let owner = authenticated_owner(state, captured)?;
    if owner != snapshot.owner {
        return Err(StreamFailure::AUTHENTICATION_EXPIRED);
    }
    let mut data = user::load(&user::user_path(&state.data_dir))
        .map_err(|_| StreamFailure::STATE_FAILED)?
        .unwrap_or_default();
    match (data.playback_state(&owner, &snapshot.profile), expected) {
        (None, None) => {}
        (Some(previous), Some((session, revision)))
            if previous.session_id == session && previous.revision == revision => {}
        _ => return Err(StreamFailure::STATE_CONFLICT),
    }
    data.set_playback_state(snapshot)
        .map_err(|_| StreamFailure::STATE_FAILED)?;
    user::save(&data, &user::user_path(&state.data_dir)).map_err(|_| StreamFailure::STATE_FAILED)
}

pub(super) struct Writer {
    pub(super) sender: mpsc::Sender<Snapshot>,
    pub(super) worker: tokio::task::JoinHandle<()>,
    pub(super) failed: Arc<AtomicBool>,
}

impl Writer {
    pub(super) fn spawn(state: ApiState, captured: auth::Principal) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Snapshot>(1);
        let failed = Arc::new(AtomicBool::new(false));
        let flag = failed.clone();
        let worker = tokio::spawn(async move {
            while let Some(snapshot) = receiver.recv().await {
                let state = state.clone();
                let captured = captured.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let session = snapshot.state.session_id.clone();
                    let outcome = persist_snapshot(
                        &state,
                        &captured,
                        snapshot.state,
                        Some((&session, snapshot.expected_revision)),
                    );
                    (outcome, snapshot.reply)
                })
                .await;
                match result {
                    Ok((outcome, reply)) => {
                        if outcome.is_err() {
                            flag.store(true, AtomicOrdering::Release);
                        }
                        if let Some(reply) = reply {
                            let _ = reply.send(outcome);
                        }
                    }
                    Err(_) => {
                        flag.store(true, AtomicOrdering::Release);
                    }
                }
            }
        });
        Self {
            sender,
            worker,
            failed,
        }
    }

    pub(super) async fn save(
        &self,
        snapshot: PlaybackState,
        expected_revision: u64,
    ) -> Result<(), StreamFailure> {
        let (reply, result) = oneshot::channel();
        let operation = async {
            self.sender
                .send(Snapshot {
                    state: snapshot,
                    expected_revision,
                    reply: Some(reply),
                })
                .await
                .map_err(|_| StreamFailure::STATE_FAILED)?;
            result.await.map_err(|_| StreamFailure::STATE_FAILED)?
        };
        tokio::time::timeout(HISTORY_WRITE_TIMEOUT + HISTORY_WRITE_TIMEOUT, operation)
            .await
            .map_err(|_| StreamFailure::STATE_FAILED)?
    }
}
