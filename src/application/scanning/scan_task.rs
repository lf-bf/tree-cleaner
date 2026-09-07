//! Units of work executed by scanner threads.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use crate::application::cleaning::discovery_rules::DiscoveryRules;
use crate::domain::storage::{NodeRef, QueryId, RequestId, SearchId, SizeMode};

/// Lists one directory, measures its files and schedules its subdirectories.
#[derive(Clone, Debug)]
pub struct MeasureDirectoryTask {
    /// Node that receives the results. For anonymous tasks this is the nearest
    /// materialised ancestor, not the directory being listed.
    pub target: NodeRef,
    pub path: PathBuf,
    /// Levels below the directory where the current scan started.
    pub depth_from_origin: u16,
    /// Levels below the root of the tree, for the absolute depth limit.
    pub tree_depth: u16,
    /// Whether `path` is the directory of `target` itself.
    pub describes_target: bool,
    /// Whether children may become nodes of their own.
    pub materialize_children: bool,
    /// Ignore the dense threshold for this directory (forced expansion).
    pub ignore_density: bool,
}

/// Lists the files directly inside one directory, keeping the largest ones.
#[derive(Clone, Debug)]
pub struct ListFilesTask {
    pub node: NodeRef,
    pub path: PathBuf,
    pub request: RequestId,
    pub limit: usize,
    pub size_mode: SizeMode,
}

/// Walks a subtree keeping the `limit` largest files, spawning one task per subdirectory.
#[derive(Clone, Debug)]
pub struct CollectHeaviestFilesTask {
    pub query: QueryId,
    pub path: PathBuf,
    pub limit: usize,
    pub size_mode: SizeMode,
    pub tree_depth: u16,
    /// Smallest size still worth reporting. Updated by the tree owner as the global top
    /// list fills up so workers can skip files that could never make the cut.
    pub floor: Arc<AtomicU64>,
}

/// Looks for directories the cleaner is interested in.
#[derive(Clone, Debug)]
pub struct DiscoverTask {
    pub search: SearchId,
    pub path: PathBuf,
    pub depth: u16,
    pub rules: Arc<DiscoveryRules>,
}

#[derive(Clone, Debug)]
pub enum ScanTask {
    MeasureDirectory(MeasureDirectoryTask),
    ListFiles(ListFilesTask),
    CollectHeaviestFiles(CollectHeaviestFilesTask),
    Discover(DiscoverTask),
}

impl ScanTask {
    pub fn path(&self) -> &Path {
        match self {
            Self::MeasureDirectory(task) => &task.path,
            Self::ListFiles(task) => &task.path,
            Self::CollectHeaviestFiles(task) => &task.path,
            Self::Discover(task) => &task.path,
        }
    }

    /// Interactive requests jump the queue regardless of the focus.
    pub const fn is_urgent(&self) -> bool {
        matches!(self, Self::ListFiles(_) | Self::CollectHeaviestFiles(_))
    }
}
