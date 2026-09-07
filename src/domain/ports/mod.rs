//! Contracts the domain needs from the outside world. Implemented in `infrastructure`.

pub mod container_engine;
pub mod directory_reader;
pub mod file_remover;
pub mod filesystem_probe;
pub mod privilege_escalator;
pub mod system_commands;
pub mod volume_provider;

pub use container_engine::{
    ContainerDiskUsage, ContainerEngine, ContainerEngineError, ContainerImage, EngineActionOutcome,
};
pub use directory_reader::{DirectoryEntryAccess, DirectoryReadError, DirectoryReader, Visit};
pub use file_remover::{
    ContentsRemoval, FileRemover, RemovalError, RemovalObserver, RemovalSummary, SilentObserver,
};
pub use filesystem_probe::FileSystemProbe;
pub use privilege_escalator::{PrivilegeError, PrivilegeEscalator};
pub use system_commands::{CommandFailure, CommandOutput, CommandRunner, FileRevealer};
pub use volume_provider::VolumeProvider;
