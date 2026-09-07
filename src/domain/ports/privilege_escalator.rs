//! Contract for removing paths the current user is not allowed to touch.
//!
//! Implementations run in the foreground with the terminal handed back to the user so the
//! operating system can ask for the password itself. The program never sees it.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum PrivilegeError {
    #[error("privilege escalation is not available: {0}")]
    Unavailable(String),
    #[error("authentication failed or was cancelled")]
    Denied,
    #[error("{0}")]
    Failed(String),
}

pub trait PrivilegeEscalator: Send + Sync {
    fn is_available(&self) -> bool;

    /// Human readable description of the mechanism (for example `sudo`).
    fn describe(&self) -> String;

    /// Removes each path recursively with elevated privileges.
    fn remove_paths(&self, paths: &[PathBuf]) -> Result<(), PrivilegeError>;

    /// Removes the contents of each directory, keeping the directories.
    fn remove_directory_contents(&self, directories: &[PathBuf]) -> Result<(), PrivilegeError>;
}
