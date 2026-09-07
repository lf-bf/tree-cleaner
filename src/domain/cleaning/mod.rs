//! Bounded context: reclaiming storage that is safe to give back.

pub mod cleaning_candidate;
pub mod cleaning_category;
pub mod cleaning_plan;
pub mod protection_rules;

pub use cleaning_candidate::{CandidateId, CleaningCandidate, CleaningLocation, DockerPruneKind, Protection};
pub use cleaning_category::CleaningCategory;
pub use cleaning_plan::{CleaningOutcome, CleaningPlan, CleaningReport, CleaningStatus};
pub use protection_rules::{InvalidPattern, ProtectionRules};
