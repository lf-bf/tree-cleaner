//! Talks to the container engine on a background thread so the interface never blocks.

use std::sync::Arc;

use crossbeam_channel::Receiver;

use crate::domain::ports::{ContainerDiskUsage, ContainerEngine, ContainerImage};

#[derive(Clone, Debug)]
pub enum DockerStatus {
    Disabled,
    Checking,
    Available { version: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug)]
pub struct DockerInventory {
    pub status: DockerStatus,
    pub images: Vec<ContainerImage>,
    pub image_ids_in_use: Vec<String>,
    pub usage: ContainerDiskUsage,
}

pub fn spawn_docker_inventory(engine: Arc<dyn ContainerEngine>) -> Receiver<DockerInventory> {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("tree-cleaner-docker".to_owned())
        .spawn(move || {
            let inventory = match engine.availability() {
                Err(error) => DockerInventory {
                    status: DockerStatus::Unavailable { reason: error.to_string() },
                    images: Vec::new(),
                    image_ids_in_use: Vec::new(),
                    usage: ContainerDiskUsage::default(),
                },
                Ok(version) => DockerInventory {
                    status: DockerStatus::Available { version },
                    images: engine.list_images().unwrap_or_default(),
                    image_ids_in_use: engine.image_ids_in_use().unwrap_or_default(),
                    usage: engine.disk_usage().unwrap_or_default(),
                },
            };
            let _ = sender.send(inventory);
        })
        .expect("failed to spawn docker inventory thread");
    receiver
}
