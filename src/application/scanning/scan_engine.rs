//! Owns the worker threads and the shared queue.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, unbounded};
use parking_lot::{Mutex, RwLock};

use super::node_id_allocator::NodeIdAllocator;
use super::scan_statistics::ScanStatistics;
use super::scan_task::ScanTask;
use super::scan_worker::{ScanSettings, ScanWorker, WorkerContext};
use super::task_queue::{QueueDepths, TaskQueue};
use crate::domain::ports::{DirectoryReader, FileSystemProbe};
use crate::domain::storage::{ScanEvent, ScanPolicy};

struct WorkerSlot {
    index: usize,
    handle: JoinHandle<()>,
}

pub struct ScanEngine {
    context: Arc<WorkerContext>,
    events: Receiver<ScanEvent>,
    desired_workers: Arc<AtomicUsize>,
    workers: Mutex<Vec<WorkerSlot>>,
}

impl ScanEngine {
    pub fn start(
        reader: Arc<dyn DirectoryReader>,
        probe: Arc<dyn FileSystemProbe>,
        policy: ScanPolicy,
        mount_points: HashSet<PathBuf>,
        worker_count: usize,
    ) -> Self {
        let (sender, receiver) = unbounded();
        let context = Arc::new(WorkerContext {
            reader,
            probe,
            queue: Arc::new(TaskQueue::new()),
            allocator: NodeIdAllocator::new(),
            statistics: Arc::new(ScanStatistics::new()),
            settings: RwLock::new(Arc::new(ScanSettings {
                policy,
                mount_points,
                graft_points: HashSet::new(),
            })),
            hard_links: Mutex::new(HashSet::new()),
            events: sender,
        });
        let engine = Self {
            context,
            events: receiver,
            desired_workers: Arc::new(AtomicUsize::new(0)),
            workers: Mutex::new(Vec::new()),
        };
        engine.set_worker_count(worker_count.max(1));
        engine
    }

    /// A sensible default. Measured on APFS (macOS) the kernel serialises metadata calls
    /// heavily: 4–8 threads finish a warm scan fastest and more threads only add system
    /// time. Linux filesystems scale further, so the default is more generous there.
    pub fn recommended_worker_count() -> usize {
        let cores = std::thread::available_parallelism().map(|cores| cores.get()).unwrap_or(4);
        if cfg!(target_os = "macos") { cores.clamp(2, 8) } else { (cores * 2).clamp(4, 32) }
    }

    pub fn set_worker_count(&self, count: usize) {
        let count = count.clamp(1, 256);
        self.desired_workers.store(count, Ordering::SeqCst);
        let mut workers = self.workers.lock();
        workers.retain(|slot| !slot.handle.is_finished());
        for index in 0..count {
            if workers.iter().any(|slot| slot.index == index) {
                continue;
            }
            let handle = self.spawn_worker(index);
            workers.push(WorkerSlot { index, handle });
        }
        // Wake idle workers so those above the new count can leave.
        self.context.queue.push_many(std::iter::empty());
    }

    fn spawn_worker(&self, index: usize) -> JoinHandle<()> {
        let context = Arc::clone(&self.context);
        let desired = Arc::clone(&self.desired_workers);
        std::thread::Builder::new()
            .name(format!("tree-cleaner-scan-{index}"))
            .spawn(move || {
                let worker = ScanWorker::new(Arc::clone(&context));
                context.statistics.worker_started();
                loop {
                    if index >= desired.load(Ordering::SeqCst) {
                        break;
                    }
                    match context.queue.pop(Duration::from_millis(250)) {
                        Some(task) => {
                            context.statistics.worker_busy();
                            worker.execute(task);
                            context.statistics.worker_idle();
                        }
                        None => {
                            if context.queue.is_shut_down() {
                                break;
                            }
                        }
                    }
                }
                context.statistics.worker_stopped();
            })
            .expect("failed to spawn scanner thread")
    }

    pub fn desired_worker_count(&self) -> usize {
        self.desired_workers.load(Ordering::SeqCst)
    }

    pub fn events(&self) -> &Receiver<ScanEvent> {
        &self.events
    }

    pub fn queue(&self) -> &Arc<TaskQueue> {
        &self.context.queue
    }

    pub fn allocator(&self) -> &NodeIdAllocator {
        &self.context.allocator
    }

    pub fn statistics(&self) -> &Arc<ScanStatistics> {
        &self.context.statistics
    }

    pub fn reader_name(&self) -> &'static str {
        self.context.reader.name()
    }

    pub fn settings(&self) -> Arc<ScanSettings> {
        self.context.settings()
    }

    pub fn update_settings(&self, update: impl FnOnce(&mut ScanSettings)) {
        let mut settings = (*self.context.settings()).clone();
        update(&mut settings);
        *self.context.settings.write() = Arc::new(settings);
    }

    pub fn submit(&self, task: ScanTask) {
        self.context.queue.push(task);
    }

    pub fn set_focus(&self, focus: Option<PathBuf>) {
        self.context.queue.set_focus(focus);
    }

    pub fn set_background_paused(&self, paused: bool) {
        self.context.queue.set_background_paused(paused);
    }

    pub fn is_background_paused(&self) -> bool {
        self.context.queue.is_background_paused()
    }

    pub fn queue_depths(&self) -> QueueDepths {
        self.context.queue.depths()
    }

    /// True when no task is queued and no worker is busy.
    pub fn is_idle(&self) -> bool {
        !self.context.queue.has_runnable_work() && self.context.statistics.busy_workers() == 0
    }

    pub fn shutdown(&self) {
        self.context.queue.shutdown();
        let workers = std::mem::take(&mut *self.workers.lock());
        for slot in workers {
            let _ = slot.handle.join();
        }
    }
}

impl Drop for ScanEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
