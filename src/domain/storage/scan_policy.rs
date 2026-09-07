//! Tunable rules that govern how aggressively the filesystem is walked.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ScanPolicy {
    /// Directories with more entries than this are considered dense: their contents are
    /// measured in aggregate and never listed individually.
    pub dense_directory_threshold: usize,
    /// How many levels below the scan origin get their own node. Deeper directories are
    /// summed into the nearest materialised ancestor and expanded lazily on demand.
    pub materialize_depth: u16,
    /// Absolute safety limit; nothing deeper than this is visited.
    pub max_depth: u16,
    /// Paths that are never entered (virtual filesystems, firmlink duplicates, ...).
    pub skip_paths: Vec<PathBuf>,
    /// Whether to descend into directories that are mount points of other volumes.
    pub cross_mount_points: bool,
    /// Count files with several hard links only once.
    pub deduplicate_hard_links: bool,
    /// How many of the largest files to keep when listing one directory on demand.
    pub file_listing_limit: usize,
}

impl Default for ScanPolicy {
    fn default() -> Self {
        Self {
            dense_directory_threshold: 20_000,
            materialize_depth: 10,
            max_depth: 256,
            skip_paths: default_skip_paths(),
            cross_mount_points: false,
            deduplicate_hard_links: true,
            file_listing_limit: 1_000,
        }
    }
}

impl ScanPolicy {
    pub fn should_skip(&self, path: &Path) -> bool {
        self.skip_paths.iter().any(|skipped| skipped == path)
    }

    pub fn is_dense(&self, entry_count: usize) -> bool {
        entry_count > self.dense_directory_threshold
    }
}

/// Paths that would either double count storage or hang the scan.
pub fn default_skip_paths() -> Vec<PathBuf> {
    [
        // macOS: the data volume is already reachable through firmlinks from `/`.
        "/System/Volumes/Data",
        // macOS: swap files live on their own volume.
        "/private/var/vm",
        // Virtual filesystems.
        "/dev",
        "/proc",
        "/sys",
        "/run",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}
