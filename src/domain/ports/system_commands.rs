//! Contracts for the few external programs the application relies on.

use std::path::Path;

use thiserror::Error;

#[derive(Clone, Debug, Error)]
#[error("{program} failed: {detail}")]
pub struct CommandFailure {
    pub program: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner: Send + Sync {
    fn is_installed(&self, program: &str) -> bool;

    fn run(&self, program: &str, arguments: &[String]) -> Result<CommandOutput, CommandFailure>;
}

pub trait FileRevealer: Send + Sync {
    /// Shows the path in the graphical file manager (Finder, Nautilus, ...).
    fn reveal(&self, path: &Path) -> Result<(), CommandFailure>;
}
