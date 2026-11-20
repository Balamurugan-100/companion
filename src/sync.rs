//! Opt-in GitHub settings sync.
//!
//! Nothing in this module runs unless the user supplies a repository. Git is
//! used only as a transport: the app never stores a token, never prompts for
//! credentials, and only ever writes its own `companion/settings.json` file.

use crate::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// The only file this feature writes inside the user's repository.
pub const SETTINGS_FILE: &str = "companion/settings.json";
pub const DEFAULT_INTERVAL_HOURS: u64 = 12;

/// Setting keys that participate in a sync. Unknown remote keys are ignored.
pub const SETTING_KEYS: [&str; 8] = [
    settings::FOCUS_KEY,
    settings::BREAK_KEY,
    settings::CALM_KEY,
    settings::OPACITY_KEY,
    settings::BREATH_INHALE_KEY,
    settings::BREATH_HOLD_KEY,
    settings::BREATH_EXHALE_KEY,
    settings::BREATH_SESSION_KEY,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// A different repo was chosen: take the remote settings.
    PullLocal,
    /// A different repo was chosen: overwrite the remote with local settings.
    PushRemote,
    /// Three-way merge; the user resolves each differing field.
    RebaseMerge,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::RebaseMerge
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Local,
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    pub repo_url: String,
    pub interval_hours: u64,
    pub mode: SyncMode,
    pub setup_declined: bool,
    pub last_sync_ms: u64,
    pub device_id: String,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            repo_url: String::new(),
            interval_hours: DEFAULT_INTERVAL_HOURS,
            mode: SyncMode::RebaseMerge,
            setup_declined: false,
            last_sync_ms: 0,
            device_id: new_device_id(),
        }
    }
}

/// One field's value and when it was last written. Timestamps let us tell a
/// one-sided edit from a real conflict instead of guessing from values alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub value: String,
    pub updated_at_ms: u64,
    pub device_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub device_id: String,
    pub fields: BTreeMap<String, Field>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub key: String,
    pub label: String,
    pub base: String,
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub document: Document,
    pub settings: Settings,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub remote_document: Document,
    pub final_document: Document,
    pub settings: Settings,
    pub conflicts: Vec<Conflict>,
    pub status: String,
}

/// Wall clock in milliseconds, clamped to `u64`.
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn new_device_id() -> String {
    format!("{:x}-{}", now_millis(), std::process::id())
}

fn label_for(key: &str) -> String {
    match key {
        settings::FOCUS_KEY => "Focus minutes".to_owned(),
        settings::BREAK_KEY => "Break minutes".to_owned(),
        settings::CALM_KEY => "Calm mode".to_owned(),
        settings::OPACITY_KEY => "Window opacity".to_owned(),
        settings::BREATH_INHALE_KEY => "Inhale seconds".to_owned(),
        settings::BREATH_HOLD_KEY => "Hold seconds".to_owned(),
        settings::BREATH_EXHALE_KEY => "Exhale seconds".to_owned(),
        settings::BREATH_SESSION_KEY => "Breathing session seconds".to_owned(),
        other => other.to_owned(),
    }
}

fn value_for(settings: &Settings, key: &str) -> String {
    match key {
        settings::FOCUS_KEY => settings.focus_minutes.to_string(),
        settings::BREAK_KEY => settings.break_minutes.to_string(),
        settings::CALM_KEY => settings.calm_mode.to_string(),
        settings::OPACITY_KEY => settings.window_opacity.to_string(),
        settings::BREATH_INHALE_KEY => settings.breath_inhale_secs.to_string(),
        settings::BREATH_HOLD_KEY => settings.breath_hold_secs.to_string(),
        settings::BREATH_EXHALE_KEY => settings.breath_exhale_secs.to_string(),
        settings::BREATH_SESSION_KEY => settings.breath_session_secs.to_string(),
        _ => String::new(),
    }
}

fn set_value(settings: &mut Settings, key: &str, value: &str) {
    match key {
        settings::FOCUS_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.focus_minutes = parsed;
            }
        }
        settings::BREAK_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.break_minutes = parsed;
            }
        }
        settings::CALM_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.calm_mode = parsed;
            }
        }
        settings::OPACITY_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.window_opacity = settings::clamp_opacity(parsed);
            }
        }
        settings::BREATH_INHALE_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.breath_inhale_secs = parsed;
            }
        }
        settings::BREATH_HOLD_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.breath_hold_secs = parsed;
            }
        }
        settings::BREATH_EXHALE_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.breath_exhale_secs = parsed;
            }
        }
        settings::BREATH_SESSION_KEY => {
            if let Ok(parsed) = value.parse() {
                settings.breath_session_secs = parsed;
            }
        }
        _ => {}
    }
}

/// Builds a document from settings, preserving the timestamp of a field that
/// did not actually change so a no-op save cannot create false conflicts.
pub fn document_from_settings(
    settings: &Settings,
    previous: Option<&Document>,
    now_ms: u64,
    device_id: &str,
) -> Document {
    let mut fields = BTreeMap::new();
    for key in SETTING_KEYS {
        let value = value_for(settings, key);
        let field = previous
            .and_then(|document| document.fields.get(key))
            .filter(|old| old.value == value)
            .cloned()
            .unwrap_or(Field {
                value,
                updated_at_ms: now_ms,
                device_id: device_id.to_owned(),
            });
        fields.insert(key.to_owned(), field);
    }
    Document {
        device_id: device_id.to_owned(),
        fields,
    }
}

pub fn settings_from_document(document: &Document) -> Settings {
    let mut settings = settings::defaults();
    for key in SETTING_KEYS {
        if let Some(field) = document.fields.get(key) {
            set_value(&mut settings, key, &field.value);
        }
    }
    settings.window_opacity = settings::clamp_opacity(settings.window_opacity);
    settings::clamp_breathing(&mut settings);
    settings
}

fn field_or_default(document: &Document, key: &str) -> Option<Field> {
    document.fields.get(key).cloned().or_else(|| {
        let settings = settings::defaults();
        let value = value_for(&settings, key);
        (!value.is_empty()).then(|| Field {
            value,
            updated_at_ms: 0,
            device_id: String::new(),
        })
    })
}

/// Three-way field merge. A field changed on only one side is adopted from
/// that side; a field changed differently on both sides is a conflict and is
/// left with the local value until the user picks a side.
pub fn merge(base: &Document, local: &Document, remote: &Document) -> MergeResult {
    let mut keys = BTreeSet::new();
    for key in SETTING_KEYS {
        keys.insert(key.to_owned());
    }
    for key in local.fields.keys().chain(remote.fields.keys()) {
        if SETTING_KEYS.contains(&key.as_str()) {
            keys.insert(key.clone());
        }
    }

    let mut fields = BTreeMap::new();
    let mut conflicts = Vec::new();
    for key in keys {
        let base_value = base
            .fields
            .get(&key)
            .map_or(String::new(), |field| field.value.clone());
        let local_field = field_or_default(local, &key);
        let remote_field = field_or_default(remote, &key);
        let local_value = local_field
            .as_ref()
            .map_or(String::new(), |field| field.value.clone());
        let remote_value = remote_field
            .as_ref()
            .map_or(String::new(), |field| field.value.clone());

        let local_changed = local_field.is_some() && local_value != base_value;
        let remote_changed = remote_field.is_some() && remote_value != base_value;
        let conflict = local_changed && remote_changed && local_value != remote_value;

        let chosen = if conflict {
            local_field
                .or(remote_field)
                .expect("a changed field must exist")
        } else if remote_changed && !local_changed {
            remote_field.or(local_field).expect("remote field exists")
        } else {
            local_field.or(remote_field).expect("a field must exist")
        };

        if conflict {
            conflicts.push(Conflict {
                label: label_for(&key),
                key: key.clone(),
                base: base_value,
                local: local_value,
                remote: remote_value,
            });
        }
        fields.insert(key, chosen);
    }

    let document = Document {
        device_id: local.device_id.clone(),
        fields,
    };
    let settings = settings_from_document(&document);
    MergeResult {
        document,
        settings,
        conflicts,
    }
}

/// Applies the user's per-field choices to a merged document.
pub fn apply_choices(
    document: &mut Document,
    conflicts: &[Conflict],
    choices: &BTreeMap<String, Side>,
    remote: &Document,
) {
    for conflict in conflicts {
        if choices.get(&conflict.key) == Some(&Side::Remote)
            && let Some(field) = remote.fields.get(&conflict.key)
        {
            document.fields.insert(conflict.key.clone(), field.clone());
        }
    }
}

/// App data directory for the local clone. `None` means sync cannot be used.
pub fn sync_root() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("companion"))
}

fn clone_dir(root: &Path, repo_url: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    repo_url.hash(&mut hasher);
    root.join("sync").join(format!("{:016x}", hasher.finish()))
}

fn git_output(dir: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new("git");
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .args(args);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    command
        .output()
        .map_err(|error| format!("Could not run git: {error}"))
}

fn git(dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let output = git_output(dir, args)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if message.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            message
        })
    }
}

fn write_document(dir: &Path, document: &Document) -> Result<(), String> {
    let path = dir.join(SETTINGS_FILE);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let raw = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
    std::fs::write(path, format!("{raw}\n")).map_err(|error| error.to_string())
}

fn read_remote_document(dir: &Path) -> Document {
    git(Some(dir), &["show", &format!("FETCH_HEAD:{SETTINGS_FILE}")])
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn has_staged_changes(dir: &Path) -> Result<bool, String> {
    let output = git_output(
        Some(dir),
        &["diff", "--cached", "--quiet", "--", SETTINGS_FILE],
    )?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
    }
}

fn commit_document(dir: &Path, message: &str) -> Result<(), String> {
    git(Some(dir), &["add", "--", SETTINGS_FILE])?;
    if has_staged_changes(dir)? {
        git(Some(dir), &["commit", "-m", message])?;
    }
    Ok(())
}

fn branch_name(dir: &Path) -> Result<String, String> {
    let branch = git(Some(dir), &["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(if branch == "HEAD" {
        "main".to_owned()
    } else {
        branch
    })
}

fn ensure_clone(repo_url: &str, dir: &Path) -> Result<(), String> {
    if dir.join(".git").exists() {
        git(Some(dir), &["remote", "set-url", "origin", repo_url])?;
    } else {
        if dir.exists() {
            std::fs::remove_dir_all(dir).map_err(|error| error.to_string())?;
        }
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        git(
            None,
            &["clone", "--quiet", repo_url, &dir.to_string_lossy()],
        )?;
    }

    git(Some(dir), &["config", "user.name", "Companion"])?;
    git(Some(dir), &["config", "user.email", "companion@local"])?;

    // An empty repository has no HEAD to fetch. Seed it once with our file.
    if git(Some(dir), &["rev-parse", "--verify", "HEAD"]).is_err() {
        write_document(dir, &Document::default())?;
        git(Some(dir), &["add", "--", SETTINGS_FILE])?;
        git(
            Some(dir),
            &["commit", "-m", "Initialize companion settings"],
        )?;
        let branch = branch_name(dir)?;
        git(
            Some(dir),
            &["push", "-u", "origin", &format!("HEAD:{branch}")],
        )?;
    }
    Ok(())
}

/// Runs one sync. This is safe to call from a worker thread: it only uses the
/// supplied plain data, git, and the local clone under `root`.
pub fn run_sync(
    repo_url: &str,
    mode: SyncMode,
    local: &Document,
    base: &Document,
    root: &Path,
) -> Result<SyncResult, String> {
    if repo_url.trim().is_empty() {
        return Err("No GitHub repository configured".to_owned());
    }
    let dir = clone_dir(root, repo_url);
    ensure_clone(repo_url, &dir)?;

    let branch = branch_name(&dir)?;
    git(Some(&dir), &["fetch", "--quiet", "origin", &branch])?;
    let remote = read_remote_document(&dir);
    // Start every sync from the fetched remote. The merge result already
    // accounts for local edits, so this avoids stale commits and push rejections.
    git(Some(&dir), &["reset", "--hard", "--quiet", "FETCH_HEAD"])?;

    match mode {
        SyncMode::PullLocal => {
            write_document(&dir, &remote)?;
            Ok(SyncResult {
                settings: settings_from_document(&remote),
                remote_document: remote.clone(),
                final_document: remote,
                conflicts: Vec::new(),
                status: "Pulled settings from GitHub".to_owned(),
            })
        }
        SyncMode::PushRemote => {
            write_document(&dir, local)?;
            commit_document(&dir, "Update companion settings")?;
            git(
                Some(&dir),
                &[
                    "push",
                    "--force-with-lease",
                    "origin",
                    &format!("HEAD:{branch}"),
                ],
            )?;
            Ok(SyncResult {
                settings: settings_from_document(local),
                remote_document: remote,
                final_document: local.clone(),
                conflicts: Vec::new(),
                status: "Pushed local settings to GitHub".to_owned(),
            })
        }
        SyncMode::RebaseMerge => {
            let merged = merge(base, local, &remote);
            write_document(&dir, &merged.document)?;
            if merged.conflicts.is_empty() {
                commit_document(&dir, "Sync companion settings")?;
                git(Some(&dir), &["push", "origin", &format!("HEAD:{branch}")])?;
                Ok(SyncResult {
                    settings: merged.settings,
                    remote_document: remote,
                    final_document: merged.document,
                    conflicts: Vec::new(),
                    status: "Settings synced with GitHub".to_owned(),
                })
            } else {
                Ok(SyncResult {
                    settings: merged.settings,
                    remote_document: remote,
                    final_document: merged.document,
                    conflicts: merged.conflicts,
                    status: "Settings conflicts need your choice".to_owned(),
                })
            }
        }
    }
}

/// Commits and pushes settings after the user resolves every conflict.
pub fn push_resolved(repo_url: &str, document: &Document, root: &Path) -> Result<String, String> {
    let dir = clone_dir(root, repo_url);
    ensure_clone(repo_url, &dir)?;
    let branch = branch_name(&dir)?;
    git(Some(&dir), &["fetch", "--quiet", "origin", &branch])?;
    git(Some(&dir), &["reset", "--hard", "--quiet", "FETCH_HEAD"])?;
    write_document(&dir, document)?;
    commit_document(&dir, "Resolve companion settings conflicts")?;
    git(Some(&dir), &["push", "origin", &format!("HEAD:{branch}")])?;
    Ok("Resolved settings pushed to GitHub".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(values: &[(&str, &str)], timestamp: u64) -> Document {
        let mut fields = BTreeMap::new();
        for (key, value) in values {
            fields.insert(
                (*key).to_owned(),
                Field {
                    value: (*value).to_owned(),
                    updated_at_ms: timestamp,
                    device_id: "test".to_owned(),
                },
            );
        }
        Document {
            device_id: "test".to_owned(),
            fields,
        }
    }

    #[test]
    fn settings_round_trip_through_a_document() {
        let mut settings = settings::defaults();
        settings.focus_minutes = 42;
        settings.window_opacity = 0.55;
        settings.calm_mode = true;
        let document = document_from_settings(&settings, None, 100, "device");
        assert_eq!(settings_from_document(&document), settings);
    }

    #[test]
    fn a_no_op_save_keeps_the_existing_timestamp() {
        let first = document_from_settings(&settings::defaults(), None, 100, "device");
        let second = document_from_settings(&settings::defaults(), Some(&first), 200, "device");
        assert_eq!(first, second);
    }

    #[test]
    fn a_changed_field_gets_a_new_timestamp() {
        let mut settings = settings::defaults();
        let first = document_from_settings(&settings, None, 100, "device");
        settings.focus_minutes = 30;
        let second = document_from_settings(&settings, Some(&first), 200, "device");
        assert_eq!(second.fields[settings::FOCUS_KEY].updated_at_ms, 200);
        assert_eq!(second.fields[settings::BREAK_KEY].updated_at_ms, 100);
    }

    #[test]
    fn a_one_sided_local_edit_is_adopted() {
        let base = doc(&[(settings::FOCUS_KEY, "25")], 1);
        let local = doc(&[(settings::FOCUS_KEY, "30")], 2);
        let merged = merge(&base, &local, &base);
        assert!(merged.conflicts.is_empty());
        assert_eq!(merged.settings.focus_minutes, 30);
    }

    #[test]
    fn a_one_sided_remote_edit_is_adopted() {
        let base = doc(&[(settings::FOCUS_KEY, "25")], 1);
        let remote = doc(&[(settings::FOCUS_KEY, "40")], 2);
        let merged = merge(&base, &base, &remote);
        assert!(merged.conflicts.is_empty());
        assert_eq!(merged.settings.focus_minutes, 40);
    }

    #[test]
    fn both_sides_editing_the_same_value_is_not_a_conflict() {
        let base = doc(&[(settings::FOCUS_KEY, "25")], 1);
        let local = doc(&[(settings::FOCUS_KEY, "30")], 2);
        let remote = doc(&[(settings::FOCUS_KEY, "30")], 3);
        let merged = merge(&base, &local, &remote);
        assert!(merged.conflicts.is_empty());
        assert_eq!(merged.settings.focus_minutes, 30);
    }

    #[test]
    fn conflicting_edits_are_reported_with_both_values() {
        let base = doc(&[(settings::FOCUS_KEY, "25")], 1);
        let local = doc(&[(settings::FOCUS_KEY, "30")], 2);
        let remote = doc(&[(settings::FOCUS_KEY, "40")], 3);
        let merged = merge(&base, &local, &remote);
        assert_eq!(merged.conflicts.len(), 1);
        assert_eq!(merged.conflicts[0].key, settings::FOCUS_KEY);
        assert_eq!(merged.conflicts[0].local, "30");
        assert_eq!(merged.conflicts[0].remote, "40");
        assert_eq!(merged.settings.focus_minutes, 30);
    }

    #[test]
    fn the_user_can_choose_the_remote_side_of_a_conflict() {
        let base = doc(&[(settings::FOCUS_KEY, "25")], 1);
        let local = doc(&[(settings::FOCUS_KEY, "30")], 2);
        let remote = doc(&[(settings::FOCUS_KEY, "40")], 3);
        let mut merged = merge(&base, &local, &remote);
        let choices = BTreeMap::from([(settings::FOCUS_KEY.to_owned(), Side::Remote)]);
        apply_choices(&mut merged.document, &merged.conflicts, &choices, &remote);
        assert_eq!(settings_from_document(&merged.document).focus_minutes, 40);
    }

    #[test]
    fn out_of_range_remote_values_are_clamped() {
        let remote = doc(
            &[
                (settings::OPACITY_KEY, "9.0"),
                (settings::BREATH_INHALE_KEY, "999"),
            ],
            1,
        );
        let settings = settings_from_document(&remote);
        assert_eq!(settings.window_opacity, settings::MAX_OPACITY);
        assert_eq!(
            settings.breath_inhale_secs,
            *settings::BREATH_INHALE_RANGE.end()
        );
    }

    fn git(args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .output()
            .expect("git should run in tests");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn bare_repo(path: &Path) {
        git(&[
            "init",
            "--bare",
            "--initial-branch=main",
            &path.to_string_lossy(),
        ]);
    }

    fn seed_remote(bare: &Path, work: &Path, document: &Document) {
        git(&[
            "clone",
            "--quiet",
            &bare.to_string_lossy(),
            &work.to_string_lossy(),
        ]);
        git(&["-C", &work.to_string_lossy(), "config", "user.name", "Seed"]);
        git(&[
            "-C",
            &work.to_string_lossy(),
            "config",
            "user.email",
            "seed@local",
        ]);
        write_document(work, document).expect("seed document");
        git(&["-C", &work.to_string_lossy(), "add", "--", SETTINGS_FILE]);
        git(&[
            "-C",
            &work.to_string_lossy(),
            "commit",
            "-m",
            "Seed settings",
        ]);
        git(&[
            "-C",
            &work.to_string_lossy(),
            "push",
            "-u",
            "origin",
            "HEAD:main",
        ]);
    }

    fn remote_document(bare: &Path) -> Document {
        let raw = git(&[
            "--git-dir",
            &bare.to_string_lossy(),
            "show",
            &format!("main:{SETTINGS_FILE}"),
        ]);
        serde_json::from_str(&raw).expect("remote document should parse")
    }

    #[test]
    fn a_local_sync_updates_a_local_bare_repository() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let bare = dir.path().join("remote.git");
        let seed = dir.path().join("seed");
        let app = dir.path().join("app");
        bare_repo(&bare);
        let initial = document_from_settings(&settings::defaults(), None, 1, "seed");
        seed_remote(&bare, &seed, &initial);

        let mut local_settings = settings::defaults();
        local_settings.focus_minutes = 55;
        let local = document_from_settings(&local_settings, Some(&initial), 2, "device");
        let result = run_sync(
            &bare.to_string_lossy(),
            SyncMode::PushRemote,
            &local,
            &initial,
            &app,
        )
        .expect("sync should succeed");

        assert!(result.conflicts.is_empty());
        assert_eq!(result.settings.focus_minutes, 55);
        assert_eq!(
            remote_document(&bare).fields[settings::FOCUS_KEY].value,
            "55"
        );
    }

    #[test]
    fn a_pull_sync_adopts_the_remote_settings() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().expect("temp dir");
        let bare = dir.path().join("remote.git");
        let seed = dir.path().join("seed");
        let app = dir.path().join("app");
        bare_repo(&bare);
        let mut remote_settings = settings::defaults();
        remote_settings.break_minutes = 17;
        let remote = document_from_settings(&remote_settings, None, 1, "seed");
        seed_remote(&bare, &seed, &remote);

        let local = document_from_settings(&settings::defaults(), None, 2, "device");
        let result = run_sync(
            &bare.to_string_lossy(),
            SyncMode::PullLocal,
            &local,
            &Document::default(),
            &app,
        )
        .expect("sync should succeed");

        assert_eq!(result.settings.break_minutes, 17);
    }
}
