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
    Refused(String),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct DeletionOutcome {
    pub item: DeletionItem,
    pub status: DeletionStatus,
}

#[derive(Clone, Debug, Default)]
pub struct DeletionReport {
    pub outcomes: Vec<DeletionOutcome>,
}

impl DeletionReport {
    pub fn succeeded(&self) -> impl Iterator<Item = &DeletionOutcome> {
        self.outcomes.iter().filter(|outcome| {
            matches!(outcome.status, DeletionStatus::Removed | DeletionStatus::MovedToTrash)
        })
    }

    pub fn reclaimed(&self) -> ByteSize {
        self.succeeded().map(|outcome| outcome.item.size).fold(ByteSize::ZERO, ByteSize::saturating_add)
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
