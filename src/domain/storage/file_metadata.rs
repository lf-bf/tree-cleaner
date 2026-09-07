//! Metadata of a single filesystem entry, as obtained from one `lstat` call.

use super::byte_size::MeasuredSize;
use super::entry_kind::EntryKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    pub kind: EntryKind,
    pub size: MeasuredSize,
    /// Number of hard links pointing at the inode. Files with more than one are counted once.
    pub hard_link_count: u64,
    pub inode: u64,
    pub device: u64,
}

impl FileMetadata {
    /// Identity of the underlying inode, used to deduplicate hard links.
    pub const fn inode_identity(&self) -> InodeIdentity {
        InodeIdentity { device: self.device, inode: self.inode }
    }

    pub const fn has_multiple_hard_links(&self) -> bool {
        self.hard_link_count > 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InodeIdentity {
    pub device: u64,
    pub inode: u64,
}
