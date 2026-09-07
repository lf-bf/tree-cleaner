//! Use cases around walking the filesystem: scheduling, executing and aggregating.

pub mod node_id_allocator;
pub mod scan_engine;
pub mod scan_statistics;
pub mod scan_task;
pub mod scan_worker;
pub mod task_queue;
pub mod tree_coordinator;

pub use scan_engine::ScanEngine;
pub use scan_statistics::{ScanStatistics, StatisticsSnapshot};
pub use scan_worker::ScanSettings;
pub use task_queue::QueueDepths;
pub use tree_coordinator::{DiscoverySearch, FileListingState, HeaviestQuery, TreeCoordinator};
