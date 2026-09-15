//! JSON backup and restore.
//!
//! - **Merge** folds a backup in like a sync from another device: whatever is
//!   newer wins, nothing is removed. Safe to run twice.
//! - **Replace** makes this device (and, through sync, its peers) look like
//!   the backup: entities missing from it are deleted and every register is
//!   re-written with a fresh stamp so it beats what peers currently hold.
//!
//! History is always merged; it's an append-only log.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::model::Completion;
use crate::store::{NewWrite, Store, StoreError};
use crate::sync::{field, Entity, HistoryRow, Reg};
use crate::time::Timestamp;

pub const FORMAT: &str = "dun-backup";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub exported_at: Timestamp,
    pub device_id: String,
    pub app_version: String,
    pub registers: Vec<Reg>,
    pub history: Vec<HistoryRow>,
    /// This device's own settings (chime, volume, hotkey...).
    pub local_settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub registers_changed: usize,
    pub history_added: usize,
    pub deleted: usize,
    pub local_settings: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("this file isn't a Dun backup")]
    NotABackup,
    #[error("this backup was made by a newer Dun (format version {0})")]
    TooNew(u32),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("backup file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn export(store: &Store, now: Timestamp) -> Result<Backup, BackupError> {
    Ok(Backup {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now,
        device_id: store.device_id().into(),
        app_version: crate::VERSION.into(),
        registers: store.registers()?,
        history: store.history(None, None, usize::MAX >> 1)?,
        local_settings: store.local_settings()?.into_iter().collect(),
    })
}

pub fn parse(json: &str) -> Result<Backup, BackupError> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(BackupError::NotABackup);
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > VERSION {
        return Err(BackupError::TooNew(version));
    }
    Ok(serde_json::from_value(value)?)
}

pub fn import(
    store: &mut Store,
    now: Timestamp,
    backup: &Backup,
    mode: ImportMode,
) -> Result<ImportReport, BackupError> {
    let mut report = ImportReport::default();

    match mode {
        ImportMode::Merge => {
            let outcome = store.merge(now, &backup.registers, &[])?;
            report.registers_changed = outcome.regs_changed;
            for (key, value) in &backup.local_settings {
                if store.local_get::<serde_json::Value>(key)?.is_none() {
                    store.local_set(key, value)?;
                    report.local_settings += 1;
                }
            }
        }
        ImportMode::Replace => {
            let in_backup: BTreeSet<(Entity, &str)> = backup
                .registers
                .iter()
                .filter(|r| r.entity != Entity::Setting)
                .map(|r| (r.entity, r.id.as_str()))
                .collect();

            let current = store.registers()?;
            let mut writes = Vec::new();

            let mut seen = BTreeSet::new();
            for r in current.iter().filter(|r| r.entity != Entity::Setting) {
                let key = (r.entity, r.id.as_str());
                if !in_backup.contains(&key) && seen.insert(key) {
                    writes.push(NewWrite::new(r.entity, r.id.clone(), field::DELETED, true));
                    report.deleted += 1;
                }
            }

            let current_by_key: BTreeMap<(Entity, &str, &str), &Reg> =
                current.iter().map(|r| (r.key(), r)).collect();
            for r in &backup.registers {
                let mut value = r.value.clone();
                if r.entity == Entity::Item && r.field == field::COMPLETION {
                    // A max-register only moves up; out-rank the current value.
                    let current_gen = current_by_key
                        .get(&r.key())
                        .and_then(|c| serde_json::from_value::<Completion>(c.value.clone()).ok())
                        .map_or(0, |c| c.gen);
                    if let Ok(mut c) = serde_json::from_value::<Completion>(value.clone()) {
                        c.gen = c.gen.max(current_gen) + 1;
                        value = serde_json::to_value(c)?;
                    }
                }
                writes.push(NewWrite {
                    entity: r.entity,
                    id: r.id.clone(),
                    field: r.field.clone(),
                    value,
                });
            }
            report.registers_changed = store.write(now, writes)?.len();

            for (key, value) in &backup.local_settings {
                store.local_set(key, value)?;
                report.local_settings += 1;
            }
        }
    }

    // Stamps on old history rows are in the past, so merging them is safe.
    report.history_added = store.merge(now, &[], &backup.history)?.history_added;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewHistory;
    use crate::sync::HistoryKind;
    use crate::time::MINUTE;

    const T: Timestamp = Timestamp(1_789_500_000_000);

    fn title(id: &str, s: &str) -> NewWrite {
        NewWrite::new(Entity::Item, id, field::TITLE, s)
    }

    fn completion(through: i64, gen: u32) -> NewWrite {
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
    }

    fn seeded() -> Store {
        let mut s = Store::open_in_memory_as("pc").unwrap();
        s.write(
            T,
            vec![
                title("a", "Pay rent"),
                title("b", "Stretch"),
                completion(1_000, 0),
            ],
        )
        .unwrap();
        s.append_history(
            T,
            NewHistory {
                item_id: "a".into(),
                kind: HistoryKind::Done,
                occurrence: Some(Timestamp(1_000)),
                snooze_count: 1,
                title: "Pay rent".into(),
                ref_id: None,
                prev_completion: None,
            },
        )
        .unwrap();
        s.local_set("volume", &0.5).unwrap();
        s
    }

    #[test]
    fn export_round_trips_through_json_and_merges_into_an_empty_store() {
        let src = seeded();
        let json = serde_json::to_string_pretty(&export(&src, T).unwrap()).unwrap();
        let backup = parse(&json).unwrap();

        let mut dst = Store::open_in_memory_as("new-pc").unwrap();
        let report = import(&mut dst, T.plus(MINUTE), &backup, ImportMode::Merge).unwrap();
        assert_eq!(report.registers_changed, 3);
        assert_eq!(report.history_added, 1);
        assert_eq!(dst.registers().unwrap(), src.registers().unwrap());
        assert_eq!(dst.local_get::<f64>("volume").unwrap(), Some(0.5));

        let again = import(&mut dst, T.plus(2 * MINUTE), &backup, ImportMode::Merge).unwrap();
        assert_eq!(
            again,
            ImportReport::default(),
            "merging the same backup twice changes nothing"
        );
    }

    #[test]
    fn replace_deletes_extras_and_restores_older_values_with_fresh_stamps() {
        let src = seeded();
        let backup = export(&src, T).unwrap();

        let mut dev = seeded();
        dev.write(
            T.plus(MINUTE),
            vec![
                title("a", "Renamed later"),
                title("c", "Added later"),
                completion(9_000, 0),
            ],
        )
        .unwrap();
        dev.local_set("volume", &1.0).unwrap();

        let report = import(&mut dev, T.plus(2 * MINUTE), &backup, ImportMode::Replace).unwrap();
        assert_eq!(report.deleted, 1);

        let regs = dev.registers().unwrap();
        let get = |id: &str, f: &str| regs.iter().find(|r| r.id == id && r.field == f).cloned();
        assert_eq!(
            get("a", field::TITLE).unwrap().value,
            serde_json::json!("Pay rent")
        );
        assert_eq!(
            get("c", field::DELETED).unwrap().value,
            serde_json::json!(true)
        );
        let c: Completion =
            serde_json::from_value(get("a", field::COMPLETION).unwrap().value).unwrap();
        assert_eq!(c.through, Some(Timestamp(1_000)));
        assert_eq!(
            c.gen, 1,
            "gen is raised so the restored completion wins everywhere"
        );
        assert_eq!(dev.local_get::<f64>("volume").unwrap(), Some(0.5));

        // A peer still holding the later rename loses to the replace.
        let mut peer = Store::open_in_memory_as("phone").unwrap();
        peer.write(T.plus(MINUTE), vec![title("a", "Renamed later")])
            .unwrap();
        peer.merge(T.plus(3 * MINUTE), &regs, &[]).unwrap();
        let peer_title = peer
            .register(Entity::Item, "a", field::TITLE)
            .unwrap()
            .unwrap();
        assert_eq!(peer_title.value, serde_json::json!("Pay rent"));
    }

    #[test]
    fn rejects_foreign_and_newer_files() {
        assert!(matches!(
            parse(r#"{"hello": "world"}"#),
            Err(BackupError::NotABackup)
        ));
        assert!(matches!(
            parse(r#"{"format": "dun-backup", "version": 99}"#),
            Err(BackupError::TooNew(99))
        ));
        assert!(matches!(parse("not json"), Err(BackupError::Json(_))));
    }
}
