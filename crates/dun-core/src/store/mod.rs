//! SQLite storage for registers, history and device-local state.
//!
//! The store always holds the merge winner for every register: local writes
//! and remote merges both go through [`crate::sync::merge::decide`]. Each
//! change gets a new local `seq`, which is the cursor peers sync from; a merge
//! that changes nothing leaves `seq` alone so rows don't ping-pong.

mod schema;

use std::collections::BTreeSet;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::hlc::{Hlc, HlcClock, HlcError};
use crate::model::Completion;
use crate::sync::merge::{decide, Decision};
use crate::sync::{Entity, HistoryKind, HistoryRow, Reg};
use crate::time::Timestamp;

pub use schema::CURRENT_VERSION;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("stored value is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Clock(#[from] HlcError),
    #[error("this database was written by a newer Dun (schema {found}; this build reads up to {supported})")]
    TooNew { found: i64, supported: i64 },
    #[error("corrupt row: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// A local change to one register; the store stamps it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewWrite {
    pub entity: Entity,
    pub id: String,
    pub field: String,
    pub value: serde_json::Value,
}

impl NewWrite {
    pub fn new(entity: Entity, id: impl Into<String>, field: &str, value: impl Serialize) -> Self {
        NewWrite {
            entity,
            id: id.into(),
            field: field.into(),
            value: serde_json::to_value(value).expect("register values are plain data"),
        }
    }
}

/// A history entry before it is stamped.
#[derive(Debug, Clone, PartialEq)]
pub struct NewHistory {
    pub item_id: String,
    pub kind: HistoryKind,
    pub occurrence: Option<Timestamp>,
    pub snooze_count: u32,
    pub title: String,
    pub ref_id: Option<String>,
    pub prev_completion: Option<Completion>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MergeOutcome {
    /// Entities with at least one register that changed.
    pub changed: BTreeSet<(Entity, String)>,
    pub regs_changed: usize,
    pub history_added: usize,
}

/// A page of rows changed after a sync cursor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    pub regs: Vec<Reg>,
    pub history: Vec<HistoryRow>,
    /// Cursor to request the next page from.
    pub max_seq: i64,
    pub more: bool,
}

pub struct Store {
    conn: Connection,
    device_id: String,
    clock: HlcClock,
    seq: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        Self::init(Connection::open(path)?, None)
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?, None)
    }

    /// In-memory store with a fixed device id, for tests and simulations.
    pub fn open_in_memory_as(device_id: &str) -> Result<Store> {
        Self::init(Connection::open_in_memory()?, Some(device_id))
    }

    fn init(mut conn: Connection, device_id: Option<&str>) -> Result<Store> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::migrate(&mut conn)?;

        let meta = |conn: &Connection, key: &str| -> Result<Option<String>> {
            Ok(conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
                .optional()?)
        };

        let device_id = match meta(&conn, "device_id")? {
            Some(id) => id,
            None => {
                let id = device_id
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                conn.execute(
                    "INSERT INTO meta(key, value) VALUES ('device_id', ?1)",
                    [&id],
                )?;
                id
            }
        };
        let hlc = meta(&conn, "hlc")?
            .map(|v| {
                v.parse::<u64>()
                    .map_err(|e| StoreError::Corrupt(format!("meta.hlc: {e}")))
            })
            .transpose()?
            .unwrap_or(0);
        let seq = meta(&conn, "seq")?
            .map(|v| {
                v.parse::<i64>()
                    .map_err(|e| StoreError::Corrupt(format!("meta.seq: {e}")))
            })
            .transpose()?
            .unwrap_or(0);

        Ok(Store {
            conn,
            device_id,
            clock: HlcClock::resume(Hlc(hlc)),
            seq,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Highest local change counter issued so far.
    pub fn seq(&self) -> i64 {
        self.seq
    }

    pub fn last_hlc(&self) -> Hlc {
        self.clock.last()
    }

    /// Applies local changes in one transaction and returns the registers that
    /// actually changed (writes of an unchanged value, or that lose a merge
    /// rule such as an older completion, are skipped).
    pub fn write(&mut self, now: Timestamp, writes: Vec<NewWrite>) -> Result<Vec<Reg>> {
        let mut clock = self.clock;
        let mut seq = self.seq;
        let tx = self.conn.transaction()?;
        let mut applied = Vec::new();
        for w in writes {
            let existing = load_reg(&tx, w.entity, &w.id, &w.field)?;
            if existing.as_ref().is_some_and(|e| e.value == w.value) {
                continue;
            }
            let reg = Reg {
                entity: w.entity,
                id: w.id,
                field: w.field,
                value: w.value,
                hlc: clock.tick(now),
                device: self.device_id.clone(),
            };
            if let Some(e) = &existing {
                if decide(e, &reg) == Decision::KeepLocal {
                    continue;
                }
            }
            seq += 1;
            upsert_reg(&tx, &reg, seq)?;
            applied.push(reg);
        }
        save_meta(&tx, clock.last(), seq)?;
        tx.commit()?;
        self.clock = clock;
        self.seq = seq;
        Ok(applied)
    }

    /// Stamps and stores a new history entry.
    pub fn append_history(&mut self, now: Timestamp, entry: NewHistory) -> Result<HistoryRow> {
        let mut clock = self.clock;
        let row = HistoryRow {
            id: crate::ids::new_id(),
            item_id: entry.item_id,
            kind: entry.kind,
            occurrence: entry.occurrence,
            at: now,
            snooze_count: entry.snooze_count,
            title: entry.title,
            ref_id: entry.ref_id,
            prev_completion: entry.prev_completion,
            hlc: clock.tick(now),
            device: self.device_id.clone(),
        };
        let seq = self.seq + 1;
        let tx = self.conn.transaction()?;
        insert_history(&tx, &row, seq)?;
        save_meta(&tx, clock.last(), seq)?;
        tx.commit()?;
        self.clock = clock;
        self.seq = seq;
        Ok(row)
    }

    /// Merges rows from a peer. The whole batch is rejected, with nothing
    /// applied, if any stamp is implausibly far ahead of `now`.
    pub fn merge(
        &mut self,
        now: Timestamp,
        regs: &[Reg],
        history: &[HistoryRow],
    ) -> Result<MergeOutcome> {
        self.clock.check_batch(
            regs.iter()
                .map(|r| r.hlc)
                .chain(history.iter().map(|h| h.hlc)),
            now,
        )?;

        let mut clock = self.clock;
        let mut seq = self.seq;
        let mut outcome = MergeOutcome::default();
        let tx = self.conn.transaction()?;

        for incoming in regs {
            clock.observe(incoming.hlc, now)?;
            let take = match load_reg(&tx, incoming.entity, &incoming.id, &incoming.field)? {
                None => true,
                Some(local) => {
                    local != *incoming && decide(&local, incoming) == Decision::TakeIncoming
                }
            };
            if take {
                seq += 1;
                upsert_reg(&tx, incoming, seq)?;
                outcome.regs_changed += 1;
                outcome
                    .changed
                    .insert((incoming.entity, incoming.id.clone()));
            }
        }

        for row in history {
            clock.observe(row.hlc, now)?;
            let exists = tx
                .query_row("SELECT 1 FROM history WHERE id = ?1", [&row.id], |_| Ok(()))
                .optional()?
                .is_some();
            if !exists {
                seq += 1;
                insert_history(&tx, row, seq)?;
                outcome.history_added += 1;
                outcome.changed.insert((Entity::Item, row.item_id.clone()));
            }
        }

        save_meta(&tx, clock.last(), seq)?;
        tx.commit()?;
        self.clock = clock;
        self.seq = seq;
        Ok(outcome)
    }

    /// Every register, ordered by key.
    pub fn registers(&self) -> Result<Vec<Reg>> {
        let mut stmt = self.conn.prepare(
            "SELECT entity, id, field, value, hlc, device FROM reg ORDER BY entity, id, field",
        )?;
        let rows = stmt.query_map([], read_reg)?;
        rows.map(|r| r?).collect()
    }

    /// Registers of one entity.
    pub fn registers_of(&self, entity: Entity, id: &str) -> Result<Vec<Reg>> {
        let mut stmt = self.conn.prepare(
            "SELECT entity, id, field, value, hlc, device FROM reg WHERE entity = ?1 AND id = ?2 ORDER BY field",
        )?;
        let rows = stmt.query_map(params![entity.as_str(), id], read_reg)?;
        rows.map(|r| r?).collect()
    }

    pub fn register(&self, entity: Entity, id: &str, field: &str) -> Result<Option<Reg>> {
        load_reg(&self.conn, entity, id, field)
    }

    /// History, newest first; optionally for one item and/or before an instant.
    pub fn history(
        &self,
        item_id: Option<&str>,
        before: Option<Timestamp>,
        limit: usize,
    ) -> Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_id, kind, occurrence, at, snooze_count, title, ref_id, prev_completion, hlc, device
             FROM history
             WHERE (?1 IS NULL OR item_id = ?1) AND (?2 IS NULL OR at < ?2)
             ORDER BY at DESC, id DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![item_id, before.map(|b| b.0), limit as i64],
            read_history,
        )?;
        rows.map(|r| r?).collect()
    }

    pub fn history_entry(&self, id: &str) -> Result<Option<HistoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, item_id, kind, occurrence, at, snooze_count, title, ref_id, prev_completion, hlc, device
             FROM history WHERE id = ?1",
        )?;
        let row = stmt.query_row([id], read_history).optional()?;
        row.transpose()
    }

    /// Up to `limit` rows (registers and history together) with `seq > after`.
    pub fn changes_since(&self, after: i64, limit: usize) -> Result<Changes> {
        let limit = limit.max(1);
        let mut regs: Vec<(i64, Reg)> = {
            let mut stmt = self.conn.prepare(
                "SELECT entity, id, field, value, hlc, device, seq FROM reg WHERE seq > ?1 ORDER BY seq LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![after, limit as i64 + 1], |r| {
                Ok((r.get::<_, i64>(6)?, read_reg(r)?))
            })?;
            rows.map(|r| {
                let (seq, reg) = r?;
                Ok((seq, reg?))
            })
            .collect::<Result<_>>()?
        };
        let mut history: Vec<(i64, HistoryRow)> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, item_id, kind, occurrence, at, snooze_count, title, ref_id, prev_completion, hlc, device, seq
                 FROM history WHERE seq > ?1 ORDER BY seq LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![after, limit as i64 + 1], |r| {
                Ok((r.get::<_, i64>(11)?, read_history(r)?))
            })?;
            rows.map(|r| {
                let (seq, row) = r?;
                Ok((seq, row?))
            })
            .collect::<Result<_>>()?
        };

        // Take the `limit` lowest seqs across both tables.
        let mut seqs: Vec<i64> = regs
            .iter()
            .map(|(s, _)| *s)
            .chain(history.iter().map(|(s, _)| *s))
            .collect();
        seqs.sort_unstable();
        let more = seqs.len() > limit;
        let cutoff = seqs.get(limit.min(seqs.len()).saturating_sub(1)).copied();

        let (max_seq, regs, history) = match cutoff {
            None => (after, Vec::new(), Vec::new()),
            Some(cut) => {
                regs.retain(|(s, _)| *s <= cut);
                history.retain(|(s, _)| *s <= cut);
                (
                    cut,
                    regs.into_iter().map(|(_, r)| r).collect(),
                    history.into_iter().map(|(_, h)| h).collect(),
                )
            }
        };
        Ok(Changes {
            regs,
            history,
            max_seq,
            more,
        })
    }

    pub fn local_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM local_setting WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
    }

    pub fn local_set<T: Serialize>(&mut self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO local_setting(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    /// Raw connection for later modules (ring state, peers) that own their tables.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn save_meta(tx: &Transaction<'_>, hlc: Hlc, seq: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO meta(key, value) VALUES ('hlc', ?1), ('seq', ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![hlc.0.to_string(), seq.to_string()],
    )?;
    Ok(())
}

fn load_reg(conn: &Connection, entity: Entity, id: &str, field: &str) -> Result<Option<Reg>> {
    let row = conn
        .query_row(
            "SELECT entity, id, field, value, hlc, device FROM reg WHERE entity = ?1 AND id = ?2 AND field = ?3",
            params![entity.as_str(), id, field],
            read_reg,
        )
        .optional()?;
    row.transpose()
}

fn upsert_reg(tx: &Transaction<'_>, reg: &Reg, seq: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO reg(entity, id, field, value, hlc, device, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(entity, id, field) DO UPDATE SET
           value = excluded.value, hlc = excluded.hlc, device = excluded.device, seq = excluded.seq",
        params![
            reg.entity.as_str(),
            reg.id,
            reg.field,
            serde_json::to_string(&reg.value)?,
            reg.hlc.0 as i64,
            reg.device,
            seq
        ],
    )?;
    Ok(())
}

fn insert_history(tx: &Transaction<'_>, row: &HistoryRow, seq: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO history(id, item_id, kind, occurrence, at, snooze_count, title, ref_id, prev_completion, hlc, device, seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            row.id,
            row.item_id,
            row.kind.as_str(),
            row.occurrence.map(|t| t.0),
            row.at.0,
            row.snooze_count,
            row.title,
            row.ref_id,
            row.prev_completion.map(|c| serde_json::to_string(&c)).transpose()?,
            row.hlc.0 as i64,
            row.device,
            seq
        ],
    )?;
    Ok(())
}

/// Row readers return `Ok(Err(..))` for rows SQLite read fine but whose
/// contents don't parse, so callers can surface them as `Corrupt`.
fn read_reg(r: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Reg>> {
    let entity: String = r.get(0)?;
    let id: String = r.get(1)?;
    let field: String = r.get(2)?;
    let value: String = r.get(3)?;
    let hlc: i64 = r.get(4)?;
    let device: String = r.get(5)?;
    Ok((|| {
        Ok(Reg {
            entity: Entity::parse(&entity)
                .ok_or_else(|| StoreError::Corrupt(format!("entity '{entity}'")))?,
            id,
            field,
            value: serde_json::from_str(&value)?,
            hlc: Hlc(hlc as u64),
            device,
        })
    })())
}

fn read_history(r: &rusqlite::Row<'_>) -> rusqlite::Result<Result<HistoryRow>> {
    let id: String = r.get(0)?;
    let item_id: String = r.get(1)?;
    let kind: String = r.get(2)?;
    let occurrence: Option<i64> = r.get(3)?;
    let at: i64 = r.get(4)?;
    let snooze_count: u32 = r.get(5)?;
    let title: String = r.get(6)?;
    let ref_id: Option<String> = r.get(7)?;
    let prev: Option<String> = r.get(8)?;
    let hlc: i64 = r.get(9)?;
    let device: String = r.get(10)?;
    Ok((|| {
        Ok(HistoryRow {
            id,
            item_id,
            kind: HistoryKind::parse(&kind)
                .ok_or_else(|| StoreError::Corrupt(format!("history kind '{kind}'")))?,
            occurrence: occurrence.map(Timestamp),
            at: Timestamp(at),
            snooze_count,
            title,
            ref_id,
            prev_completion: prev.map(|p| serde_json::from_str(&p)).transpose()?,
            hlc: Hlc(hlc as u64),
            device,
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::field;
    use serde_json::json;

    const T: Timestamp = Timestamp(1_789_500_000_000);

    fn title(id: &str, s: &str) -> NewWrite {
        NewWrite::new(Entity::Item, id, field::TITLE, s)
    }

    fn done_entry(item: &str) -> NewHistory {
        NewHistory {
            item_id: item.into(),
            kind: HistoryKind::Done,
            occurrence: Some(T.minus(60_000)),
            snooze_count: 3,
            title: "Pay rent".into(),
            ref_id: None,
            prev_completion: Some(Completion {
                through: Some(Timestamp(5)),
                done_at: Some(Timestamp(6)),
                gen: 2,
            }),
        }
    }

    #[test]
    fn writes_are_stamped_and_unchanged_values_skipped() {
        let mut s = Store::open_in_memory_as("pc").unwrap();
        let applied = s
            .write(T, vec![title("a", "Pay rent"), title("b", "Stretch")])
            .unwrap();
        assert_eq!(applied.len(), 2);
        assert!(applied[0].hlc < applied[1].hlc);
        assert_eq!(s.seq(), 2);

        assert!(s.write(T, vec![title("a", "Pay rent")]).unwrap().is_empty());
        assert_eq!(s.seq(), 2, "a no-op write must not create a sync change");

        let reg = s
            .register(Entity::Item, "a", field::TITLE)
            .unwrap()
            .unwrap();
        assert_eq!(reg.value, json!("Pay rent"));
        assert_eq!(reg.device, "pc");
    }

    #[test]
    fn merge_only_bumps_seq_when_something_changes() {
        let mut pc = Store::open_in_memory_as("pc").unwrap();
        let mut phone = Store::open_in_memory_as("phone").unwrap();
        let regs = phone.write(T, vec![title("a", "From phone")]).unwrap();

        let first = pc.merge(T, &regs, &[]).unwrap();
        assert_eq!(first.regs_changed, 1);
        assert!(first.changed.contains(&(Entity::Item, "a".to_string())));
        let seq = pc.seq();

        let again = pc.merge(T, &regs, &[]).unwrap();
        assert_eq!(again, MergeOutcome::default());
        assert_eq!(pc.seq(), seq);
    }

    #[test]
    fn older_remote_write_loses_and_local_clock_moves_past_newer_ones() {
        let mut pc = Store::open_in_memory_as("pc").unwrap();
        let mut phone = Store::open_in_memory_as("phone").unwrap();
        let old = phone.write(T, vec![title("a", "old")]).unwrap();
        pc.write(T.plus(1_000), vec![title("a", "new")]).unwrap();
        assert_eq!(pc.merge(T.plus(2_000), &old, &[]).unwrap().regs_changed, 0);

        let future = phone
            .write(T.plus(60_000), vec![title("b", "later")])
            .unwrap();
        pc.merge(T.plus(2_000), &future, &[]).unwrap();
        let mine = pc.write(T.plus(3_000), vec![title("b", "mine")]).unwrap();
        assert!(
            mine[0].hlc > future[0].hlc,
            "a later local write must beat what we've seen"
        );
    }

    #[test]
    fn local_write_cannot_regress_completion() {
        let mut s = Store::open_in_memory_as("pc").unwrap();
        let done = |through: i64, gen: u32| {
            NewWrite::new(
                Entity::Item,
                "a",
                field::COMPLETION,
                Completion {
                    through: Some(Timestamp(through)),
                    done_at: None,
                    gen,
                },
            )
        };
        assert_eq!(s.write(T, vec![done(2_000, 0)]).unwrap().len(), 1);
        assert!(s.write(T, vec![done(1_000, 0)]).unwrap().is_empty());
        assert_eq!(
            s.write(T, vec![done(1_000, 1)]).unwrap().len(),
            1,
            "undo bumps gen"
        );
    }

    #[test]
    fn drifted_batch_is_rejected_atomically() {
        let mut pc = Store::open_in_memory_as("pc").unwrap();
        let mut phone = Store::open_in_memory_as("phone").unwrap();
        let ok = phone.write(T, vec![title("a", "fine")]).unwrap();
        let far = phone
            .write(T.plus(2 * crate::time::DAY), vec![title("b", "future")])
            .unwrap();
        let batch: Vec<Reg> = ok.into_iter().chain(far).collect();
        assert!(matches!(
            pc.merge(T, &batch, &[]),
            Err(StoreError::Clock(_))
        ));
        assert!(pc.registers().unwrap().is_empty());
        assert_eq!(pc.seq(), 0);
    }

    #[test]
    fn changes_since_pages_across_registers_and_history() {
        let mut s = Store::open_in_memory_as("pc").unwrap();
        s.write(T, vec![title("a", "1"), title("b", "2")]).unwrap();
        s.append_history(T, done_entry("a")).unwrap();
        s.write(T, vec![title("c", "3")]).unwrap();

        let p1 = s.changes_since(0, 3).unwrap();
        assert_eq!(
            (p1.regs.len(), p1.history.len(), p1.max_seq, p1.more),
            (2, 1, 3, true)
        );
        let p2 = s.changes_since(p1.max_seq, 3).unwrap();
        assert_eq!(
            (p2.regs.len(), p2.history.len(), p2.max_seq, p2.more),
            (1, 0, 4, false)
        );
        let p3 = s.changes_since(p2.max_seq, 3).unwrap();
        assert_eq!(
            p3,
            Changes {
                max_seq: 4,
                ..Changes::default()
            }
        );
    }

    #[test]
    fn history_round_trips_and_merges_by_id() {
        let mut pc = Store::open_in_memory_as("pc").unwrap();
        let mut phone = Store::open_in_memory_as("phone").unwrap();
        let row = phone.append_history(T, done_entry("a")).unwrap();
        assert_eq!(
            pc.merge(T, &[], std::slice::from_ref(&row))
                .unwrap()
                .history_added,
            1
        );
        assert_eq!(
            pc.merge(T, &[], std::slice::from_ref(&row))
                .unwrap()
                .history_added,
            0
        );
        assert_eq!(pc.history(Some("a"), None, 10).unwrap(), vec![row.clone()]);
        assert_eq!(pc.history_entry(&row.id).unwrap(), Some(row));
    }

    #[test]
    fn reopening_keeps_device_clock_and_cursor() {
        let dir = std::env::temp_dir().join(format!("dun-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dun.sqlite3");
        let (device, hlc) = {
            let mut s = Store::open(&path).unwrap();
            let regs = s.write(T, vec![title("a", "x")]).unwrap();
            s.local_set("volume", &0.8).unwrap();
            (s.device_id().to_string(), regs[0].hlc)
        };
        let mut s = Store::open(&path).unwrap();
        assert_eq!(s.device_id(), device);
        assert_eq!(s.seq(), 1);
        assert_eq!(s.local_get::<f64>("volume").unwrap(), Some(0.8));
        // Even with the wall clock set back, a new stamp sorts after the old one.
        let later = s.write(T.minus(10_000), vec![title("a", "y")]).unwrap();
        assert!(later[0].hlc > hlc);
        drop(s);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn newer_schema_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", CURRENT_VERSION + 1)
            .unwrap();
        assert!(matches!(
            schema::migrate(&mut conn),
            Err(StoreError::TooNew { .. })
        ));
    }
}
