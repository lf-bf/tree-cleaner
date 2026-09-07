//! A set of candidates the user agreed to clean, and what happened to them.

use super::cleaning_candidate::{CandidateId, CleaningCandidate};
use crate::domain::storage::ByteSize;

#[derive(Clone, Debug, Default)]
pub struct CleaningPlan {
    pub candidates: Vec<CleaningCandidate>,
}

impl CleaningPlan {
    pub fn from_actionable(candidates: &[CleaningCandidate]) -> Self {
        Self {
            candidates: candidates.iter().filter(|candidate| candidate.is_actionable()).cloned().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn estimated_reclaim(&self) -> ByteSize {
        self.candidates
            .iter()
            .filter_map(|candidate| candidate.size)
            .fold(ByteSize::ZERO, ByteSize::saturating_add)
    }

    pub fn requires_privileges(&self) -> bool {
        self.candidates.iter().any(|candidate| candidate.requires_privileges)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleaningStatus {
    Cleaned,
    NeedsPrivileges,
    Skipped(String),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct CleaningOutcome {
    pub candidate: CandidateId,
    pub label: String,
    pub status: CleaningStatus,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct CleaningReport {
    pub outcomes: Vec<CleaningOutcome>,
    pub reclaimed_estimate: ByteSize,
}

impl CleaningReport {
    pub fn cleaned_count(&self) -> usize {
        self.outcomes.iter().filter(|outcome| outcome.status == CleaningStatus::Cleaned).count()
    }

    pub fn failed_count(&self) -> usize {
        self.outcomes.iter().filter(|outcome| matches!(outcome.status, CleaningStatus::Failed(_))).count()
    }

    pub fn needing_privileges(&self) -> Vec<CandidateId> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == CleaningStatus::NeedsPrivileges)
            .map(|outcome| outcome.candidate)
            .collect()
    }
}
