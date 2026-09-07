//! Contract for enumerating one directory as cheaply as the platform allows.

use std::ffi::OsStr;
use std::path::Path;

use thiserror::Error;

use crate::domain::storage::{AccessProblem, EntryKind, FileMetadata};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DirectoryReadError {
    #[error("permission denied")]
    PermissionDenied,
    #[error("not found")]
    NotFound,
    #[error("not a directory")]
    NotADirectory,
    #[error("{0}")]
    Other(String),
}

impl DirectoryReadError {
    pub fn from_io(error: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        match error.kind() {
            ErrorKind::PermissionDenied => Self::PermissionDenied,
            ErrorKind::NotFound => Self::NotFound,
            ErrorKind::NotADirectory => Self::NotADirectory,
            _ => match error.raw_os_error() {
                // EPERM is what macOS returns for TCC protected folders.
                Some(1) | Some(13) => Self::PermissionDenied,
                Some(2) => Self::NotFound,
                Some(20) => Self::NotADirectory,
                _ => Self::Other(error.to_string()),
            },
        }
    }

    pub fn to_access_problem(&self) -> AccessProblem {
        match self {
            Self::PermissionDenied => AccessProblem::PermissionDenied,
            Self::NotFound => AccessProblem::NotFound,
            Self::NotADirectory => AccessProblem::NotADirectory,
            Self::Other(reason) => AccessProblem::Other(reason.clone()),
        }
    }
}

/// Whether to keep iterating a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    Continue,
    Stop,
}

/// One entry, exposed lazily: the kind usually comes for free from the directory stream,
/// the metadata costs a system call and is only fetched when asked for.
pub trait DirectoryEntryAccess {
    fn file_name(&self) -> &OsStr;

    /// Kind as reported by the directory stream, `None` when the filesystem does not say.
    fn kind_hint(&self) -> Option<EntryKind>;

    /// `lstat` of the entry. Never follows symbolic links.
    fn metadata(&self) -> Result<FileMetadata, DirectoryReadError>;
}

pub trait DirectoryReader: Send + Sync {
    /// Calls `visitor` for every entry except `.` and `..`.
    fn read_directory(
        &self,
        path: &Path,
        visitor: &mut dyn FnMut(&dyn DirectoryEntryAccess) -> Visit,
    ) -> Result<(), DirectoryReadError>;

    /// Short name of the implementation, shown in developer statistics.
    fn name(&self) -> &'static str;
}
