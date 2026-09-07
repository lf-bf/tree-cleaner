//! Explicit removal of things the user picked in the explorer.

use std::path::PathBuf;

use crate::domain::storage::{ByteSize, NodeId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeletionMode {
    /// `rm -rf` semantics. Space is freed immediately.
    #[default]
    Permanent,
    /// Move to the system Trash. Nothing is freed until the Trash is emptied.
    Trash,
}

impl DeletionMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Permanent => "delete permanently",
            Self::Trash => "move to Trash",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletionItem {
    pub path: PathBuf,
    pub size: ByteSize,
    pub is_directory: bool,
    /// The tree node describing the item, when the item is a materialised directory.
    pub node: Option<NodeId>,
}

#[derive(Clone, Debug, Default)]
pub struct DeletionPlan {
    pub items: Vec<DeletionItem>,
    pub mode: DeletionMode,
}

impl DeletionPlan {
    pub fn new(items: Vec<DeletionItem>, mode: DeletionMode) -> Self {
        Self { items, mode }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn total_size(&self) -> ByteSize {
        self.items.iter().map(|item| item.size).fold(ByteSize::ZERO, ByteSize::saturating_add)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeletionStatus {
    Removed,
    MovedToTrash,
    NeedsPrivileges,
    /// The user cancelled the run before or while this item was being removed.
    Cancelled,
    Refused(String),
    Failed(String),
}

impl DeletionStatus {
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Removed | Self::MovedToTrash)
    }
}

#[derive(Clone, Debug)]
pub struct DeletionOutcome {
    pub item: DeletionItem,
    pub status: DeletionStatus,
    /// Bytes actually freed, as observed while removing. Zero when nothing was measured.
    pub bytes_removed: ByteSize,
    pub entries_removed: u64,
}

impl DeletionOutcome {
    /// What the item gave back: the measured bytes when known, otherwise the estimate.
    pub fn reclaimed(&self) -> ByteSize {
        if !self.bytes_removed.is_zero() {
            self.bytes_removed
        } else if self.status.is_success() {
            self.item.size
        } else {
            ByteSize::ZERO
        }
    }

    /// Something was removed even though the item as a whole did not finish.
    pub fn is_partial(&self) -> bool {
        !self.status.is_success() && self.entries_removed > 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeletionReport {
    pub outcomes: Vec<DeletionOutcome>,
}

impl DeletionReport {
    pub fn succeeded(&self) -> impl Iterator<Item = &DeletionOutcome> {
        self.outcomes.iter().filter(|outcome| outcome.status.is_success())
    }

    pub fn reclaimed(&self) -> ByteSize {
        self.outcomes.iter().map(DeletionOutcome::reclaimed).fold(ByteSize::ZERO, ByteSize::saturating_add)
    }

    pub fn cancelled_count(&self) -> usize {
        self.outcomes.iter().filter(|outcome| outcome.status == DeletionStatus::Cancelled).count()
    }

    pub fn needing_privileges(&self) -> Vec<DeletionItem> {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == DeletionStatus::NeedsPrivileges)
            .map(|outcome| outcome.item.clone())
            .collect()
    }

    pub fn failures(&self) -> Vec<&DeletionOutcome> {
        self.outcomes
            .iter()
            .filter(|outcome| {
                matches!(outcome.status, DeletionStatus::Failed(_) | DeletionStatus::Refused(_))
            })
            .collect()
    }
}
