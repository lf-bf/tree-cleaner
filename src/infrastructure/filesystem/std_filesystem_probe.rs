//! Existence checks through the standard library.

use std::path::{Path, PathBuf};

use crate::domain::ports::FileSystemProbe;

#[derive(Debug, Default)]
pub struct StdFileSystemProbe;

impl FileSystemProbe for StdFileSystemProbe {
    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn is_directory(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
    }

    fn is_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
    }

    fn home_directory(&self) -> Option<PathBuf> {
        dirs::home_dir()
    }
}
