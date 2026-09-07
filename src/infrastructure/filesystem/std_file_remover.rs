//! Removes files as the current user, permanently or through the system Trash.

use std::path::Path;

use crate::domain::deletion::DeletionMode;
use crate::domain::ports::{ContentsRemoval, FileRemover, RemovalError};

#[derive(Debug, Default)]
pub struct StdFileRemover;

impl StdFileRemover {
    pub fn new() -> Self {
        Self
    }

    fn remove_permanently(path: &Path) -> Result<(), RemovalError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| RemovalError::from_io(&error))?;
        let result =
            if metadata.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
        result.map_err(|error| RemovalError::from_io(&error))
    }

    fn move_to_trash(path: &Path) -> Result<(), RemovalError> {
        trash::delete(path).map_err(|error| match error {
            trash::Error::CouldNotAccess { .. } => RemovalError::PermissionDenied,
            trash::Error::CanonicalizePath { .. } => RemovalError::NotFound,
            other => RemovalError::Other(other.to_string()),
        })
    }
}

impl FileRemover for StdFileRemover {
    fn remove(&self, path: &Path, mode: DeletionMode) -> Result<(), RemovalError> {
        match mode {
            DeletionMode::Permanent => Self::remove_permanently(path),
            DeletionMode::Trash => Self::move_to_trash(path),
        }
    }

    fn remove_contents(&self, path: &Path, mode: DeletionMode) -> Result<ContentsRemoval, RemovalError> {
        let entries = std::fs::read_dir(path).map_err(|error| RemovalError::from_io(&error))?;
        let mut result = ContentsRemoval::default();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    result.failed.push((path.to_path_buf(), RemovalError::from_io(&error)));
                    continue;
                }
            };
            let child = entry.path();
            match self.remove(&child, mode) {
                Ok(()) => result.removed += 1,
                Err(RemovalError::NotFound) => result.removed += 1,
                Err(error) => result.failed.push((child, error)),
            }
        }
        Ok(result)
    }
}
