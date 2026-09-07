//! Arena-backed forest of directories with live, incrementally aggregated sizes.
//!
//! The tree is owned by a single thread. Scanner threads never touch it: they emit events
//! that the owner applies. Ids are allocated by the scanner threads so that a directory can
//! enqueue work for its children without a round trip through the owner.

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use super::byte_size::{MeasuredSize, SizeMode};
use super::tree_node::{NodeFlags, NodeId, NodeRef, TreeNode};

/// Why a root was added to the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootPurpose {
    /// A place the user explores (a volume, `/`, the home directory).
    Filesystem,
    /// A directory measured on behalf of the cleaner. Hidden from the explorer.
    Measurement,
}

#[derive(Clone, Debug)]
pub struct TreeRoot {
    pub node: NodeId,
    pub path: PathBuf,
    pub purpose: RootPurpose,
}

/// Why a child directory was materialised without being entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryReason {
    MountPoint,
    ConfiguredSkip,
    /// The directory is already being measured as a root of its own (for example the
    /// directory the explorer opened at startup) and gets grafted instead of rescanned.
    OtherRoot,
}

/// A subdirectory discovered by the scanner, with the id it was assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildDirectory {
    pub id: NodeId,
    pub name: OsString,
}

/// What one listing task learned about a directory, expressed as increments.
#[derive(Clone, Copy, Debug, Default)]
pub struct MeasurementDelta {
    pub own_size: MeasuredSize,
    pub file_count: u64,
    pub directory_count: u64,
    pub dense: bool,
    pub depth_limited: bool,
    pub has_unmaterialized_children: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeStatistics {
    pub live_nodes: usize,
    pub roots: usize,
}

#[derive(Debug, Default)]
pub struct FileTree {
    slots: Vec<Option<TreeNode>>,
    roots: Vec<TreeRoot>,
    live_nodes: usize,
}

impl FileTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn statistics(&self) -> TreeStatistics {
        TreeStatistics { live_nodes: self.live_nodes, roots: self.roots.len() }
    }

    pub fn roots(&self) -> &[TreeRoot] {
        &self.roots
    }

    pub fn roots_with_purpose(&self, purpose: RootPurpose) -> impl Iterator<Item = &TreeRoot> {
        self.roots.iter().filter(move |root| root.purpose == purpose)
    }

    pub fn root_by_path(&self, path: &Path) -> Option<&TreeRoot> {
        self.roots.iter().find(|root| root.path == path)
    }

    pub fn node(&self, id: NodeId) -> Option<&TreeNode> {
        self.slots.get(id.index()).and_then(Option::as_ref)
    }

    fn node_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.slots.get_mut(id.index()).and_then(Option::as_mut)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.node(id).is_some()
    }

    /// Current reference to a node, or `None` if it no longer exists.
    pub fn reference(&self, id: NodeId) -> Option<NodeRef> {
        self.node(id).map(|node| NodeRef { id, generation: node.generation })
    }

    /// The node behind a reference, provided it was not reset since the reference was taken.
    pub fn resolve(&self, reference: NodeRef) -> Option<&TreeNode> {
        self.node(reference.id).filter(|node| node.generation == reference.generation)
    }

    pub fn is_current(&self, reference: NodeRef) -> bool {
        self.resolve(reference).is_some()
    }

    // ----------------------------------------------------------------------------------
    // Structure
    // ----------------------------------------------------------------------------------

    pub fn insert_root(&mut self, id: NodeId, path: PathBuf, purpose: RootPurpose) -> NodeRef {
        let name: OsString = match path.file_name() {
            Some(name) => name.to_os_string(),
            None => path.as_os_str().to_os_string(),
        };
        self.place(id, TreeNode::new(name.into_boxed_os_str(), None, 0));
        self.roots.push(TreeRoot { node: id, path, purpose });
        NodeRef { id, generation: 0 }
    }

    /// Adds children that will be measured. Each child accounts for one unit of the
    /// parent's outstanding work until it completes.
    pub fn insert_children(&mut self, parent: NodeId, children: Vec<ChildDirectory>) {
        let Some(parent_node) = self.node(parent) else {
            return;
        };
        let depth = parent_node.depth.saturating_add(1);
        let count = children.len() as u32;
        for child in children {
            self.place(child.id, TreeNode::new(child.name.into_boxed_os_str(), Some(parent), depth));
            if let Some(parent_node) = self.node_mut(parent) {
                parent_node.children.push(child.id);
            }
        }
        if let Some(parent_node) = self.node_mut(parent) {
            parent_node.outstanding_work = parent_node.outstanding_work.saturating_add(count);
        }
    }

    /// Adds children that will never be entered (other volumes, configured skips). They are
    /// complete from the start and contribute nothing to the parent's size.
    pub fn insert_boundary_children(
        &mut self,
        parent: NodeId,
        children: Vec<ChildDirectory>,
        reason: BoundaryReason,
    ) {
        let Some(parent_node) = self.node(parent) else {
            return;
        };
        let depth = parent_node.depth.saturating_add(1);
        for child in children {
            let mut node = TreeNode::new(child.name.into_boxed_os_str(), Some(parent), depth);
            node.outstanding_work = 0;
            node.flags.boundary = true;
            let _ = reason;
            self.place(child.id, node);
            if let Some(parent_node) = self.node_mut(parent) {
                parent_node.children.push(child.id);
            }
        }
    }

    /// Attaches an existing root below `parent`, so the subtree measured separately
    /// becomes part of the bigger tree without being scanned twice.
    pub fn graft_root(&mut self, root: NodeId, parent: NodeId) -> bool {
        let Some(position) = self.roots.iter().position(|entry| entry.node == root) else {
            return false;
        };
        let Some(parent_node) = self.node(parent) else {
            return false;
        };
        let parent_depth = parent_node.depth;
        let Some(root_node) = self.node(root) else {
            return false;
        };
        let was_complete = root_node.is_complete();
        let total_size = root_node.total_size;
        let total_files = root_node.total_file_count;
        let total_directories = root_node.total_directory_count;
        self.roots.remove(position);
        if let Some(root_node) = self.node_mut(root) {
            root_node.parent = Some(parent);
        }
        if let Some(parent_node) = self.node_mut(parent) {
            parent_node.children.push(root);
            if !was_complete {
                parent_node.outstanding_work = parent_node.outstanding_work.saturating_add(1);
            }
        }
        self.shift_depths(root, parent_depth.saturating_add(1));
        self.propagate_totals(parent, total_size, total_files, total_directories.saturating_add(1));
        true
    }

    fn shift_depths(&mut self, start: NodeId, depth: u16) {
        let mut stack = vec![(start, depth)];
        while let Some((id, depth)) = stack.pop() {
            let Some(node) = self.node_mut(id) else {
                continue;
            };
            node.depth = depth;
            let child_depth = depth.saturating_add(1);
            stack.extend(node.children.iter().map(|child| (*child, child_depth)));
        }
    }

    fn place(&mut self, id: NodeId, node: TreeNode) {
        let index = id.index();
        if index >= self.slots.len() {
            self.slots.resize_with(index + 1, || None);
        }
        if self.slots[index].is_none() {
            self.live_nodes += 1;
        }
        self.slots[index] = Some(node);
    }

    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let mut names: Vec<&OsStr> = Vec::new();
        let mut current = id;
        while let Some(node) = self.node(current) {
            match node.parent {
                Some(parent) => {
                    names.push(&node.name);
                    current = parent;
                }
                None => {
                    let mut path = self
                        .roots
                        .iter()
                        .find(|root| root.node == current)
                        .map(|root| root.path.clone())
                        .unwrap_or_else(|| PathBuf::from(&*node.name));
                    for name in names.iter().rev() {
                        path.push(name);
                    }
                    return path;
                }
            }
        }
        PathBuf::new()
    }

    pub fn root_of(&self, id: NodeId) -> Option<&TreeRoot> {
        let mut current = id;
        loop {
            let node = self.node(current)?;
            match node.parent {
                Some(parent) => current = parent,
                None => return self.roots.iter().find(|root| root.node == current),
            }
        }
    }

    /// Ancestors from the parent upwards to the root.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut current = self.node(id).and_then(|node| node.parent);
        while let Some(ancestor) = current {
            result.push(ancestor);
            current = self.node(ancestor).and_then(|node| node.parent);
        }
        result
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.node(id).map(|node| node.children.as_slice()).unwrap_or(&[])
    }

    /// Children ordered from the largest to the smallest.
    pub fn children_sorted_by_size(&self, id: NodeId, mode: SizeMode) -> Vec<NodeId> {
        let mut children: Vec<NodeId> = self.children(id).to_vec();
        children.sort_by(|left, right| {
            let left_size = self.node(*left).map(|node| node.total_size.select(mode));
            let right_size = self.node(*right).map(|node| node.total_size.select(mode));
            right_size.cmp(&left_size).then_with(|| {
                let left_name = self.node(*left).map(|node| node.name.clone());
                let right_name = self.node(*right).map(|node| node.name.clone());
                left_name.cmp(&right_name)
            })
        });
        children
    }

    pub fn child_named(&self, parent: NodeId, name: &OsStr) -> Option<NodeId> {
        self.children(parent)
            .iter()
            .copied()
            .find(|child| self.node(*child).is_some_and(|node| &*node.name == name))
    }

    /// The node for exactly `path`, if it is materialised.
    pub fn locate(&self, path: &Path) -> Option<NodeId> {
        let (node, remaining) = self.locate_nearest(path)?;
        remaining.is_empty().then_some(node)
    }

    /// The deepest materialised node along `path`, plus the components that were not found.
    pub fn locate_nearest(&self, path: &Path) -> Option<(NodeId, Vec<OsString>)> {
        let root = self
            .roots
            .iter()
            .filter(|root| path.starts_with(&root.path))
            .max_by_key(|root| root.path.as_os_str().len())?;
        let relative = path.strip_prefix(&root.path).ok()?;
        let mut current = root.node;
        let mut components = relative.components().filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_os_string()),
            _ => None,
        });
        let mut remaining = Vec::new();
        for component in components.by_ref() {
            match self.child_named(current, &component) {
                Some(child) => current = child,
                None => {
                    remaining.push(component);
                    break;
                }
            }
        }
        remaining.extend(components);
        Some((current, remaining))
    }

    /// Every node below `id`, not including `id` itself.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut stack: Vec<NodeId> = self.children(id).to_vec();
        while let Some(current) = stack.pop() {
            result.push(current);
            stack.extend_from_slice(self.children(current));
        }
        result
    }

    // ----------------------------------------------------------------------------------
    // Measurement
    // ----------------------------------------------------------------------------------

    /// Applies what one listing task learned. Does not touch outstanding work; callers pair
    /// it with [`Self::add_outstanding_work`] and [`Self::finish_work_unit`].
    pub fn record_measurement(&mut self, target: NodeId, delta: MeasurementDelta) {
        let Some(node) = self.node_mut(target) else {
            return;
        };
        node.own_size += delta.own_size;
        node.own_file_count = node.own_file_count.saturating_add(delta.file_count);
        node.flags.dense |= delta.dense;
        node.flags.depth_limited |= delta.depth_limited;
        node.flags.has_unmaterialized_children |= delta.has_unmaterialized_children;
        self.propagate_totals(target, delta.own_size, delta.file_count, delta.directory_count);
    }

    /// Files directly inside `target` were deleted: shrink it and every ancestor.
    pub fn forget_files(&mut self, target: NodeId, size: MeasuredSize, count: u64) {
        let Some(node) = self.node_mut(target) else {
            return;
        };
        node.own_size = node.own_size.saturating_sub(size);
        node.own_file_count = node.own_file_count.saturating_sub(count);
        self.subtract_from_ancestors(Some(target), size, count, 0);
    }

    pub fn record_problem(&mut self, target: NodeId, own: bool, denied: bool) {
        if let Some(node) = self.node_mut(target) {
            if own {
                node.flags.access_denied |= denied;
                node.flags.read_failed |= !denied;
            } else {
                node.unreadable_descendants = node.unreadable_descendants.saturating_add(1);
            }
        }
    }

    pub fn add_outstanding_work(&mut self, target: NodeId, units: u32) {
        if let Some(node) = self.node_mut(target) {
            node.outstanding_work = node.outstanding_work.saturating_add(units);
        }
    }

    /// One unit of work for `target` finished. Completion bubbles up while ancestors have
    /// nothing else pending.
    pub fn finish_work_unit(&mut self, target: NodeId) {
        let mut current = Some(target);
        while let Some(id) = current {
            let Some(node) = self.node_mut(id) else {
                break;
            };
            node.outstanding_work = node.outstanding_work.saturating_sub(1);
            if node.outstanding_work == 0 {
                current = node.parent;
            } else {
                break;
            }
        }
    }

    fn propagate_totals(&mut self, start: NodeId, size: MeasuredSize, files: u64, directories: u64) {
        let mut current = Some(start);
        while let Some(id) = current {
            let Some(node) = self.node_mut(id) else {
                break;
            };
            node.total_size += size;
            node.total_file_count = node.total_file_count.saturating_add(files);
            node.total_directory_count = node.total_directory_count.saturating_add(directories);
            current = node.parent;
        }
    }

    fn subtract_from_ancestors(
        &mut self,
        start: Option<NodeId>,
        size: MeasuredSize,
        files: u64,
        directories: u64,
    ) {
        let mut current = start;
        while let Some(id) = current {
            let Some(node) = self.node_mut(id) else {
                break;
            };
            node.total_size = node.total_size.saturating_sub(size);
            node.total_file_count = node.total_file_count.saturating_sub(files);
            node.total_directory_count = node.total_directory_count.saturating_sub(directories);
            current = node.parent;
        }
    }

    // ----------------------------------------------------------------------------------
    // Rescan and removal
    // ----------------------------------------------------------------------------------

    /// Forgets everything known about `id` and its descendants and marks it pending again.
    /// Returns the new reference that future events must carry.
    pub fn reset_for_rescan(&mut self, id: NodeId) -> Option<NodeRef> {
        let node = self.node(id)?;
        let was_complete = node.is_complete();
        let parent = node.parent;
        let removed_size = node.total_size;
        let removed_files = node.total_file_count;
        let removed_directories = node.total_directory_count;
        let children = node.children.clone();
        for child in children {
            self.tombstone_subtree(child);
        }
        let node = self.node_mut(id)?;
        node.children.clear();
        node.own_size = MeasuredSize::ZERO;
        node.total_size = MeasuredSize::ZERO;
        node.own_file_count = 0;
        node.total_file_count = 0;
        node.total_directory_count = 0;
        node.unreadable_descendants = 0;
        node.flags = NodeFlags::default();
        node.outstanding_work = 1;
        node.generation = node.generation.wrapping_add(1);
        let generation = node.generation;
        self.subtract_from_ancestors(parent, removed_size, removed_files, removed_directories);
        if was_complete {
            self.reopen_ancestors(parent);
        }
        Some(NodeRef { id, generation })
    }

    /// A completed node became pending again: every ancestor that had nothing else
    /// outstanding now has one unit of work pending.
    fn reopen_ancestors(&mut self, start: Option<NodeId>) {
        let mut current = start;
        while let Some(id) = current {
            let Some(node) = self.node_mut(id) else {
                break;
            };
            let was_complete = node.outstanding_work == 0;
            node.outstanding_work = node.outstanding_work.saturating_add(1);
            if was_complete {
                current = node.parent;
            } else {
                break;
            }
        }
    }

    /// Removes a directory (for example after deleting it) and adjusts every ancestor.
    pub fn remove_subtree(&mut self, id: NodeId) {
        let Some(node) = self.node(id) else {
            return;
        };
        let parent = node.parent;
        let was_complete = node.is_complete();
        let removed_size = node.total_size;
        let removed_files = node.total_file_count;
        let removed_directories = node.total_directory_count.saturating_add(1);
        self.subtract_from_ancestors(parent, removed_size, removed_files, removed_directories);
        self.tombstone_subtree(id);
        match parent {
            Some(parent_id) => {
                if let Some(parent_node) = self.node_mut(parent_id) {
                    parent_node.children.retain(|child| *child != id);
                }
                if !was_complete {
                    self.finish_work_unit(parent_id);
                }
            }
            None => self.roots.retain(|root| root.node != id),
        }
    }

    fn tombstone_subtree(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            let index = current.index();
            if let Some(slot) = self.slots.get_mut(index) {
                if let Some(node) = slot.take() {
                    self.live_nodes = self.live_nodes.saturating_sub(1);
                    stack.extend(node.children);
                }
            }
        }
    }
}
