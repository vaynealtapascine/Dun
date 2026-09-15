//! The store, its projection and the scheduler, kept in step.
//!
//! Platform shells hold one `Engine` per process and call it for every user
//! action, every sync merge and every scheduler wake-up.

use std::path::Path;

use crate::actions::{self, ActionError, Change, ItemDraft, PresetDraft};
use crate::backup::{self, Backup, BackupError, ImportMode, ImportReport};
use crate::scheduler::{Effect, Role, Scheduler};
use crate::state::State;
use crate::store::{MergeOutcome, Store, StoreError};
use crate::sync::{field, Entity, HistoryKind, HistoryRow, Reg};
use crate::time::{TimeZone, Timestamp};

const SCHEDULER_KEY: &str = "scheduler";

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Action(#[from] ActionError),
    #[error(transparent)]
    Backup(#[from] BackupError),
    #[error("that history entry no longer exists")]
    HistoryNotFound,
}

pub type Result<T> = std::result::Result<T, EngineError>;

pub struct Engine {
    store: Store,
    state: State,
    scheduler: Scheduler,
}

impl Engine {
    pub fn open(path: &Path) -> Result<Self> {
        Self::load(Store::open(path)?)
    }

    pub fn open_in_memory_as(device_id: &str) -> Result<Self> {
        Self::load(Store::open_in_memory_as(device_id)?)
    }

    fn load(store: Store) -> Result<Self> {
        let state = State::from_registers(&store.registers()?);
        // Unreadable scheduler state (a format change) just means a fresh start.
        let scheduler = store
            .local_get(SCHEDULER_KEY)
            .ok()
            .flatten()
            .unwrap_or_default();
        Ok(Engine {
            store,
            state,
            scheduler,
        })
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.scheduler
    }

    pub fn device_id(&self) -> &str {
        self.store.device_id()
    }

    /// Writes a change and folds the winning registers into the projection.
    pub fn apply(&mut self, now: Timestamp, change: Change) -> Result<Vec<HistoryRow>> {
        for reg in self.store.write(now, change.writes)? {
            self.state.apply(&reg);
        }
        change
            .history
            .into_iter()
            .map(|h| Ok(self.store.append_history(now, h)?))
            .collect()
    }

    /// Merges rows from a peer and re-projects what changed.
    pub fn merge_remote(
        &mut self,
        now: Timestamp,
        regs: &[Reg],
        history: &[HistoryRow],
    ) -> Result<MergeOutcome> {
        let outcome = self.store.merge(now, regs, history)?;
        let mut settings_changed = false;
        for (entity, id) in &outcome.changed {
            if *entity == Entity::Setting {
                settings_changed = true;
                continue;
            }
            let regs = self.store.registers_of(*entity, id)?;
            self.state.replace_entity(*entity, id, &regs);
        }
        if settings_changed {
            let regs = self
                .store
                .registers_of(Entity::Setting, field::SETTINGS_ID)?;
            self.state
                .replace_entity(Entity::Setting, field::SETTINGS_ID, &regs);
        }
        Ok(outcome)
    }

    /// Runs the scheduler and persists its ring states if they moved.
    pub fn evaluate(
        &mut self,
        now: Timestamp,
        tz: &TimeZone,
        role: Role<'_>,
    ) -> Result<Vec<Effect>> {
        let before = self.scheduler.clone();
        let effects = self.scheduler.evaluate(&self.state, now, tz, role);
        if self.scheduler != before {
            self.store.local_set(SCHEDULER_KEY, &self.scheduler)?;
        }
        Ok(effects)
    }

    pub fn due_for_alert(&self, now: Timestamp, tz: &TimeZone) -> Vec<(String, Timestamp)> {
        self.scheduler.due_for_alert(&self.state, now, tz)
    }

    // ---- items ----

    pub fn create_item(&mut self, now: Timestamp, draft: ItemDraft) -> Result<String> {
        let id = crate::ids::new_id();
        let change = actions::create_item(&self.state, self.store.device_id(), now, &id, draft)?;
        self.apply(now, change)?;
        Ok(id)
    }

    pub fn update_item(&mut self, now: Timestamp, id: &str, draft: ItemDraft) -> Result<()> {
        let change = actions::update_item(&self.state, now, id, draft)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn set_deleted(&mut self, now: Timestamp, id: &str, deleted: bool) -> Result<()> {
        let change = actions::set_deleted(&self.state, id, deleted)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn done(
        &mut self,
        now: Timestamp,
        tz: &TimeZone,
        id: &str,
        occ: Option<Timestamp>,
    ) -> Result<()> {
        let change = actions::done(&self.state, now, tz, id, occ)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn snooze(
        &mut self,
        now: Timestamp,
        tz: &TimeZone,
        id: &str,
        occ: Option<Timestamp>,
        minutes: u32,
    ) -> Result<()> {
        let change = actions::snooze(&self.state, now, tz, id, occ, minutes)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn snooze_many(
        &mut self,
        now: Timestamp,
        tz: &TimeZone,
        ids: &[String],
        minutes: u32,
    ) -> Result<()> {
        let change = actions::snooze_many(&self.state, now, tz, ids, minutes)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn undo(&mut self, now: Timestamp, history_id: &str) -> Result<()> {
        let entry = self
            .store
            .history_entry(history_id)?
            .ok_or(EngineError::HistoryNotFound)?;
        let already_undone = self
            .store
            .history(Some(&entry.item_id), None, 10_000)?
            .iter()
            .any(|h| h.kind == HistoryKind::Undo && h.ref_id.as_deref() == Some(history_id));
        let change = actions::undo(&self.state, &entry, already_undone)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn history(
        &self,
        item_id: Option<&str>,
        before: Option<Timestamp>,
        limit: usize,
    ) -> Result<Vec<HistoryRow>> {
        Ok(self.store.history(item_id, before, limit)?)
    }

    // ---- timers and presets ----

    pub fn start_timer(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::start_timer(&self.state, now, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn pause_timer(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::pause_timer(&self.state, now, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn resume_timer(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::resume_timer(&self.state, now, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn reset_timer(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::reset_timer(&self.state, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn save_preset(
        &mut self,
        now: Timestamp,
        id: Option<&str>,
        draft: PresetDraft,
    ) -> Result<String> {
        let id = id.map_or_else(crate::ids::new_id, str::to_owned);
        let change = actions::save_preset(&self.state, &id, draft)?;
        self.apply(now, change)?;
        Ok(id)
    }

    pub fn delete_preset(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::delete_preset(&self.state, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn start_preset(&mut self, now: Timestamp, preset_id: &str) -> Result<String> {
        let id = crate::ids::new_id();
        let change =
            actions::start_preset(&self.state, self.store.device_id(), now, preset_id, &id)?;
        self.apply(now, change)?;
        Ok(id)
    }

    // ---- tags and settings ----

    pub fn save_tag(
        &mut self,
        now: Timestamp,
        id: Option<&str>,
        name: &str,
        color: &str,
        order: i64,
    ) -> Result<String> {
        let id = id.map_or_else(crate::ids::new_id, str::to_owned);
        let change = actions::save_tag(&self.state, &id, name, color, order)?;
        self.apply(now, change)?;
        Ok(id)
    }

    pub fn delete_tag(&mut self, now: Timestamp, id: &str) -> Result<()> {
        let change = actions::delete_tag(&self.state, id)?;
        self.apply(now, change).map(|_| ())
    }

    pub fn set_setting(
        &mut self,
        now: Timestamp,
        key: &str,
        value: impl serde::Serialize,
    ) -> Result<()> {
        self.apply(now, actions::set_setting(key, value))
            .map(|_| ())
    }

    pub fn mute_for(&mut self, now: Timestamp, minutes: u32) -> Result<()> {
        self.apply(now, actions::mute_for(now, minutes)).map(|_| ())
    }

    pub fn mute_until_tomorrow(&mut self, now: Timestamp, tz: &TimeZone) -> Result<()> {
        let change = actions::mute_until_tomorrow(&self.state, now, tz);
        self.apply(now, change).map(|_| ())
    }

    pub fn unmute(&mut self, now: Timestamp) -> Result<()> {
        self.apply(now, actions::unmute()).map(|_| ())
    }

    // ---- backup ----

    pub fn export_backup(&self, now: Timestamp) -> Result<Backup> {
        Ok(backup::export(&self.store, now)?)
    }

    pub fn import_backup(
        &mut self,
        now: Timestamp,
        backup: &Backup,
        mode: ImportMode,
    ) -> Result<ImportReport> {
        let report = backup::import(&mut self.store, now, backup, mode)?;
        self.state = State::from_registers(&self.store.registers()?);
        Ok(report)
    }
}
