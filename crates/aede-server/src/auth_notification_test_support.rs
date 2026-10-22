//! Queued notification work observed on a dedicated runtime's sole async worker.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadKind {
    Credentials,
    Policy,
}

impl ReadKind {
    pub(super) fn available(self, state: &ApiState) -> usize {
        match self {
            Self::Credentials => state.auth.read_slots.available_permits(),
            Self::Policy => state.parental.read_slots.available_permits(),
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Credentials => Self::Policy,
            Self::Policy => Self::Credentials,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct ReadBudgets {
    pub(super) credentials: usize,
    pub(super) policy: usize,
}

impl ReadBudgets {
    pub(super) fn capture(state: &ApiState) -> Self {
        Self {
            credentials: state.auth.read_slots.available_permits(),
            policy: state.parental.read_slots.available_permits(),
        }
    }

    fn available_to_stream(self, kind: ReadKind) -> usize {
        match kind {
            ReadKind::Credentials => self.credentials,
            // The gate always leaves one slot free before the stream acquires it.
            ReadKind::Policy => 1,
        }
    }
}

pub(super) struct QueuedRead {
    release: tokio::sync::oneshot::Sender<()>,
    worker: tokio::task::JoinHandle<()>,
}

impl QueuedRead {
    pub(super) async fn release(self) {
        drop(self.release);
        self.worker.await.unwrap();
    }
}

/// The caller's runtime must have one async worker and one blocking worker.
pub(super) async fn queue_read(
    state: ApiState,
    budgets: ReadBudgets,
    target: ReadKind,
) -> QueuedRead {
    let (queued, ready) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let policy_gate = state
            .parental
            .read_slots
            .clone()
            .acquire_many_owned((budgets.policy - 1).try_into().unwrap())
            .await
            .unwrap();
        let (mut release_worker, mut held_worker) = hold_blocking_worker().await;
        assert_eq!(state.events.receiver_count(), 1);
        state
            .events
            .send(CatalogEvent::CatalogChanged {
                scanned_at: Some(1),
            })
            .unwrap();
        loop {
            let observed = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    for kind in [target, target.other()] {
                        if kind.available(&state) < budgets.available_to_stream(kind) {
                            return kind;
                        }
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("the established stream must queue a credential or policy read");
            // Only this socket receives the synthetic event. An empty channel
            // proves this is its validation rather than an earlier heartbeat.
            if observed == target && state.events.is_empty() {
                break;
            }
            // Ownership is observed by another task on the same async worker.
            // Its read task has therefore already returned Pending after
            // spawn_blocking; a second gate queues behind that actual job.
            let (next_release, held) = std::sync::mpsc::channel();
            let (started, next_ready) = tokio::sync::oneshot::channel();
            let next_worker = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                let _ = held.recv();
            });
            drop(release_worker);
            held_worker.await.unwrap();
            next_ready.await.unwrap();
            release_worker = next_release;
            held_worker = next_worker;
        }
        queued.send(()).unwrap();
        let _ = released.await;
        drop(policy_gate);
        drop(release_worker);
        held_worker.await.unwrap();
        tokio::task::spawn_blocking(|| ()).await.unwrap();
    });
    ready.await.expect("queued read setup must finish");
    QueuedRead { release, worker }
}
