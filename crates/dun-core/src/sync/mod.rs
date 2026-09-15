//! Sync data model and merge rules. Transport lives in the `dun-sync` crate.

pub mod merge;
pub mod rows;

pub use rows::{field, Entity, HistoryKind, HistoryRow, Reg};
