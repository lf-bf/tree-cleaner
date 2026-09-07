//! tree-cleaner: an interactive terminal explorer and cleaner for disk usage.
//!
//! The crate follows a layered, domain-driven layout:
//!
//! * [`domain`] — pure model and the contracts (ports) it needs from the outside world;
//! * [`application`] — use cases: the parallel scanner, the tree coordinator, cleaning and
//!   deletion services, configuration;
//! * [`infrastructure`] — adapters implementing the ports (raw filesystem calls, Docker CLI,
//!   sudo, TOML files);
//! * [`presentation`] — the terminal interface.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
