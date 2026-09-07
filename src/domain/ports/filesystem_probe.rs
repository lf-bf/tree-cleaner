//! Cheap existence checks used to confirm what a directory is.

use std::path::{Path, PathBuf};

pub trait FileSystemProbe: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn is_directory(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn home_directory(&self) -> Option<PathBuf>;
}
