//! Configuration model shared by every layer above the domain.

pub mod app_config;

pub use app_config::{
    AppConfig, CleanerConfig, DeletionConfig, DockerConfig, ScanConfig, ViewConfig, expand_tilde,
};
