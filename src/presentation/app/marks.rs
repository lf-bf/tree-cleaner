//! Items the user marked for deletion, across screens and directories.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::deletion::{DeletionItem, DeletionMode, DeletionPlan};
use crate::domain::storage::{ByteSize, NodeId};

#[derive(Clone, Debug)]
pub struct MarkedItem {
    pub size: ByteSize,
    pub is_directory: bool,
    /// Node of the directory itself (for directories) or of the containing directory (for files).
    pub node: Option<NodeId>,
    pub parent_node: Option<NodeId>,
}

#[derive(Debug, Default)]
pub struct Marks {
    items: BTreeMap<PathBuf, MarkedItem>,
}

impl Marks {
    pub fn toggle(&mut self, path: PathBuf, item: MarkedItem) -> bool {
        if self.items.remove(&path).is_some() {
            false
        } else {
            self.items.insert(path, item);
            true
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.items.contains_key(path)
    }

    pub fn remove(&mut self, path: &Path) {
        self.items.remove(path);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn total_size(&self) -> ByteSize {
        self.items.values().map(|item| item.size).fold(ByteSize::ZERO, ByteSize::saturating_add)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &MarkedItem)> {
        self.items.iter()
    }

    /// Refreshes sizes so the confirmation shows what the tree knows right now.
    pub fn update_size(&mut self, path: &Path, size: ByteSize) {
        if let Some(item) = self.items.get_mut(path) {
            item.size = size;
        }
    }

    pub fn plan(&self, mode: DeletionMode) -> DeletionPlan {
        let items = self
            .items
            .iter()
            .map(|(path, item)| DeletionItem {
                path: path.clone(),
                size: item.size,
                is_directory: item.is_directory,
                node: item.node,
            })
            .collect();
        DeletionPlan::new(items, mode)
    }

    pub fn parent_of(&self, path: &Path) -> Option<NodeId> {
        self.items.get(path).and_then(|item| item.parent_node)
    }
}
