//! The domain layer: pure model, no I/O. Everything here can be reasoned about without a
//! filesystem, a terminal or a container engine.

pub mod cleaning;
pub mod deletion;
pub mod ports;
pub mod storage;
