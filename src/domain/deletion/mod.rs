//! Bounded context: removing what the user explicitly selected.

pub mod critical_paths;
pub mod deletion_plan;

pub use critical_paths::CriticalPathGuard;
pub use deletion_plan::{
    DeletionItem, DeletionMode, DeletionOutcome, DeletionPlan, DeletionReport, DeletionStatus,
};
