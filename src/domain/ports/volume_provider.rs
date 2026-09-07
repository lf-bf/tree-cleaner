//! Contract for discovering mounted volumes.

use std::path::PathBuf;

use crate::domain::storage::Volume;

pub trait VolumeProvider: Send + Sync {
    fn volumes(&self) -> Vec<Volume>;

    /// Every mount point on the system, including hidden and virtual ones.
    fn mount_points(&self) -> Vec<PathBuf>;
}
