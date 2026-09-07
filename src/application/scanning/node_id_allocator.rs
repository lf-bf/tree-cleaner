//! Hands out node ids to scanner threads without any coordination with the tree owner.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::domain::storage::NodeId;

#[derive(Clone, Debug, Default)]
pub struct NodeIdAllocator {
    next: Arc<AtomicU32>,
}

impl NodeIdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allocate(&self) -> NodeId {
        NodeId(self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// Allocates `count` consecutive ids in one atomic step.
    pub fn allocate_many(&self, count: u32) -> impl Iterator<Item = NodeId> {
        let first = self.next.fetch_add(count, Ordering::Relaxed);
        (first..first.saturating_add(count)).map(NodeId)
    }

    pub fn allocated_so_far(&self) -> u32 {
        self.next.load(Ordering::Relaxed)
    }
}
