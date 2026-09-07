//! A mounted filesystem, as shown on the dashboard.

use std::path::PathBuf;

use super::byte_size::ByteSize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Volume {
    pub name: String,
    pub mount_point: PathBuf,
    pub file_system: String,
    pub total: ByteSize,
    pub available: ByteSize,
    pub is_removable: bool,
    pub is_read_only: bool,
    pub kind: VolumeKind,
}

impl Volume {
    pub fn used(&self) -> ByteSize {
        self.total.saturating_sub(self.available)
    }

    pub fn usage_ratio(&self) -> f64 {
        self.used().ratio_of(self.total)
    }

    pub fn is_system_root(&self) -> bool {
        self.mount_point.as_os_str() == "/"
    }
}
