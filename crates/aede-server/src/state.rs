//! Shared server state and command callbacks.

use super::*;
use std::sync::atomic::AtomicUsize;

/// Process-local activity of accepted scan workers, including publication.
#[derive(Clone, Default)]
pub(super) struct ScanActivity(Arc<AtomicUsize>);

impl ScanActivity {
    pub(super) fn is_running(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }

    pub(super) fn start(&self) -> ScanGuard {
        self.0.fetch_add(1, Ordering::AcqRel);
        ScanGuard(self.0.clone())
    }
}

/// Lives with the actual worker if an HTTP waiter is cancelled or disconnected.
pub(super) struct ScanGuard(Arc<AtomicUsize>);

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub(super) struct ApiState {
    pub(super) data_dir: PathBuf,
    pub(super) catalog: Arc<RwLock<Option<Catalog>>>,
    pub(super) loaded_stamp: Arc<RwLock<Option<(SystemTime, u64)>>>,
    pub(super) reload_gate: Arc<Mutex<()>>,
    pub(super) events: broadcast::Sender<CatalogEvent>,
    pub(super) shutdown: broadcast::Sender<()>,
    pub(super) admin: Option<Admin>,
    pub(super) auth: Arc<auth::AuthState>,
    pub(super) subsonic: Arc<subsonic::authentication::Authentication>,
    pub(super) next_task_id: Arc<AtomicU64>,
    pub(super) scan_activity: ScanActivity,
    pub(super) connection_slots: Arc<Semaphore>,
    pub(super) remote_request_slots: Arc<Semaphore>,
    pub(super) websocket_slots: Arc<Semaphore>,
    pub(super) playback_slots: Arc<Semaphore>,
    pub(super) inspection_slots: Arc<Semaphore>,
    pub(super) jobs: Arc<jobs::JobRegistry>,
    #[cfg(unix)]
    pub(super) tasks: Arc<delegation::TaskRegistry>,
}

pub(super) type ScanCallback =
    dyn Fn(&mut (dyn FnMut(Progress) + Send)) -> Result<(), String> + Send + Sync;
#[cfg(unix)]
pub(super) type CommandValidator = dyn Fn(&[String], &str) -> bool + Send + Sync;

#[derive(Clone)]
pub(super) struct Admin {
    pub(super) token: String,
    pub(super) data_dir: PathBuf,
    pub(super) scan: Arc<ScanCallback>,
    pub(super) job: Arc<jobs::JobCallback>,
}
