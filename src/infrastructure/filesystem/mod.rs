//! Filesystem adapters.

pub mod mount_table;
pub mod std_directory_reader;
pub mod std_file_remover;
pub mod std_filesystem_probe;
pub mod sysinfo_volume_provider;

#[cfg(unix)]
pub mod rustix_directory_reader;

#[cfg(unix)]
pub use rustix_directory_reader::RustixDirectoryReader;
pub use std_directory_reader::StdDirectoryReader;
pub use std_file_remover::StdFileRemover;
pub use std_filesystem_probe::StdFileSystemProbe;
pub use sysinfo_volume_provider::SysinfoVolumeProvider;
