//! A small worker pool that runs transfers and reports progress.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use crate::client::{local_size, SftpClient};
use crate::error::SftpError;

pub type TransferId = u32;

/// Transfers that run at the same time.
const WORKERS: usize = 2;
/// Progress events are throttled to this interval.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum Direction {
    Upload,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum TransferState {
    Queued,
    Running,
    Done,
    Cancelled,
    Failed { message: String },
}

/// Sent whenever a transfer changes state or makes progress.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct TransferEvent {
    pub id: TransferId,
    pub direction: Direction,
    /// Display name (the file or folder name).
    pub name: String,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
    #[serde(flatten)]
    pub state: TransferState,
}

pub struct TransferRequest {
    pub direction: Direction,
    pub local: PathBuf,
    pub remote: String,
}

struct Job {
    id: TransferId,
    req: TransferRequest,
    cancel: Arc<AtomicBool>,
}

pub struct TransferQueue {
    next_id: AtomicU32,
    tx: mpsc::Sender<Job>,
    cancels: Mutex<HashMap<TransferId, Arc<AtomicBool>>>,
    emit: Arc<dyn Fn(TransferEvent) + Send + Sync>,
}

impl TransferQueue {
    /// `emit` is called from worker threads with every state/progress change.
    pub fn new(
        client: Arc<SftpClient>,
        emit: impl Fn(TransferEvent) + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let emit: Arc<dyn Fn(TransferEvent) + Send + Sync> = Arc::new(emit);
        for _ in 0..WORKERS {
            let (rx, client, emit) = (rx.clone(), client.clone(), emit.clone());
            thread::spawn(move || loop {
                // Hold the lock only while waiting for the next job.
                let job = match rx.lock().unwrap().recv() {
                    Ok(j) => j,
                    Err(_) => return,
                };
                run(&client, job, &*emit);
            });
        }
        Self {
            next_id: AtomicU32::new(0),
            tx,
            cancels: Mutex::new(HashMap::new()),
            emit,
        }
    }

    pub fn submit(&self, req: TransferRequest) -> TransferId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancels.lock().unwrap().insert(id, cancel.clone());
        (self.emit)(event(id, &req, 0, 0, TransferState::Queued));
        let _ = self.tx.send(Job { id, req, cancel });
        id
    }

    /// Ask a queued or running transfer to stop.
    pub fn cancel(&self, id: TransferId) {
        if let Some(flag) = self.cancels.lock().unwrap().get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

fn event(
    id: TransferId,
    req: &TransferRequest,
    done: u64,
    total: u64,
    state: TransferState,
) -> TransferEvent {
    let name = match req.direction {
        Direction::Upload => req
            .local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        Direction::Download => req
            .remote
            .rsplit('/')
            .find(|s| !s.is_empty())
            .map(str::to_string),
    }
    .unwrap_or_default();
    TransferEvent {
        id,
        direction: req.direction,
        name,
        done,
        total,
        state,
    }
}

fn run(client: &SftpClient, job: Job, emit: &(dyn Fn(TransferEvent) + Send + Sync)) {
    let Job { id, req, cancel } = job;
    if cancel.load(Ordering::Relaxed) {
        emit(event(id, &req, 0, 0, TransferState::Cancelled));
        return;
    }
    let total = match req.direction {
        Direction::Upload => local_size(&req.local).map_err(|e| SftpError::local(&req.local, e)),
        Direction::Download => client.size_of(&req.remote),
    };
    let total = match total {
        Ok(t) => t,
        Err(e) => {
            emit(event(
                id,
                &req,
                0,
                0,
                TransferState::Failed {
                    message: e.to_string(),
                },
            ));
            return;
        }
    };
    emit(event(id, &req, 0, total, TransferState::Running));

    let mut done = 0u64;
    let mut last = Instant::now();
    let mut progress = |n: u64| {
        done += n;
        if last.elapsed() >= PROGRESS_EVERY {
            last = Instant::now();
            emit(event(id, &req, done, total, TransferState::Running));
        }
    };
    let result = match req.direction {
        Direction::Upload => client.upload(&req.local, &req.remote, &cancel, &mut progress),
        Direction::Download => client.download(&req.remote, &req.local, &cancel, &mut progress),
    };
    let state = match result {
        Ok(()) => {
            done = total;
            TransferState::Done
        }
        Err(SftpError::Cancelled) => TransferState::Cancelled,
        Err(e) => TransferState::Failed {
            message: e.to_string(),
        },
    };
    emit(event(id, &req, done, total, state));
}
