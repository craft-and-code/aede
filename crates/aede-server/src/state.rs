//! Shared server state and command callbacks.

use super::*;

#[derive(Clone)]
pub(super) struct ApiState {
    pub(super) data_dir: PathBuf,
    pub(super) catalog: Arc<RwLock<Option<Catalog>>>,
    pub(super) loaded_stamp: Arc<RwLock<Option<(SystemTime, u64)>>>,
    pub(super) reload_gate: Arc<Mutex<()>>,
    pub(super) events: broadcast::Sender<CatalogEvent>,
    pub(super) shutdown: broadcast::Sender<()>,
    pub(super) admin: Option<Admin>,
    pub(super) next_task_id: Arc<AtomicU64>,
    pub(super) websocket_slots: Arc<Semaphore>,
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
