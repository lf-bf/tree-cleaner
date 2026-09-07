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

#[derive(Clone, Debug, Default)]
pub struct ContentsRemoval {
    pub removed: usize,
    pub failed: Vec<(PathBuf, RemovalError)>,
}

pub trait FileRemover: Send + Sync {
    /// Removes a file or a whole directory.
    fn remove(&self, path: &Path, mode: DeletionMode) -> Result<(), RemovalError>;

    /// Removes every entry inside `path`, keeping `path` itself.
    fn remove_contents(&self, path: &Path, mode: DeletionMode) -> Result<ContentsRemoval, RemovalError>;
}
