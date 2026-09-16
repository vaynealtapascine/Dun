//! The phone's process-wide engine.
//!
//! Receivers and the app run in the same process and load the same `.so`, so
//! there is exactly one engine (and one SQLite handle) behind this mutex. The
//! data directory and time zone arrive with every call because a receiver can
//! run long before any Activity exists.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use dun_core::engine::Engine;
use dun_core::time::{TimeZone, Timestamp};

pub const DB_FILE: &str = "dun.sqlite3";

struct Shared {
    dir: PathBuf,
    engine: Engine,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

/// Runs `f` with the shared engine, opening it the first time.
///
/// Re-opens if Android hands us a different data directory (it doesn't in
/// practice, but a stale handle would be silently wrong).
pub fn with_engine<T>(data_dir: &str, f: impl FnOnce(&mut Engine) -> T) -> Result<T, String> {
    let mut guard = lock();
    let dir = Path::new(data_dir);
    let reopen = guard.as_ref().is_none_or(|s| s.dir != dir);
    if reopen {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let engine = Engine::open(&dir.join(DB_FILE)).map_err(|e| e.to_string())?;
        *guard = Some(Shared {
            dir: dir.to_path_buf(),
            engine,
        });
    }
    let shared = guard.as_mut().expect("just opened");
    Ok(f(&mut shared.engine))
}

/// Drops the engine, so the next call re-opens it (used by tests).
pub fn reset() {
    *lock() = None;
}

fn lock() -> MutexGuard<'static, Option<Shared>> {
    SHARED.lock().unwrap_or_else(|p| p.into_inner())
}

/// Android gives an IANA id; an unknown one falls back to UTC rather than
/// failing the whole event.
pub fn zone(tz_id: &str) -> TimeZone {
    TimeZone::get(tz_id).unwrap_or(TimeZone::UTC)
}

pub fn now(now_ms: i64) -> Timestamp {
    Timestamp(now_ms)
}
