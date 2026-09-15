//! Sync transport for Dun: pinned TLS, the PC's server and the phone's client.
//!
//! The rules of a sync live in `dun_core::sync`; this crate only moves the
//! bytes and proves who is on the other end.

pub mod cert;
pub mod client;
pub mod pairing;
pub mod pin;
pub mod server;
