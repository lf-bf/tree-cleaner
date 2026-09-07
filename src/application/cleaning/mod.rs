//! Use cases around reclaiming space.

pub mod cleaning_service;
pub mod discovery_rules;
pub mod docker_inventory;
pub mod known_cache_locations;

pub use cleaning_service::{
    CleaningExecutor, CleaningHandle, CleaningPorts, CleaningProgress, CleaningService,
};
pub use discovery_rules::{DiscoveryRule, DiscoveryRules, Verification};
pub use docker_inventory::{DockerInventory, DockerStatus};
