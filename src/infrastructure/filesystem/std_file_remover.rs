//! Removes files as the current user, permanently or through the system Trash.
//!
//! Permanent removal walks the tree itself (iteratively, children before parents) instead
//! of calling `remove_dir_all`, so every removed entry can be reported and the operation
//! can be cancelled between entries.

use std::path::{Path, PathBuf};

use crate::domain::deletion::DeletionMode;
use crate::domain::ports::{ContentsRemoval, FileRemover, RemovalError, RemovalObserver, RemovalSummary};

/// How many entries go by between two cancellation checks.
const CANCEL_CHECK_INTERVAL: u64 = 128;

#[derive(Debug, Default)]
pub struct StdFileRemover;

impl StdFileRemover {
    pub fn new() -> Self {
        Self
    }

    fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.blocks().saturating_mul(512)
        }
        #[cfg(not(unix))]
        {
            metadata.len()
        }
    }

    fn move_to_trash(
        path: &Path,
        observer: &mut dyn RemovalObserver,
    ) -> Result<RemovalSummary, RemovalError> {
        trash::delete(path).map_err(|error| match error {
            trash::Error::CouldNotAccess { .. } => RemovalError::PermissionDenied,
            trash::Error::CanonicalizePath { .. } => RemovalError::NotFound,
            other => RemovalError::Other(other.to_string()),
        })?;
        observer.entry_removed(path, 0);
        Ok(RemovalSummary { entries_removed: 1, bytes_removed: 0 })
    }

    /// Removes `root` and everything below it. Continues past entries that cannot be
    /// removed and reports the first error at the end, so a partially protected tree
    /// loses everything it can before the caller asks for privileges.
    fn remove_permanently(
        root: &Path,
        observer: &mut dyn RemovalObserver,
    ) -> Result<RemovalSummary, RemovalError> {
        let metadata = std::fs::symlink_metadata(root).map_err(|error| RemovalError::from_io(&error))?;
        let mut summary = RemovalSummary::default();
        if !metadata.is_dir() {
            let bytes = Self::allocated_bytes(&metadata);
            std::fs::remove_file(root).map_err(|error| RemovalError::from_io(&error))?;
            summary.entries_removed = 1;
            summary.bytes_removed = bytes;
            observer.entry_removed(root, bytes);
            return Ok(summary);
        }

        let mut first_error: Option<RemovalError> = None;
        let mut since_cancel_check = 0u64;
        // (directory, children already handled)
        let mut stack: Vec<(PathBuf, bool)> = vec![(root.to_path_buf(), false)];
        while let Some((directory, children_done)) = stack.pop() {
            if children_done {
                match std::fs::remove_dir(&directory) {
                    Ok(()) => {
                        summary.entries_removed += 1;
                        observer.entry_removed(&directory, 0);
                    }
                    Err(error) => {
                        let error = RemovalError::from_io(&error);
                        if error != RemovalError::NotFound {
                            first_error.get_or_insert(error);
                        }
                    }
                }
                continue;
            }
            if observer.should_cancel() {
                return Err(RemovalError::Cancelled);
            }
            let entries = match std::fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    first_error.get_or_insert(RemovalError::from_io(&error));
                    continue;
                }
            };
            stack.push((directory, true));
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        first_error.get_or_insert(RemovalError::from_io(&error));
                        continue;
                    }
                };
                let path = entry.path();
                let is_directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                if is_directory {
                    stack.push((path, false));
                    continue;
                }
                let bytes = entry.metadata().map(|metadata| Self::allocated_bytes(&metadata)).unwrap_or(0);
                match std::fs::remove_file(&path) {
                    Ok(()) => {
                        summary.entries_removed += 1;
                        summary.bytes_removed = summary.bytes_removed.saturating_add(bytes);
                        observer.entry_removed(&path, bytes);
                    }
                    Err(error) => {
                        let error = RemovalError::from_io(&error);
                        if error != RemovalError::NotFound {
                            first_error.get_or_insert(error);
                        }
                    }
                }
                since_cancel_check += 1;
                if since_cancel_check >= CANCEL_CHECK_INTERVAL {
                    since_cancel_check = 0;
                    if observer.should_cancel() {
                        return Err(RemovalError::Cancelled);
                    }
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(summary),
        }
    }
}

impl FileRemover for StdFileRemover {
    fn remove(
        &self,
        path: &Path,
        mode: DeletionMode,
        observer: &mut dyn RemovalObserver,
    ) -> Result<RemovalSummary, RemovalError> {
        match mode {
            DeletionMode::Permanent => Self::remove_permanently(path, observer),
            DeletionMode::Trash => Self::move_to_trash(path, observer),
        }
    }

    fn remove_contents(
        &self,
        path: &Path,
        mode: DeletionMode,
        observer: &mut dyn RemovalObserver,
    ) -> Result<ContentsRemoval, RemovalError> {
        let entries = std::fs::read_dir(path).map_err(|error| RemovalError::from_io(&error))?;
        let mut result = ContentsRemoval::default();
        for entry in entries {
            if observer.should_cancel() {
                return Err(RemovalError::Cancelled);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    result.failed.push((path.to_path_buf(), RemovalError::from_io(&error)));
                    continue;
                }
            };
            let child = entry.path();
            match self.remove(&child, mode, observer) {
                Ok(summary) => {
                    result.removed += 1;
                    result.summary.absorb(summary);
                }
                Err(RemovalError::NotFound) => result.removed += 1,
                Err(RemovalError::Cancelled) => return Err(RemovalError::Cancelled),
                Err(error) => result.failed.push((child, error)),
            }
        }
        Ok(result)
    }
}
