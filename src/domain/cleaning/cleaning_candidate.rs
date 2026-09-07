//! Something the cleaner can remove.

use std::path::{Path, PathBuf};

use super::cleaning_category::CleaningCategory;
use crate::domain::storage::{ByteSize, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CandidateId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockerPruneKind {
    StoppedContainers,
    DanglingVolumes,
    BuildCache,
}

impl DockerPruneKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::StoppedContainers => "docker container prune",
            Self::DanglingVolumes => "docker volume prune",
            Self::BuildCache => "docker builder prune --all",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleaningLocation {
    /// Remove the directory itself.
    Directory(PathBuf),
    /// Remove everything inside the directory but keep the directory.
    DirectoryContents(PathBuf),
    DockerImage {
        id: String,
        reference: String,
    },
    DockerPrune(DockerPruneKind),
    /// An external command that frees space on its own (for example `brew cleanup`).
    Command {
        program: String,
        arguments: Vec<String>,
    },
}

impl CleaningLocation {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Directory(path) | Self::DirectoryContents(path) => Some(path),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Directory(path) => path.display().to_string(),
            Self::DirectoryContents(path) => format!("{}/*", path.display()),
            Self::DockerImage { reference, id } => format!("{reference} ({id})"),
            Self::DockerPrune(kind) => kind.label().to_owned(),
            Self::Command { program, arguments } => {
                let mut text = program.clone();
                for argument in arguments {
                    text.push(' ');
                    text.push_str(argument);
                }
                text
            }
        }
    }
}

/// Why a candidate must not be touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protection {
    None,
    /// Matched a rule from the configuration file.
    Rule(String),
    /// Still referenced by something (for example an image used by a container).
    InUse(String),
}

impl Protection {
    pub const fn is_protected(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Rule(reason) | Self::InUse(reason) => Some(reason),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CleaningCandidate {
    pub id: CandidateId,
    pub category: CleaningCategory,
    pub label: String,
    pub location: CleaningLocation,
    /// Unknown until measured.
    pub size: Option<ByteSize>,
    pub requires_privileges: bool,
    pub protection: Protection,
    pub selected: bool,
    /// Root node in the file tree that measures this candidate, when it is a directory.
    pub measurement_node: Option<NodeId>,
}

impl CleaningCandidate {
    pub fn is_protected(&self) -> bool {
        self.protection.is_protected()
    }

    pub fn is_actionable(&self) -> bool {
        self.selected && !self.is_protected()
    }

    pub fn path(&self) -> Option<&Path> {
        self.location.path()
    }
}
