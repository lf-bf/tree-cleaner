//! Executes deletion plans built in the explorer, reporting progress as it goes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use crate::domain::deletion::{
    CriticalPathGuard, DeletionMode, DeletionOutcome, DeletionPlan, DeletionReport, DeletionStatus,
};
use crate::domain::ports::{FileRemover, RemovalError, RemovalObserver};
use crate::domain::storage::ByteSize;

/// How often progress inside one item is reported at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// Progress messages sent while a plan runs on a background thread.
#[derive(Clone, Debug)]
pub enum DeletionProgress {
    Started {
        total: usize,
    },
    ItemStarted {
        index: usize,
        path: PathBuf,
    },
    /// Periodic update while a large item is being removed.
    ItemProgress {
        index: usize,
        entries_removed: u64,
        bytes_removed: u64,
        current: PathBuf,
    },
    ItemFinished {
        index: usize,
        outcome: DeletionOutcome,
    },
    Finished(DeletionReport),
}

/// Lets the interface follow and cancel a run.
#[derive(Debug)]
pub struct DeletionHandle {
    pub receiver: Receiver<DeletionProgress>,
    cancel: Arc<AtomicBool>,
}

impl DeletionHandle {
    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Forwards removal progress for one item, throttled, and relays cancellation.
struct ItemObserver<'a> {
    index: usize,
    sender: &'a Sender<DeletionProgress>,
    cancel: &'a AtomicBool,
    entries_removed: u64,
    bytes_removed: u64,
    last_report: Instant,
}

impl<'a> ItemObserver<'a> {
    fn new(index: usize, sender: &'a Sender<DeletionProgress>, cancel: &'a AtomicBool) -> Self {
        Self { index, sender, cancel, entries_removed: 0, bytes_removed: 0, last_report: Instant::now() }
    }
}

impl RemovalObserver for ItemObserver<'_> {
    fn entry_removed(&mut self, path: &Path, bytes: u64) {
        self.entries_removed += 1;
        self.bytes_removed = self.bytes_removed.saturating_add(bytes);
        if self.last_report.elapsed() >= PROGRESS_INTERVAL {
            self.last_report = Instant::now();
            let _ = self.sender.send(DeletionProgress::ItemProgress {
                index: self.index,
                entries_removed: self.entries_removed,
                bytes_removed: self.bytes_removed,
                current: path.to_path_buf(),
            });
        }
    }

    fn should_cancel(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct DeletionService {
    remover: Arc<dyn FileRemover>,
    guard: Arc<CriticalPathGuard>,
}

impl DeletionService {
    pub fn new(remover: Arc<dyn FileRemover>, guard: Arc<CriticalPathGuard>) -> Self {
        Self { remover, guard }
    }

    pub fn guard(&self) -> &CriticalPathGuard {
        &self.guard
    }

    /// Runs the plan, reporting each item as it starts and finishes. Items after a
    /// cancellation request are reported as cancelled without being touched.
    pub fn execute(
        &self,
        plan: &DeletionPlan,
        progress: &Sender<DeletionProgress>,
        cancel: &AtomicBool,
    ) -> DeletionReport {
        let _ = progress.send(DeletionProgress::Started { total: plan.items.len() });
        let mut report = DeletionReport::default();
        for (index, item) in plan.items.iter().enumerate() {
            let outcome = if cancel.load(Ordering::SeqCst) {
                DeletionOutcome {
                    item: item.clone(),
                    status: DeletionStatus::Cancelled,
                    bytes_removed: ByteSize::ZERO,
                    entries_removed: 0,
                }
            } else {
                let _ = progress.send(DeletionProgress::ItemStarted { index, path: item.path.clone() });
                let mut observer = ItemObserver::new(index, progress, cancel);
                let status = match self.guard.check(&item.path) {
                    Err(reason) => DeletionStatus::Refused(reason),
                    Ok(()) => match self.remover.remove(&item.path, plan.mode, &mut observer) {
                        Ok(_) => match plan.mode {
                            DeletionMode::Permanent => DeletionStatus::Removed,
                            DeletionMode::Trash => DeletionStatus::MovedToTrash,
                        },
                        Err(RemovalError::PermissionDenied) => DeletionStatus::NeedsPrivileges,
                        Err(RemovalError::NotFound) => DeletionStatus::Removed,
                        Err(RemovalError::Cancelled) => DeletionStatus::Cancelled,
                        Err(RemovalError::Other(reason)) => DeletionStatus::Failed(reason),
                    },
                };
                DeletionOutcome {
                    item: item.clone(),
                    status,
                    bytes_removed: ByteSize::new(observer.bytes_removed),
                    entries_removed: observer.entries_removed,
                }
            };
            let _ = progress.send(DeletionProgress::ItemFinished { index, outcome: outcome.clone() });
            report.outcomes.push(outcome);
        }
        let _ = progress.send(DeletionProgress::Finished(report.clone()));
        report
    }

    /// Spawns the plan on its own thread; progress arrives through the handle.
    pub fn execute_in_background(&self, plan: DeletionPlan) -> DeletionHandle {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let service = self.clone();
        let cancel_flag = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name("tree-cleaner-delete".to_owned())
            .spawn(move || {
                service.execute(&plan, &sender, &cancel_flag);
            })
            .expect("failed to spawn deletion thread");
        DeletionHandle { receiver, cancel }
    }
}
