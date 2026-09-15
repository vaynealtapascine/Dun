//! Schema migrations, applied in order and tracked with `PRAGMA user_version`.

use rusqlite::Connection;

use super::StoreError;

/// Each entry upgrades the schema by one version. Never edit a released entry;
/// append a new one.
const MIGRATIONS: &[&str] = &[
    // 1: registers, history, local settings
    "CREATE TABLE meta(
       key TEXT PRIMARY KEY,
       value TEXT NOT NULL);
     CREATE TABLE reg(
       entity TEXT NOT NULL,
       id TEXT NOT NULL,
       field TEXT NOT NULL,
       value TEXT NOT NULL,
       hlc INTEGER NOT NULL,
       device TEXT NOT NULL,
       seq INTEGER NOT NULL,
       PRIMARY KEY(entity, id, field)) WITHOUT ROWID;
     CREATE INDEX reg_seq ON reg(seq);
     CREATE TABLE history(
       id TEXT PRIMARY KEY,
       item_id TEXT NOT NULL,
       kind TEXT NOT NULL,
       occurrence INTEGER,
       at INTEGER NOT NULL,
       snooze_count INTEGER NOT NULL DEFAULT 0,
       title TEXT NOT NULL,
       ref_id TEXT,
       prev_completion TEXT,
       hlc INTEGER NOT NULL,
       device TEXT NOT NULL,
       seq INTEGER NOT NULL);
     CREATE INDEX history_seq ON history(seq);
     CREATE INDEX history_item ON history(item_id, at);
     CREATE TABLE local_setting(
       key TEXT PRIMARY KEY,
       value TEXT NOT NULL);",
];

pub const CURRENT_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let found: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if found > CURRENT_VERSION {
        return Err(StoreError::TooNew {
            found,
            supported: CURRENT_VERSION,
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}
