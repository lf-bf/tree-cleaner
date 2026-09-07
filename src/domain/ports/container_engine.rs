//! Contract for talking to a container engine such as Docker.

use thiserror::Error;

use crate::domain::cleaning::DockerPruneKind;
use crate::domain::storage::ByteSize;

#[derive(Clone, Debug, Error)]
pub enum ContainerEngineError {
    #[error("container engine unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    CommandFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size: ByteSize,
    pub created: String,
}

impl ContainerImage {
    pub fn reference(&self) -> String {
        if self.tag.is_empty() || self.tag == "<none>" {
            self.repository.clone()
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }

    pub fn is_dangling(&self) -> bool {
        self.repository == "<none>"
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerDiskUsage {
    pub images: ByteSize,
    pub images_reclaimable: ByteSize,
    pub containers: ByteSize,
    pub containers_reclaimable: ByteSize,
    pub volumes: ByteSize,
    pub volumes_reclaimable: ByteSize,
    pub build_cache: ByteSize,
    pub build_cache_reclaimable: ByteSize,
}

pub trait ContainerEngine: Send + Sync {
    /// `Ok(version)` when the daemon answers.
    fn availability(&self) -> Result<String, ContainerEngineError>;

    fn list_images(&self) -> Result<Vec<ContainerImage>, ContainerEngineError>;

    /// Ids (possibly truncated) of images referenced by any container, running or not.
    fn image_ids_in_use(&self) -> Result<Vec<String>, ContainerEngineError>;

    fn disk_usage(&self) -> Result<ContainerDiskUsage, ContainerEngineError>;

    fn remove_images(&self, ids: &[String]) -> Result<String, ContainerEngineError>;

    fn prune(&self, kind: DockerPruneKind) -> Result<String, ContainerEngineError>;
}
