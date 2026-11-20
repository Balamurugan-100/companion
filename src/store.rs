use crate::notes;
use crate::settings::{self, Settings};
use crate::sync;
use crate::tasks;
use bevy::log::{error, warn};
use redb::{Builder, Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::error::Error;
use std::path::{Path, PathBuf};

/// `<OS data dir>/companion/companion.db`, e.g.
/// `~/Library/Application Support/companion/companion.db` on macOS.
pub fn database_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("companion").join("companion.db"))
}

/// Opens the real on-disk store, degrading to in-memory defaults if the host
/// has no data directory. Used at startup; tests use `Store::open(temp_path)`.
pub fn open_default_store() -> Store {
    match database_path() {
        Some(path) => Store::open(&path),
        None => {
            error!("No OS data directory found; settings will stay in memory");
            Store { db: None }
        }
    }
}

const SETTINGS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("settings");
const FOCUS_SESSIONS_TABLE: TableDefinition<u64, &str> = TableDefinition::new("focus_sessions");
const TASKS_TABLE: TableDefinition<u64, &str> = TableDefinition::new("tasks");
const NOTES_TABLE: TableDefinition<u64, &str> = TableDefinition::new("notes");

/// Keys for opt-in GitHub sync state. They live in the same settings table so
/// older databases need no migration: missing keys simply fall back to defaults.
pub const SYNC_CONFIG_KEY: &str = "sync.config";
pub const SYNC_LOCAL_KEY: &str = "sync.local";
pub const SYNC_REMOTE_KEY: &str = "sync.remote";
pub const SYNC_BASE_KEY: &str = "sync.base";
pub const SYNC_CONFLICTS_KEY: &str = "sync.conflicts";

/// Only the newest sessions are shown in the panel, so reads stay bounded.
pub const FOCUS_HISTORY_LIMIT: usize = 20;
pub const TASK_HISTORY_LIMIT: usize = 50;
pub const NOTE_HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusSession {
    pub started_at_ms: u64,
    pub planned_seconds: u64,
    pub completed: bool,
}

pub use crate::settings::{
    BREAK_KEY, BREATH_EXHALE_KEY, BREATH_HOLD_KEY, BREATH_INHALE_KEY, BREATH_SESSION_KEY, CALM_KEY,
    FOCUS_KEY, OPACITY_KEY,
};

pub struct Store {
    db: Option<Database>,
}

type StoreResult<T> = Result<T, Box<dyn Error>>;

impl Store {
    /// Opens (or creates) the database file at `path`. A corrupt or otherwise
    /// unusable file leaves the store in memory-only mode instead of crashing.
    pub fn open(path: &Path) -> Self {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            error!("Cannot create store directory {}: {err}", parent.display());
            return Self { db: None };
        }

        let db = match Builder::new()
            .set_check_integrity_on_open(true)
            .create(path)
        {
            Ok(db) => db,
            Err(err) => {
                error!("Cannot open store at {}: {err}", path.display());
                return Self { db: None };
            }
        };
        let store = Self { db: Some(db) };
        store.bootstrap_if_empty();
        store
    }

    /// A brand-new database gets one row per setting so the file is usable as
    /// soon as it exists. Older or partially populated databases are left
    /// alone: `load` falls back per key, so no migration step is needed when
    /// new settings are added.
    fn bootstrap_if_empty(&self) {
        let Some(db) = &self.db else {
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                txn.open_table(FOCUS_SESSIONS_TABLE)?;
                txn.open_table(TASKS_TABLE)?;
                txn.open_table(NOTES_TABLE)?;
                let mut table = txn.open_table(SETTINGS_TABLE)?;
                if table.iter()?.next().transpose()?.is_none() {
                    let defaults = settings::defaults();
                    let focus = defaults.focus_minutes.to_string();
                    let break_minutes = defaults.break_minutes.to_string();
                    let calm = defaults.calm_mode.to_string();
                    let opacity = defaults.window_opacity.to_string();
                    let inhale = defaults.breath_inhale_secs.to_string();
                    let hold = defaults.breath_hold_secs.to_string();
                    let exhale = defaults.breath_exhale_secs.to_string();
                    let session = defaults.breath_session_secs.to_string();
                    table.insert(FOCUS_KEY, focus.as_str())?;
                    table.insert(BREAK_KEY, break_minutes.as_str())?;
                    table.insert(CALM_KEY, calm.as_str())?;
                    table.insert(OPACITY_KEY, opacity.as_str())?;
                    table.insert(BREATH_INHALE_KEY, inhale.as_str())?;
                    table.insert(BREATH_HOLD_KEY, hold.as_str())?;
                    table.insert(BREATH_EXHALE_KEY, exhale.as_str())?;
                    table.insert(BREATH_SESSION_KEY, session.as_str())?;
                }
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to bootstrap settings: {err}");
        }
    }

    /// Writes every setting as one string key/value row in one transaction.
    pub fn save(&self, settings: &Settings) {
        let Some(db) = &self.db else {
            error!("Cannot save settings: the store is unavailable");
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                let mut table = txn.open_table(SETTINGS_TABLE)?;
                let focus = settings.focus_minutes.to_string();
                let break_minutes = settings.break_minutes.to_string();
                let calm = settings.calm_mode.to_string();
                let opacity = settings::clamp_opacity(settings.window_opacity).to_string();
                let mut breathing = settings.clone();
                settings::clamp_breathing(&mut breathing);
                let inhale = breathing.breath_inhale_secs.to_string();
                let hold = breathing.breath_hold_secs.to_string();
                let exhale = breathing.breath_exhale_secs.to_string();
                let session = breathing.breath_session_secs.to_string();
                table.insert(FOCUS_KEY, focus.as_str())?;
                table.insert(BREAK_KEY, break_minutes.as_str())?;
                table.insert(CALM_KEY, calm.as_str())?;
                table.insert(OPACITY_KEY, opacity.as_str())?;
                table.insert(BREATH_INHALE_KEY, inhale.as_str())?;
                table.insert(BREATH_HOLD_KEY, hold.as_str())?;
                table.insert(BREATH_EXHALE_KEY, exhale.as_str())?;
                table.insert(BREATH_SESSION_KEY, session.as_str())?;
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to save settings: {err}");
        }
    }

    /// Reads every setting, falling back per key for missing or invalid data.
    pub fn load(&self) -> Settings {
        let Some(db) = &self.db else {
            return settings::defaults();
        };
        let result = (|| -> StoreResult<Settings> {
            let txn = db.begin_read()?;
            let table = txn.open_table(SETTINGS_TABLE)?;
            let defaults = settings::defaults();
            let mut loaded = Settings {
                focus_minutes: read_u32(&table, FOCUS_KEY, defaults.focus_minutes)?,
                break_minutes: read_u32(&table, BREAK_KEY, defaults.break_minutes)?,
                calm_mode: read_bool(&table, CALM_KEY, defaults.calm_mode)?,
                window_opacity: read_f32(&table, OPACITY_KEY, defaults.window_opacity)?,
                breath_inhale_secs: read_u32(
                    &table,
                    BREATH_INHALE_KEY,
                    defaults.breath_inhale_secs,
                )?,
                breath_hold_secs: read_u32(&table, BREATH_HOLD_KEY, defaults.breath_hold_secs)?,
                breath_exhale_secs: read_u32(
                    &table,
                    BREATH_EXHALE_KEY,
                    defaults.breath_exhale_secs,
                )?,
                breath_session_secs: read_u32(
                    &table,
                    BREATH_SESSION_KEY,
                    defaults.breath_session_secs,
                )?,
            };
            loaded.window_opacity = settings::clamp_opacity(loaded.window_opacity);
            settings::clamp_breathing(&mut loaded);
            Ok(loaded)
        })();
        match result {
            Ok(loaded) => loaded,
            Err(err) => {
                error!("Failed to load settings: {err}; using in-memory defaults");
                settings::defaults()
            }
        }
    }

    /// Stores one completed work session, keyed by its start time. A colliding
    /// key is bumped so two sessions in the same millisecond stay distinct.
    pub fn save_focus_session(&self, session: FocusSession) {
        let Some(db) = &self.db else {
            error!("Cannot save focus session: the store is unavailable");
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                let mut table = txn.open_table(FOCUS_SESSIONS_TABLE)?;
                let mut key = session.started_at_ms;
                while table.get(key)?.is_some() {
                    key += 1;
                }
                let value = format!(
                    "{},{}",
                    session.planned_seconds,
                    u8::from(session.completed)
                );
                table.insert(key, value.as_str())?;
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to save focus session: {err}");
        }
    }

    /// Returns the newest sessions first, capped at `FOCUS_HISTORY_LIMIT`.
    pub fn focus_sessions(&self) -> Vec<FocusSession> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let result = (|| -> StoreResult<Vec<FocusSession>> {
            let txn = db.begin_read()?;
            let table = txn.open_table(FOCUS_SESSIONS_TABLE)?;
            let mut sessions = Vec::new();
            for row in table.iter()?.rev().take(FOCUS_HISTORY_LIMIT) {
                let (key, value) = row?;
                if let Some(session) = parse_focus_session(key.value(), value.value()) {
                    sessions.push(session);
                } else {
                    warn!("Skipping malformed focus session row {}", key.value());
                }
            }
            Ok(sessions)
        })();
        match result {
            Ok(sessions) => sessions,
            Err(err) => {
                error!("Failed to load focus sessions: {err}");
                Vec::new()
            }
        }
    }

    /// Returns every session starting at or after `since_ms`, with no display
    /// limit: aggregation must not miss rows from a busy day.
    pub fn focus_sessions_since(&self, since_ms: u64) -> Vec<FocusSession> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let result = (|| -> StoreResult<Vec<FocusSession>> {
            let txn = db.begin_read()?;
            let table = txn.open_table(FOCUS_SESSIONS_TABLE)?;
            let mut sessions = Vec::new();
            for row in table.iter()? {
                let (key, value) = row?;
                if key.value() < since_ms {
                    continue;
                }
                if let Some(session) = parse_focus_session(key.value(), value.value()) {
                    sessions.push(session);
                } else {
                    warn!("Skipping malformed focus session row {}", key.value());
                }
            }
            Ok(sessions)
        })();
        match result {
            Ok(sessions) => sessions,
            Err(err) => {
                error!("Failed to load focus sessions since {since_ms}: {err}");
                Vec::new()
            }
        }
    }

    /// Local midnight in milliseconds. If the timezone has no midnight on the
    /// current date (a DST jump), the cutoff is "now", so nothing is counted
    /// until the next ordinary day.
    pub fn start_of_today_ms() -> u64 {
        use chrono::TimeZone;

        let now = chrono::Local::now();
        let boundary = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|midnight| chrono::Local.from_local_datetime(&midnight).earliest())
            .unwrap_or(now);
        boundary.timestamp_millis().max(0) as u64
    }

    /// Wall clock in milliseconds, clamped to `u64` for the persisted rows.
    pub fn now_millis() -> u64 {
        chrono::Utc::now().timestamp_millis().max(0) as u64
    }

    /// Returns the newest tasks first, capped at `TASK_HISTORY_LIMIT`.
    pub fn tasks(&self) -> Vec<tasks::Task> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let result = (|| -> StoreResult<Vec<tasks::Task>> {
            let txn = db.begin_read()?;
            let table = txn.open_table(TASKS_TABLE)?;
            let mut found = Vec::new();
            for row in table.iter()?.rev().take(TASK_HISTORY_LIMIT) {
                let (key, value) = row?;
                if let Some(task) = parse_task(key.value(), value.value()) {
                    found.push(task);
                } else {
                    warn!("Skipping malformed task row {}", key.value());
                }
            }
            Ok(found)
        })();
        match result {
            Ok(found) => found,
            Err(err) => {
                error!("Failed to load tasks: {err}");
                Vec::new()
            }
        }
    }

    /// Returns every completed task at or after `since_ms`, with no display
    /// limit. Open rows and rows without a completion timestamp are excluded.
    pub fn completed_tasks_since(&self, since_ms: u64) -> Vec<tasks::Task> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let result = (|| -> StoreResult<Vec<tasks::Task>> {
            let txn = db.begin_read()?;
            let table = txn.open_table(TASKS_TABLE)?;
            let mut found = Vec::new();
            for row in table.iter()? {
                let (key, value) = row?;
                if let Some(task) = parse_task(key.value(), value.value()) {
                    if task.done && task.completed_at_ms.is_some_and(|ms| ms >= since_ms) {
                        found.push(task);
                    }
                } else {
                    warn!("Skipping malformed task row {}", key.value());
                }
            }
            Ok(found)
        })();
        match result {
            Ok(found) => found,
            Err(err) => {
                error!("Failed to load completed tasks since {since_ms}: {err}");
                Vec::new()
            }
        }
    }

    /// Adds a task if its text is not blank and returns the generated id.
    pub fn add_task(&self, text: &str, now_ms: u64) -> Option<u64> {
        let db = self.db.as_ref()?;
        let result = (|| -> StoreResult<Option<u64>> {
            let txn = db.begin_write()?;
            let id = {
                let table = txn.open_table(TASKS_TABLE)?;
                next_id(&table)?
            };
            let Some(task) = tasks::Task::new(id, text, now_ms) else {
                return Ok(None);
            };
            {
                let mut table = txn.open_table(TASKS_TABLE)?;
                let row = task_row(&task);
                table.insert(id, row.as_str())?;
            }
            txn.commit()?;
            Ok(Some(id))
        })();
        match result {
            Ok(id) => id,
            Err(err) => {
                error!("Failed to add task: {err}");
                None
            }
        }
    }

    /// Marks a task done and reports whether this call caused the transition.
    pub fn complete_task(&self, id: u64, now_ms: u64) -> bool {
        let Some(db) = &self.db else {
            error!("Cannot complete task: the store is unavailable");
            return false;
        };
        let result = (|| -> StoreResult<bool> {
            let txn = db.begin_write()?;
            let changed = {
                let mut table = txn.open_table(TASKS_TABLE)?;
                let Some(value) = table.get(id)? else {
                    return Ok(false);
                };
                let Some(mut task) = parse_task(id, value.value()) else {
                    warn!("Skipping malformed task row {id}");
                    return Ok(false);
                };
                drop(value);
                if !task.complete(now_ms) {
                    return Ok(false);
                }
                let row = task_row(&task);
                table.insert(id, row.as_str())?;
                true
            };
            txn.commit()?;
            Ok(changed)
        })();
        match result {
            Ok(changed) => changed,
            Err(err) => {
                error!("Failed to complete task: {err}");
                false
            }
        }
    }

    pub fn delete_task(&self, id: u64) {
        let Some(db) = &self.db else {
            error!("Cannot delete task: the store is unavailable");
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                let mut table = txn.open_table(TASKS_TABLE)?;
                table.remove(id)?;
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to delete task: {err}");
        }
    }

    /// Returns the newest notes first, capped at `NOTE_HISTORY_LIMIT`.
    pub fn notes(&self) -> Vec<notes::Note> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let result = (|| -> StoreResult<Vec<notes::Note>> {
            let txn = db.begin_read()?;
            let table = txn.open_table(NOTES_TABLE)?;
            let mut found = Vec::new();
            for row in table.iter()? {
                let (key, value) = row?;
                if let Some(note) = parse_note(key.value(), value.value()) {
                    found.push(note);
                } else {
                    warn!("Skipping malformed note row {}", key.value());
                }
            }
            found.sort_unstable_by(|a, b| (b.updated_at_ms, b.id).cmp(&(a.updated_at_ms, a.id)));
            found.truncate(NOTE_HISTORY_LIMIT);
            Ok(found)
        })();
        match result {
            Ok(found) => found,
            Err(err) => {
                error!("Failed to load notes: {err}");
                Vec::new()
            }
        }
    }

    /// Creates a note when `id` is `None`, otherwise edits that note. Returns
    /// the written id, or `None` if the id does not exist or the store is down.
    pub fn upsert_note(
        &self,
        id: Option<u64>,
        title: &str,
        body: &str,
        now_ms: u64,
    ) -> Option<u64> {
        let db = self.db.as_ref()?;
        let result = (|| -> StoreResult<Option<u64>> {
            let txn = db.begin_write()?;
            let note = {
                let table = txn.open_table(NOTES_TABLE)?;
                match id {
                    Some(id) => {
                        let Some(value) = table.get(id)? else {
                            return Ok(None);
                        };
                        let Some(mut note) = parse_note(id, value.value()) else {
                            warn!("Skipping malformed note row {id}");
                            return Ok(None);
                        };
                        note.edit(title, body, now_ms);
                        note
                    }
                    None => notes::Note::new(next_id(&table)?, title, body, now_ms),
                }
            };
            {
                let mut table = txn.open_table(NOTES_TABLE)?;
                let row = note_row(&note);
                table.insert(note.id, row.as_str())?;
            }
            txn.commit()?;
            Ok(Some(note.id))
        })();
        match result {
            Ok(id) => id,
            Err(err) => {
                error!("Failed to save note: {err}");
                None
            }
        }
    }

    pub fn delete_note(&self, id: u64) {
        let Some(db) = &self.db else {
            error!("Cannot delete note: the store is unavailable");
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                let mut table = txn.open_table(NOTES_TABLE)?;
                table.remove(id)?;
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to delete note: {err}");
        }
    }

    /// Reads one raw setting row. Sync state is stored as JSON in these rows.
    fn get_setting(&self, key: &str) -> Option<String> {
        let db = self.db.as_ref()?;
        let txn = db.begin_read().ok()?;
        let table = txn.open_table(SETTINGS_TABLE).ok()?;
        let value = table.get(key).ok()??;
        Some(value.value().to_owned())
    }

    /// Writes one raw setting row, leaving every other row untouched.
    fn set_setting(&self, key: &str, value: &str) {
        let Some(db) = &self.db else {
            return;
        };
        let result = (|| -> StoreResult<()> {
            let txn = db.begin_write()?;
            {
                let mut table = txn.open_table(SETTINGS_TABLE)?;
                table.insert(key, value)?;
            }
            txn.commit()?;
            Ok(())
        })();
        if let Err(err) = result {
            error!("Failed to save setting {key}: {err}");
        }
    }

    pub fn load_sync_config(&self) -> sync::SyncConfig {
        self.get_setting(SYNC_CONFIG_KEY)
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_sync_config(&self, config: &sync::SyncConfig) {
        if let Ok(raw) = serde_json::to_string(config) {
            self.set_setting(SYNC_CONFIG_KEY, &raw);
        }
    }

    pub fn load_sync_document(&self, key: &str) -> sync::Document {
        self.get_setting(key)
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_sync_document(&self, key: &str, document: &sync::Document) {
        if let Ok(raw) = serde_json::to_string(document) {
            self.set_setting(key, &raw);
        }
    }

    pub fn load_sync_conflicts(&self) -> Vec<sync::Conflict> {
        self.get_setting(SYNC_CONFLICTS_KEY)
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save_sync_conflicts(&self, conflicts: &[sync::Conflict]) {
        if let Ok(raw) = serde_json::to_string(conflicts) {
            self.set_setting(SYNC_CONFLICTS_KEY, &raw);
        }
    }
}

fn parse_focus_session(started_at_ms: u64, value: &str) -> Option<FocusSession> {
    let (planned, completed) = value.split_once(',')?;
    let completed = match completed {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    Some(FocusSession {
        started_at_ms,
        planned_seconds: planned.parse().ok()?,
        completed,
    })
}

fn next_id(table: &impl ReadableTable<u64, &'static str>) -> StoreResult<u64> {
    let last = table.iter()?.rev().next().transpose()?;
    Ok(last.map_or(1, |(key, _)| key.value().saturating_add(1)))
}

fn task_row(task: &tasks::Task) -> String {
    format!(
        "{},{},{}\n{}",
        u8::from(task.done),
        task.created_at_ms,
        task.completed_at_ms.unwrap_or(0),
        task.text
    )
}

fn parse_task(id: u64, value: &str) -> Option<tasks::Task> {
    let (header, text) = value.split_once('\n')?;
    let mut fields = header.split(',');
    let done = match fields.next()? {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let created_at_ms = fields.next()?.parse().ok()?;
    let completed = fields.next()?.parse::<u64>().ok()?;
    if fields.next().is_some() {
        return None;
    }
    let mut task = tasks::Task::new(id, text, created_at_ms)?;
    if done {
        task.done = true;
        task.completed_at_ms = Some(completed);
    }
    Some(task)
}

fn note_row(note: &notes::Note) -> String {
    format!("{}\n{}\n{}", note.updated_at_ms, note.title, note.body)
}

fn parse_note(id: u64, value: &str) -> Option<notes::Note> {
    let mut fields = value.splitn(3, '\n');
    let updated_at_ms = fields.next()?.parse().ok()?;
    let title = fields.next()?;
    let body = fields.next()?;
    Some(notes::Note::new(id, title, body, updated_at_ms))
}

fn read_u32(
    table: &impl ReadableTable<&'static str, &'static str>,
    key: &str,
    fallback: u32,
) -> StoreResult<u32> {
    match table.get(key)? {
        Some(value) => Ok(value.value().parse().unwrap_or_else(|err| {
            warn!(
                "Invalid setting {key}={}: {err}; using default",
                value.value()
            );
            fallback
        })),
        None => Ok(fallback),
    }
}

fn read_bool(
    table: &impl ReadableTable<&'static str, &'static str>,
    key: &str,
    fallback: bool,
) -> StoreResult<bool> {
    match table.get(key)? {
        Some(value) => Ok(value.value().parse().unwrap_or_else(|err| {
            warn!(
                "Invalid setting {key}={}: {err}; using default",
                value.value()
            );
            fallback
        })),
        None => Ok(fallback),
    }
}

fn read_f32(
    table: &impl ReadableTable<&'static str, &'static str>,
    key: &str,
    fallback: f32,
) -> StoreResult<f32> {
    match table.get(key)? {
        Some(value) => Ok(value.value().parse().unwrap_or_else(|err| {
            warn!(
                "Invalid setting {key}={}: {err}; using default",
                value.value()
            );
            fallback
        })),
        None => Ok(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;

    #[test]
    fn database_path_is_under_the_os_data_directory() {
        let Some(data_dir) = dirs::data_dir() else {
            return; // No OS data directory on this host; nothing to assert.
        };
        assert_eq!(
            database_path(),
            Some(data_dir.join("companion").join("companion.db"))
        );
    }

    #[test]
    fn empty_database_creates_a_file_and_loads_defaults() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("nested").join("companion.db");

        let store = Store::open(&path);

        assert!(path.exists(), "the database file should be created");
        assert_eq!(store.load(), settings::defaults());
    }

    #[test]
    fn saved_settings_survive_reopening_the_database() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        let edited = Settings {
            focus_minutes: 42,
            break_minutes: 12,
            calm_mode: true,
            window_opacity: 0.42,
            breath_inhale_secs: 6,
            breath_exhale_secs: 9,
            breath_session_secs: 180,
            ..settings::defaults()
        };

        {
            let store = Store::open(&path);
            store.save(&edited);
        }

        let reopened = Store::open(&path);
        assert_eq!(reopened.load(), edited);
    }

    #[test]
    fn breathing_settings_survive_reopening_the_database() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        let edited = Settings {
            breath_inhale_secs: 5,
            breath_hold_secs: 0,
            breath_exhale_secs: 8,
            breath_session_secs: 300,
            ..settings::defaults()
        };

        {
            let store = Store::open(&path);
            store.save(&edited);
        }

        assert_eq!(Store::open(&path).load(), edited);
    }

    #[test]
    fn a_database_from_before_breathing_settings_needs_no_migration() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn
                    .open_table(SETTINGS_TABLE)
                    .expect("failed to open table");
                table.insert(FOCUS_KEY, "42").expect("failed to insert");
                table.insert(BREAK_KEY, "12").expect("failed to insert");
                table.insert(CALM_KEY, "true").expect("failed to insert");
                table.insert(OPACITY_KEY, "0.5").expect("failed to insert");
            }
            txn.commit().expect("failed to commit the fixture");
        }

        let loaded = Store::open(&path).load();

        assert_eq!(loaded.focus_minutes, 42);
        assert_eq!(loaded.break_minutes, 12);
        assert!(loaded.calm_mode);
        assert_eq!(loaded.window_opacity, 0.5);
        assert_eq!(
            loaded.breath_inhale_secs,
            settings::defaults().breath_inhale_secs
        );
        assert_eq!(
            loaded.breath_hold_secs,
            settings::defaults().breath_hold_secs
        );
        assert_eq!(
            loaded.breath_exhale_secs,
            settings::defaults().breath_exhale_secs
        );
        assert_eq!(
            loaded.breath_session_secs,
            settings::defaults().breath_session_secs
        );
    }

    #[test]
    fn out_of_range_breathing_values_are_clamped_on_load() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn
                    .open_table(SETTINGS_TABLE)
                    .expect("failed to open table");
                table
                    .insert(BREATH_INHALE_KEY, "999")
                    .expect("failed to insert");
                table
                    .insert(BREATH_SESSION_KEY, "1")
                    .expect("failed to insert");
            }
            txn.commit().expect("failed to commit the fixture");
        }

        let loaded = Store::open(&path).load();

        assert_eq!(loaded.breath_inhale_secs, 20);
        assert_eq!(loaded.breath_session_secs, 30);
    }

    #[test]
    fn partial_settings_fall_back_to_defaults_for_missing_keys() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn
                    .open_table(SETTINGS_TABLE)
                    .expect("failed to open table");
                table
                    .insert(FOCUS_KEY, "42")
                    .expect("failed to insert the fixture row");
            }
            txn.commit().expect("failed to commit the fixture");
        }

        let store = Store::open(&path);
        let expected = Settings {
            focus_minutes: 42,
            ..settings::defaults()
        };
        assert_eq!(store.load(), expected);
    }

    #[test]
    fn invalid_values_use_defaults_instead_of_crashing() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn
                    .open_table(SETTINGS_TABLE)
                    .expect("failed to open table");
                table
                    .insert(FOCUS_KEY, "not-a-number")
                    .expect("failed to insert the fixture row");
            }
            txn.commit().expect("failed to commit the fixture");
        }

        let store = Store::open(&path);
        assert_eq!(store.load(), settings::defaults());
    }

    #[test]
    fn a_corrupt_database_falls_back_to_in_memory_defaults() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        std::fs::write(&path, b"this is not a redb database").expect("failed to write garbage");

        let store = Store::open(&path);
        assert_eq!(store.load(), settings::defaults());
        // Saving must not panic even though the file could not be opened.
        store.save(&settings::defaults());
    }

    fn session(at_ms: u64, seconds: u64) -> FocusSession {
        FocusSession {
            started_at_ms: at_ms,
            planned_seconds: seconds,
            completed: true,
        }
    }

    #[test]
    fn saved_sessions_survive_reopening_the_database() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");

        {
            let store = Store::open(&path);
            store.save_focus_session(session(1000, 1500));
            store.save_focus_session(session(2000, 300));
        }

        let reopened = Store::open(&path);
        assert_eq!(
            reopened.focus_sessions(),
            vec![session(2000, 300), session(1000, 1500)]
        );
        assert_eq!(reopened.load(), settings::defaults());
    }

    #[test]
    fn focus_history_is_bounded_to_the_newest_sessions() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        let store = Store::open(&path);

        for n in 0..25 {
            store.save_focus_session(session(1000 + n, 60));
        }

        let history = store.focus_sessions();
        assert_eq!(history.len(), FOCUS_HISTORY_LIMIT);
        assert_eq!(history[0].started_at_ms, 1024);
        assert_eq!(history[FOCUS_HISTORY_LIMIT - 1].started_at_ms, 1005);
    }

    #[test]
    fn malformed_session_rows_are_skipped() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn
                    .open_table(FOCUS_SESSIONS_TABLE)
                    .expect("failed to open table");
                table
                    .insert(999, "not-a-session")
                    .expect("failed to insert");
            }
            txn.commit().expect("failed to commit the fixture");
        }
        let store = Store::open(&path);
        store.save_focus_session(session(1000, 60));

        assert_eq!(store.focus_sessions(), vec![session(1000, 60)]);
    }

    #[test]
    fn a_corrupt_database_has_an_empty_focus_history() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        std::fs::write(&path, b"not a redb database").expect("failed to write garbage");

        let store = Store::open(&path);
        assert!(store.focus_sessions().is_empty());
        store.save_focus_session(session(1000, 60));
        assert!(store.focus_sessions().is_empty());
    }

    #[test]
    fn a_fresh_store_has_no_tasks_and_rejects_blank_text() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        assert!(store.tasks().is_empty());
        assert_eq!(store.add_task("   ", 1), None);
        assert!(store.tasks().is_empty());
    }

    #[test]
    fn a_task_can_be_added_completed_and_deleted() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        let id = store
            .add_task("  Buy milk  ", 100)
            .expect("text should be accepted");
        let tasks = store.tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, id);
        assert_eq!(tasks[0].text, "Buy milk");
        assert!(!tasks[0].done);
        assert_eq!(tasks[0].created_at_ms, 100);

        assert!(store.complete_task(id, 200));
        assert!(
            !store.complete_task(id, 300),
            "a repeat is not a transition"
        );
        let tasks = store.tasks();
        assert!(tasks[0].done);
        assert_eq!(tasks[0].completed_at_ms, Some(200));

        store.delete_task(id);
        assert!(store.tasks().is_empty());
        assert!(!store.complete_task(id, 400));
    }

    #[test]
    fn tasks_survive_dropping_and_reopening_the_store() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        let id;
        {
            let store = Store::open(&path);
            id = store.add_task("Remember the keys", 10).expect("valid task");
            store.complete_task(id, 20);
        }

        let reopened = Store::open(&path);
        let tasks = reopened.tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, id);
        assert_eq!(tasks[0].text, "Remember the keys");
        assert!(tasks[0].done);
        assert_eq!(tasks[0].completed_at_ms, Some(20));
    }

    #[test]
    fn the_task_list_is_bounded_to_the_newest_items() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        for n in 0..(TASK_HISTORY_LIMIT as u64 + 5) {
            store.add_task(&format!("task {n}"), n);
        }

        let tasks = store.tasks();
        assert_eq!(tasks.len(), TASK_HISTORY_LIMIT);
        assert_eq!(tasks[0].text, "task 54");
        assert_eq!(tasks[TASK_HISTORY_LIMIT - 1].text, "task 5");
    }

    #[test]
    fn malformed_task_rows_are_skipped() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn.open_table(TASKS_TABLE).expect("failed to open table");
                table.insert(1, "not-a-task").expect("failed to insert");
            }
            txn.commit().expect("failed to commit the fixture");
        }
        let store = Store::open(&path);

        let id = store.add_task("valid", 2).expect("valid task");
        assert_eq!(
            store
                .tasks()
                .into_iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![id]
        );
    }

    #[test]
    fn a_corrupt_database_has_no_tasks() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        std::fs::write(&path, b"not a redb database").expect("failed to write garbage");

        let store = Store::open(&path);
        assert!(store.tasks().is_empty());
        assert_eq!(store.add_task("ignored", 1), None);
        assert!(store.tasks().is_empty());
    }

    #[test]
    fn a_fresh_store_has_no_notes() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        assert!(store.notes().is_empty());
    }

    #[test]
    fn a_note_can_be_created_edited_and_deleted() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        let id = store
            .upsert_note(None, " First ", "draft", 100)
            .expect("a new note should get an id");
        let notes = store.notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, id);
        assert_eq!(notes[0].title, "First");
        assert_eq!(notes[0].body, "draft");
        assert_eq!(notes[0].updated_at_ms, 100);

        assert_eq!(
            store.upsert_note(Some(id), "Edited", "new\nbody", 200),
            Some(id)
        );
        let notes = store.notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Edited");
        assert_eq!(notes[0].body, "new\nbody");
        assert_eq!(notes[0].updated_at_ms, 200);

        store.delete_note(id);
        assert!(store.notes().is_empty());
        assert_eq!(store.upsert_note(Some(id), "gone", "", 300), None);
    }

    #[test]
    fn notes_survive_dropping_and_reopening_the_store() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let store = Store::open(&path);
            store.upsert_note(None, "Trip", "line one\nline two", 10);
        }

        let reopened = Store::open(&path);
        let notes = reopened.notes();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Trip");
        assert_eq!(notes[0].body, "line one\nline two");
        assert_eq!(notes[0].updated_at_ms, 10);
    }

    #[test]
    fn the_note_list_is_bounded_and_newest_first_by_update_time() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));

        for n in 0..(NOTE_HISTORY_LIMIT as u64 + 5) {
            store.upsert_note(None, &format!("note {n}"), "", n);
        }
        let newest = store
            .upsert_note(None, "just edited", "", 10_000)
            .expect("valid note");
        let oldest = store
            .upsert_note(None, "oldest", "", 0)
            .expect("valid note");

        let notes = store.notes();
        assert_eq!(notes.len(), NOTE_HISTORY_LIMIT);
        assert_eq!(notes[0].id, newest);
        assert_eq!(notes[0].title, "just edited");
        assert!(!notes.iter().any(|note| note.id == oldest));
    }

    #[test]
    fn malformed_note_rows_are_skipped() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        {
            let db = redb::Database::create(&path).expect("failed to create the fixture DB");
            let txn = db.begin_write().expect("failed to begin a fixture write");
            {
                let mut table = txn.open_table(NOTES_TABLE).expect("failed to open table");
                table.insert(1, "not-a-note").expect("failed to insert");
            }
            txn.commit().expect("failed to commit the fixture");
        }
        let store = Store::open(&path);

        let id = store
            .upsert_note(None, "valid", "body", 2)
            .expect("valid note");
        assert_eq!(
            store
                .notes()
                .into_iter()
                .map(|note| note.id)
                .collect::<Vec<_>>(),
            vec![id]
        );
    }

    #[test]
    fn a_corrupt_database_has_no_notes() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let path = dir.path().join("companion.db");
        std::fs::write(&path, b"not a redb database").expect("failed to write garbage");

        let store = Store::open(&path);
        assert!(store.notes().is_empty());
        assert_eq!(store.upsert_note(None, "ignored", "", 1), None);
        assert!(store.notes().is_empty());
    }

    #[test]
    fn start_of_today_ms_points_at_local_midnight_when_it_exists() {
        use chrono::{DateTime, Local, TimeZone, Timelike};

        let now = Local::now();
        let cutoff = Store::start_of_today_ms();
        assert!(cutoff <= now.timestamp_millis().max(0) as u64);

        let midnight_exists = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| Local.from_local_datetime(&naive).single())
            .is_some();
        if midnight_exists {
            let boundary = DateTime::from_timestamp_millis(cutoff as i64)
                .expect("a valid millisecond timestamp")
                .with_timezone(&Local);
            assert_eq!(boundary.date_naive(), now.date_naive());
            assert_eq!(
                (boundary.hour(), boundary.minute(), boundary.second()),
                (0, 0, 0)
            );
        }
    }

    const DAY_START: u64 = 1_000_000;

    #[test]
    fn sessions_since_include_today_but_exclude_yesterday_and_exceed_the_display_limit() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));
        store.save_focus_session(session(DAY_START - 1, 1500));
        for n in 0..(FOCUS_HISTORY_LIMIT as u64 + 5) {
            store.save_focus_session(session(DAY_START + n, 60));
        }

        let rows = store.focus_sessions_since(DAY_START);

        assert_eq!(rows.len(), FOCUS_HISTORY_LIMIT + 5);
        assert!(rows.iter().all(|row| row.started_at_ms >= DAY_START));
        assert!(rows.iter().any(|row| row.started_at_ms == DAY_START));
        assert!(rows.iter().all(|row| row.started_at_ms != DAY_START - 1));
    }

    #[test]
    fn completed_tasks_since_include_today_but_exclude_yesterday_and_open_tasks() {
        let dir = tempfile::tempdir().expect("failed to create a temp dir");
        let store = Store::open(&dir.path().join("companion.db"));
        let yesterday = store.add_task("yesterday", 100).expect("valid task");
        store.complete_task(yesterday, DAY_START - 1);
        let today = store.add_task("today", DAY_START).expect("valid task");
        store.complete_task(today, DAY_START);
        let open = store.add_task("open", DAY_START + 1).expect("valid task");
        for n in 0..(TASK_HISTORY_LIMIT as u64 + 5) {
            let id = store
                .add_task("busy day", DAY_START + n)
                .expect("valid task");
            store.complete_task(id, DAY_START + n);
        }

        let rows = store.completed_tasks_since(DAY_START);

        assert_eq!(rows.len(), TASK_HISTORY_LIMIT + 6);
        assert!(
            rows.iter()
                .all(|row| row.done && row.completed_at_ms.is_some_and(|ms| ms >= DAY_START))
        );
        assert!(rows.iter().any(|row| row.id == today));
        assert!(rows.iter().all(|row| row.id != yesterday && row.id != open));
    }
}
