//! Contract for removing files and directories as the current user.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::domain::deletion::DeletionMode;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RemovalError {
    #[error("permission denied")]
    PermissionDenied,
    #[error("not found")]
    NotFound,
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl RemovalError {
    pub fn from_io(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::NotFound => Self::NotFound,
            _ => match error.raw_os_error() {
                Some(1) | Some(13) => Self::PermissionDenied,
                Some(2) => Self::NotFound,
                _ => Self::Other(error.to_string()),
            },
        }
    }

    pub const fn needs_privileges(&self) -> bool {
        matches!(self, Self::PermissionDenied)
    }
}

/// Told about every entry that disappears, so callers can show progress and cancel.
pub trait RemovalObserver {
    fn entry_removed(&mut self, path: &Path, bytes: u64);

    /// Polled regularly. Returning true aborts the removal after the current entry.
    fn should_cancel(&self) -> bool {
        false
    }
}

/// For callers that do not care about progress.
#[derive(Debug, Default)]
pub struct SilentObserver;

impl RemovalObserver for SilentObserver {
    fn entry_removed(&mut self, _path: &Path, _bytes: u64) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemovalSummary {
    pub entries_removed: u64,
    pub bytes_removed: u64,
}

impl RemovalSummary {
    pub fn absorb(&mut self, other: Self) {
        self.entries_removed = self.entries_removed.saturating_add(other.entries_removed);
        self.bytes_removed = self.bytes_removed.saturating_add(other.bytes_removed);
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContentsRemoval {
    pub removed: usize,
    pub failed: Vec<(PathBuf, RemovalError)>,
    pub summary: RemovalSummary,
}

pub trait FileRemover: Send + Sync {
    /// Removes a file or a whole directory, reporting every entry to `observer`.
    fn remove(
        &self,
        path: &Path,
        mode: DeletionMode,
        observer: &mut dyn RemovalObserver,
    ) -> Result<RemovalSummary, RemovalError>;

    /// Removes every entry inside `path`, keeping `path` itself.
    fn remove_contents(
        &self,
        path: &Path,
        mode: DeletionMode,
        observer: &mut dyn RemovalObserver,
    ) -> Result<ContentsRemoval, RemovalError>;
}
