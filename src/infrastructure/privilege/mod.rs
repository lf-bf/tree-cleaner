//! Privilege escalation adapters.

pub mod sudo_escalator;

pub use sudo_escalator::{SudoEscalator, find_in_path};
