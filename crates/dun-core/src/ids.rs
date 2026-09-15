//! Identifiers.

/// A new entity or history id: UUIDv7, so ids sort roughly by creation time.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
