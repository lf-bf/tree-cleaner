//! A directory inside the [`FileTree`](super::file_tree::FileTree).

use std::ffi::OsStr;

use super::byte_size::MeasuredSize;

/// Stable identity of a node inside the tree. Ids are handed out by a monotonic allocator
/// shared with the scanner threads and are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// A node id together with the generation of the node when the reference was taken.
/// The generation changes whenever a node is reset for a rescan, so events produced by an
/// older scan of the same directory can be recognised and ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub id: NodeId,
    pub generation: u32,
}

/// Situations the scanner ran into while measuring a directory.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeFlags {
    /// The directory holds more entries than the dense threshold. Its children are
    /// measured in aggregate and never materialised individually.
    pub dense: bool,
    /// The directory itself could not be opened because of missing permissions.
    pub access_denied: bool,
    /// The directory could not be read for another reason.
    pub read_failed: bool,
    /// Some descendants exist beyond the materialisation depth. Entering this node
    /// requires a scan rooted at it.
    pub has_unmaterialized_children: bool,
    /// Descendants beyond the maximum depth were not visited at all.
    pub depth_limited: bool,
    /// The directory is a mount point (or a configured skip path) and was not entered.
    pub boundary: bool,
}

impl NodeFlags {
    pub const fn has_problem(self) -> bool {
        self.access_denied || self.read_failed
    }
}

/// Aggregate knowledge about one directory.
#[derive(Debug)]
pub struct TreeNode {
    pub(super) name: Box<OsStr>,
    pub(super) parent: Option<NodeId>,
    pub(super) children: Vec<NodeId>,
    pub(super) depth: u16,
    /// Bytes of files directly inside the directory, plus everything attributed to it by
    /// descendants that were not materialised.
    pub(super) own_size: MeasuredSize,
    /// `own_size` plus the total size of every materialised child.
    pub(super) total_size: MeasuredSize,
    pub(super) own_file_count: u64,
    pub(super) total_file_count: u64,
    pub(super) total_directory_count: u64,
    /// Number of unfinished pieces of work (own listing task, pending children,
    /// anonymous descendant tasks) that must finish before this node is complete.
    pub(super) outstanding_work: u32,
    /// Descendants that were attributed to this node and could not be read.
    pub(super) unreadable_descendants: u32,
    pub(super) flags: NodeFlags,
    pub(super) generation: u32,
}

impl TreeNode {
    pub(super) fn new(name: Box<OsStr>, parent: Option<NodeId>, depth: u16) -> Self {
        Self {
            name,
            parent,
            children: Vec::new(),
            depth,
            own_size: MeasuredSize::ZERO,
            total_size: MeasuredSize::ZERO,
            own_file_count: 0,
            total_file_count: 0,
            total_directory_count: 0,
            outstanding_work: 1,
            unreadable_descendants: 0,
            flags: NodeFlags::default(),
            generation: 0,
        }
    }

    pub const fn generation(&self) -> u32 {
        self.generation
    }

    pub fn name(&self) -> &OsStr {
        &self.name
    }

    pub fn display_name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }

    pub const fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub const fn depth(&self) -> u16 {
        self.depth
    }

    pub const fn own_size(&self) -> MeasuredSize {
        self.own_size
    }

    pub const fn total_size(&self) -> MeasuredSize {
        self.total_size
    }

    pub const fn own_file_count(&self) -> u64 {
        self.own_file_count
    }

    pub const fn total_file_count(&self) -> u64 {
        self.total_file_count
    }

    pub const fn total_directory_count(&self) -> u64 {
        self.total_directory_count
    }

    pub const fn total_item_count(&self) -> u64 {
        self.total_file_count + self.total_directory_count
    }

    pub const fn flags(&self) -> NodeFlags {
        self.flags
    }

    pub const fn unreadable_descendants(&self) -> u32 {
        self.unreadable_descendants
    }

    pub const fn is_complete(&self) -> bool {
        self.outstanding_work == 0
    }

    pub const fn outstanding_work(&self) -> u32 {
        self.outstanding_work
    }
}
