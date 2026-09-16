//! Android integration. Everything here except the commands is plain Rust,
//! so it is unit-tested on the desktop.

pub mod bridge;
#[cfg(mobile)]
pub mod commands;
pub mod core;
pub mod plan;
pub mod sync;
