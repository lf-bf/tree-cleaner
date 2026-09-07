//! A deletion or cleaning run in progress: what the modal shows and how it is fed.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::application::cleaning::{CleaningHandle, CleaningProgress};
use crate::application::deletion::{DeletionHandle, DeletionProgress};
use crate::domain::cleaning::{CleaningReport, CleaningStatus};
use crate::domain::deletion::{DeletionReport, DeletionStatus};
use crate::presentation::formatting;

const RECENT_CAPACITY: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Deletion,
    Cleaning,
}

impl OperationKind {
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Deletion => "Deleting",
            Self::Cleaning => "Cleaning",
        }
    }

    /// `item` / `items`, `target` / `targets`.
    pub const fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (Self::Deletion, 1) => "item",
            (Self::Deletion, _) => "items",
            (Self::Cleaning, 1) => "target",
            (Self::Cleaning, _) => "targets",
        }
    }
}

pub enum OperationSource {
    Deletion(DeletionHandle),
    Cleaning(CleaningHandle),
}

#[derive(Clone, Debug)]
pub enum OperationReport {
    Deletion(DeletionReport),
    Cleaning(CleaningReport),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStatus {
    Success,
    Warning,
    Failure,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct FinishedEntry {
    pub label: String,
    pub detail: String,
    pub status: EntryStatus,
}

/// Everything the progress dialog needs. Updated by draining the source's channel.
pub struct OperationRun {
    pub kind: OperationKind,
    pub source: OperationSource,
    pub total: usize,
    pub finished_count: usize,
    pub current_index: Option<usize>,
    pub current_label: String,
    pub current_detail: String,
    pub current_entries: u64,
    pub current_bytes: u64,
    pub finished_entries: u64,
    pub finished_bytes: u64,
    pub recent: VecDeque<FinishedEntry>,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
    pub cancel_requested: bool,
    pub report: Option<OperationReport>,
    home: Option<PathBuf>,
}

impl OperationRun {
    pub fn new(kind: OperationKind, source: OperationSource, total: usize, home: Option<PathBuf>) -> Self {
        Self {
            kind,
            source,
            total,
            finished_count: 0,
            current_index: None,
            current_label: String::new(),
            current_detail: String::new(),
            current_entries: 0,
            current_bytes: 0,
            finished_entries: 0,
            finished_bytes: 0,
            recent: VecDeque::with_capacity(RECENT_CAPACITY),
            started_at: Instant::now(),
            finished_at: None,
            cancel_requested: false,
            report: None,
            home,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished_at.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        self.finished_at.unwrap_or_else(Instant::now).duration_since(self.started_at)
    }

    /// Fraction of items finished. Items are opaque, so progress moves in steps.
    pub fn ratio(&self) -> f64 {
        if self.total == 0 { 1.0 } else { (self.finished_count as f64 / self.total as f64).clamp(0.0, 1.0) }
    }

    pub fn bytes_reclaimed(&self) -> u64 {
        self.finished_bytes.saturating_add(self.current_bytes)
    }

    pub fn entries_removed(&self) -> u64 {
        self.finished_entries.saturating_add(self.current_entries)
    }

    pub fn request_cancel(&mut self) {
        if self.is_finished() {
            return;
        }
        self.cancel_requested = true;
        match &self.source {
            OperationSource::Deletion(handle) => handle.request_cancel(),
            OperationSource::Cleaning(handle) => handle.request_cancel(),
        }
    }

    pub fn title(&self) -> String {
        if self.is_finished() {
            format!("{} finished", self.kind.verb())
        } else {
            format!("{} {} {}", self.kind.verb(), self.total, self.kind.noun(self.total))
        }
    }

    fn label_for(&self, path: &Path) -> String {
        formatting::home_relative(path, self.home.as_deref())
    }

    fn push_recent(&mut self, entry: FinishedEntry) {
        if self.recent.len() >= RECENT_CAPACITY {
            self.recent.pop_front();
        }
        self.recent.push_back(entry);
    }

    fn finish_current(&mut self, entries: u64, bytes: u64) {
        self.finished_count = self.finished_count.saturating_add(1).min(self.total.max(1));
        self.finished_entries = self.finished_entries.saturating_add(entries);
        self.finished_bytes = self.finished_bytes.saturating_add(bytes);
        self.current_index = None;
        self.current_entries = 0;
        self.current_bytes = 0;
        self.current_detail.clear();
    }

    /// Drains every pending message. Returns the report when the run just finished.
    pub fn drain(&mut self) -> Option<OperationReport> {
        let mut finished = None;
        match &self.source {
            OperationSource::Deletion(handle) => {
                let messages: Vec<DeletionProgress> = handle.receiver.try_iter().collect();
                for message in messages {
                    if let Some(report) = self.apply_deletion(message) {
                        finished = Some(report);
                    }
                }
            }
            OperationSource::Cleaning(handle) => {
                let messages: Vec<CleaningProgress> = handle.receiver.try_iter().collect();
                for message in messages {
                    if let Some(report) = self.apply_cleaning(message) {
                        finished = Some(report);
                    }
                }
            }
        }
        finished
    }

    fn apply_deletion(&mut self, message: DeletionProgress) -> Option<OperationReport> {
        match message {
            DeletionProgress::Started { total } => self.total = total,
            DeletionProgress::ItemStarted { index, path } => {
                self.current_index = Some(index);
                self.current_label = self.label_for(&path);
                self.current_detail.clear();
                self.current_entries = 0;
                self.current_bytes = 0;
            }
            DeletionProgress::ItemProgress { entries_removed, bytes_removed, current, .. } => {
                self.current_entries = entries_removed;
                self.current_bytes = bytes_removed;
                self.current_detail = self.label_for(&current);
            }
            DeletionProgress::ItemFinished { outcome, .. } => {
                let label = self.label_for(&outcome.item.path);
                let (status, detail) = match &outcome.status {
                    DeletionStatus::Removed => (EntryStatus::Success, "removed".to_owned()),
                    DeletionStatus::MovedToTrash => (EntryStatus::Success, "moved to Trash".to_owned()),
                    DeletionStatus::NeedsPrivileges => (EntryStatus::Warning, "needs sudo".to_owned()),
                    DeletionStatus::Cancelled => (EntryStatus::Cancelled, "cancelled".to_owned()),
                    DeletionStatus::Refused(reason) => (EntryStatus::Failure, reason.clone()),
                    DeletionStatus::Failed(reason) => (EntryStatus::Failure, reason.clone()),
                };
                self.push_recent(FinishedEntry { label, detail, status });
                self.finish_current(outcome.entries_removed, outcome.reclaimed().as_u64());
            }
            DeletionProgress::Finished(report) => {
                self.finished_at = Some(Instant::now());
                self.finished_count = self.total;
                let report = OperationReport::Deletion(report);
                self.report = Some(report.clone());
                return Some(report);
            }
        }
        None
    }

    fn apply_cleaning(&mut self, message: CleaningProgress) -> Option<OperationReport> {
        match message {
            CleaningProgress::Started { total } => self.total = total,
            CleaningProgress::ItemStarted { index, label } => {
                self.current_index = Some(index);
                self.current_label = label;
                self.current_detail.clear();
                self.current_entries = 0;
                self.current_bytes = 0;
            }
            CleaningProgress::ItemProgress { entries_removed, bytes_removed, current, .. } => {
                self.current_entries = entries_removed;
                self.current_bytes = bytes_removed;
                self.current_detail = self.label_for(&current);
            }
            CleaningProgress::ItemFinished { outcome, .. } => {
                let (status, detail) = match &outcome.status {
                    CleaningStatus::Cleaned => (EntryStatus::Success, outcome.detail.clone()),
                    CleaningStatus::NeedsPrivileges => (EntryStatus::Warning, "needs sudo".to_owned()),
                    CleaningStatus::Cancelled => (EntryStatus::Cancelled, "cancelled".to_owned()),
                    CleaningStatus::Skipped(reason) => (EntryStatus::Warning, reason.clone()),
                    CleaningStatus::Failed(reason) => (EntryStatus::Failure, reason.clone()),
                };
                self.push_recent(FinishedEntry { label: outcome.label.clone(), detail, status });
                self.finish_current(outcome.entries_removed, outcome.reclaimed.as_u64());
            }
            CleaningProgress::Finished(report) => {
                self.finished_at = Some(Instant::now());
                self.finished_count = self.total;
                let report = OperationReport::Cleaning(report);
                self.report = Some(report.clone());
                return Some(report);
            }
        }
        None
    }
}
