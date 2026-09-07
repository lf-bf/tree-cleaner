//! Volumes as reported by `sysinfo`, filtered down to what a person cares about.

use std::path::{Path, PathBuf};

use sysinfo::{DiskKind, Disks};

use super::mount_table;
use crate::domain::ports::VolumeProvider;
use crate::domain::storage::{ByteSize, Volume, VolumeKind};

#[derive(Debug, Default)]
pub struct SysinfoVolumeProvider;

impl SysinfoVolumeProvider {
    pub fn new() -> Self {
        Self
    }
}

/// macOS splits the boot container into several volumes that all report the same numbers.
/// Only `/` is shown; the others are reachable through it anyway.
fn is_hidden_system_volume(mount_point: &Path) -> bool {
    mount_point.starts_with("/System/Volumes")
        || mount_point.starts_with("/private/var/vm")
        || mount_point.starts_with("/dev")
        || mount_point.starts_with("/proc")
        || mount_point.starts_with("/sys")
        || mount_point.starts_with("/run")
        || mount_point.starts_with("/snap")
        || mount_point.starts_with("/boot/efi")
}

impl VolumeProvider for SysinfoVolumeProvider {
    fn volumes(&self) -> Vec<Volume> {
        let disks = Disks::new_with_refreshed_list();
        let mut volumes: Vec<Volume> = disks
            .list()
            .iter()
            .filter(|disk| disk.total_space() > 0)
            .filter(|disk| !is_hidden_system_volume(disk.mount_point()))
            .map(|disk| Volume {
                name: {
                    let name = disk.name().to_string_lossy().trim().to_owned();
                    if name.is_empty() { disk.mount_point().display().to_string() } else { name }
                },
                mount_point: disk.mount_point().to_path_buf(),
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total: ByteSize::new(disk.total_space()),
                available: ByteSize::new(disk.available_space()),
                is_removable: disk.is_removable(),
                is_read_only: disk.is_read_only(),
                kind: match disk.kind() {
                    DiskKind::SSD => VolumeKind::Ssd,
                    DiskKind::HDD => VolumeKind::Hdd,
                    DiskKind::Unknown(_) => VolumeKind::Unknown,
                },
            })
            .collect();
        volumes.sort_by(|left, right| {
            right
                .is_system_root()
                .cmp(&left.is_system_root())
                .then_with(|| left.mount_point.cmp(&right.mount_point))
        });
        volumes.dedup_by(|left, right| left.mount_point == right.mount_point);
        volumes
    }

    fn mount_points(&self) -> Vec<PathBuf> {
        let mut points: Vec<PathBuf> = mount_table::mount_points().into_iter().collect();
        if points.is_empty() {
            let disks = Disks::new_with_refreshed_list();
            points = disks.list().iter().map(|disk| disk.mount_point().to_path_buf()).collect();
        }
        points.sort();
        points.dedup();
        points
    }
}
