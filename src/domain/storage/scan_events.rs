//! Events emitted by scanner threads and applied by the tree owner.

use std::ffi::OsString;
use std::path::PathBuf;

use super::byte_size::MeasuredSize;
use super::entry_kind::EntryKind;
use super::file_tree::{BoundaryReason, ChildDirectory};
use super::heaviest_files::HeavyFile;
use super::tree_node::NodeRef;
use crate::domain::cleaning::CleaningCategory;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SearchId(pub u64);

/// Why a directory could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessProblem {
    PermissionDenied,
    NotFound,
    NotADirectory,
    TooDeep,
    Other(String),
}

impl AccessProblem {
    pub const fn is_permission_denied(&self) -> bool {
        matches!(self, Self::PermissionDenied)
    }

    pub fn describe(&self) -> String {
        match self {
            Self::PermissionDenied => "permission denied".to_owned(),
            Self::NotFound => "not found".to_owned(),
            Self::NotADirectory => "not a directory".to_owned(),
            Self::TooDeep => "deeper than the depth limit".to_owned(),
            Self::Other(reason) => reason.clone(),
        }
    }
}

/// Whether a measurement describes the target directory itself or an anonymous
/// descendant whose bytes are attributed to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attribution {
    Own,
    Anonymous,
}

#[derive(Clone, Debug)]
pub struct DirectoryMeasurement {
    pub target: NodeRef,
    pub attribution: Attribution,
    pub own_size: MeasuredSize,
    pub file_count: u64,
    pub entry_count: u64,
    pub materialized_children: Vec<ChildDirectory>,
    pub boundary_children: Vec<(ChildDirectory, BoundaryReason)>,
    /// Subdirectories that were scheduled anonymously (dense or beyond materialise depth).
    pub anonymous_subdirectory_count: u32,
    /// Subdirectories that were skipped entirely because of the depth limit.
    pub depth_limited_subdirectory_count: u32,
    pub dense: bool,
    pub problem: Option<AccessProblem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedFile {
    pub name: OsString,
    pub size: MeasuredSize,
    pub kind: EntryKind,
}

#[derive(Clone, Debug, Default)]
pub struct FileListing {
    /// Largest first, at most the requested limit.
    pub files: Vec<ListedFile>,
    pub total_file_count: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredTarget {
    pub category: CleaningCategory,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum ScanEvent {
    DirectoryMeasured(DirectoryMeasurement),
    FilesListed {
        node: NodeRef,
        request: RequestId,
        listing: FileListing,
        problem: Option<AccessProblem>,
    },
    HeaviestFilesProgress {
        query: QueryId,
        files: Vec<HeavyFile>,
        files_seen: u64,
        /// Tasks spawned for subdirectories. The query has one more outstanding unit per
        /// spawned task and one fewer for the task that produced this event.
        spawned: u32,
    },
    DiscoveryProgress {
        search: SearchId,
        found: Vec<DiscoveredTarget>,
        spawned: u32,
    },
}
