//! Executes deletion plans built in the explorer.

use std::sync::Arc;

use crossbeam_channel::Sender;

use crate::domain::deletion::{
    CriticalPathGuard, DeletionMode, DeletionOutcome, DeletionPlan, DeletionReport, DeletionStatus,
};
use crate::domain::ports::{FileRemover, RemovalError};

/// Progress messages sent while a plan runs on a background thread.
#[derive(Clone, Debug)]
pub enum DeletionProgress {
    Started { total: usize },
    ItemFinished(DeletionOutcome),
    Finished(DeletionReport),
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

    /// Runs the plan, reporting each item as it finishes.
    pub fn execute(&self, plan: &DeletionPlan, progress: &Sender<DeletionProgress>) -> DeletionReport {
        let _ = progress.send(DeletionProgress::Started { total: plan.items.len() });
        let mut report = DeletionReport::default();
        for item in &plan.items {
            let status = match self.guard.check(&item.path) {
                Err(reason) => DeletionStatus::Refused(reason),
                Ok(()) => match self.remover.remove(&item.path, plan.mode) {
                    Ok(()) => match plan.mode {
                        DeletionMode::Permanent => DeletionStatus::Removed,
                        DeletionMode::Trash => DeletionStatus::MovedToTrash,
                    },
                    Err(RemovalError::PermissionDenied) => DeletionStatus::NeedsPrivileges,
                    Err(RemovalError::NotFound) => DeletionStatus::Removed,
                    Err(RemovalError::Other(reason)) => DeletionStatus::Failed(reason),
                },
            };
            let outcome = DeletionOutcome { item: item.clone(), status };
            let _ = progress.send(DeletionProgress::ItemFinished(outcome.clone()));
            report.outcomes.push(outcome);
        }
        let _ = progress.send(DeletionProgress::Finished(report.clone()));
        report
    }

    /// Spawns the plan on its own thread; results arrive through the returned receiver.
    pub fn execute_in_background(&self, plan: DeletionPlan) -> crossbeam_channel::Receiver<DeletionProgress> {
        let (sender, receiver) = crossbeam_channel::unbounded();
        let service = self.clone();
        std::thread::Builder::new()
            .name("tree-cleaner-delete".to_owned())
            .spawn(move || {
                service.execute(&plan, &sender);
            })
            .expect("failed to spawn deletion thread");
        receiver
    }
}
