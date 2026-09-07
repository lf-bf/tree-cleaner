//! Lock-free counters updated by scanner threads and read by the interface.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct ScanStatistics {
    started_at: Instant,
    directories_listed: AtomicU64,
    files_measured: AtomicU64,
    bytes_measured: AtomicU64,
    unreadable_directories: AtomicU64,
    permission_denied: AtomicU64,
    dense_directories: AtomicU64,
    hard_links_skipped: AtomicU64,
    busy_workers: AtomicUsize,
    worker_count: AtomicUsize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatisticsSnapshot {
    pub elapsed_seconds: f64,
    pub directories_listed: u64,
    pub files_measured: u64,
    pub bytes_measured: u64,
    pub unreadable_directories: u64,
    pub permission_denied: u64,
    pub dense_directories: u64,
    pub hard_links_skipped: u64,
    pub busy_workers: usize,
    pub worker_count: usize,
}

impl StatisticsSnapshot {
    pub fn files_per_second(&self) -> f64 {
        if self.elapsed_seconds <= 0.0 { 0.0 } else { self.files_measured as f64 / self.elapsed_seconds }
    }
}

impl Default for ScanStatistics {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            directories_listed: AtomicU64::new(0),
            files_measured: AtomicU64::new(0),
            bytes_measured: AtomicU64::new(0),
            unreadable_directories: AtomicU64::new(0),
            permission_denied: AtomicU64::new(0),
            dense_directories: AtomicU64::new(0),
            hard_links_skipped: AtomicU64::new(0),
            busy_workers: AtomicUsize::new(0),
            worker_count: AtomicUsize::new(0),
        }
    }
}

impl ScanStatistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_directory(&self, files: u64, bytes: u64) {
        self.directories_listed.fetch_add(1, Ordering::Relaxed);
        self.files_measured.fetch_add(files, Ordering::Relaxed);
        self.bytes_measured.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_unreadable(&self, permission_denied: bool) {
        self.unreadable_directories.fetch_add(1, Ordering::Relaxed);
        if permission_denied {
            self.permission_denied.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_dense(&self) {
        self.dense_directories.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_hard_link_skipped(&self) {
        self.hard_links_skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_started(&self) {
        self.worker_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_stopped(&self) {
        self.worker_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn worker_busy(&self) {
        self.busy_workers.fetch_add(1, Ordering::Relaxed);
    }

    pub fn worker_idle(&self) {
        self.busy_workers.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn busy_workers(&self) -> usize {
        self.busy_workers.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> StatisticsSnapshot {
        StatisticsSnapshot {
            elapsed_seconds: self.started_at.elapsed().as_secs_f64(),
            directories_listed: self.directories_listed.load(Ordering::Relaxed),
            files_measured: self.files_measured.load(Ordering::Relaxed),
            bytes_measured: self.bytes_measured.load(Ordering::Relaxed),
            unreadable_directories: self.unreadable_directories.load(Ordering::Relaxed),
            permission_denied: self.permission_denied.load(Ordering::Relaxed),
            dense_directories: self.dense_directories.load(Ordering::Relaxed),
            hard_links_skipped: self.hard_links_skipped.load(Ordering::Relaxed),
            busy_workers: self.busy_workers.load(Ordering::Relaxed),
            worker_count: self.worker_count.load(Ordering::Relaxed),
        }
    }
}
