//! Platform-neutral core of Dun: domain model, recurrence, the scheduler state
//! machine, hybrid logical clocks, register merge and SQLite storage.
//!
//! Nothing in this crate knows about Tauri, Windows or Android. Every function
//! that depends on time takes `now` (and a time zone) as an argument so it can
//! be driven by a fake clock in tests.

pub mod hlc;
pub mod ids;
pub mod model;
pub mod occurrence;
pub mod quiet;
pub mod recurrence;
pub mod store;
pub mod sync;
pub mod time;

/// Crate version, surfaced in sync handshakes and backups.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
