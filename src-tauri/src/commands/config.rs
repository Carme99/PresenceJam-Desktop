//! Configuration load/save Tauri commands.
//!
//! See issue #76.

use crate::config::{self, AppConfig, ConfigPatch, QuietHoursEntry};
use crate::teams::{self, TeamsApiError};
use crate::AppState;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.CONFIG]";

#[tauri::command]
pub fn load_config() -> Result<AppConfig, String> {
    log::debug!("{CMD} load_config: ENTRY");
    match config::load_config() {
        Ok(cfg) => {
            log::info!(
                "{CMD} load_config: SUCCESS - spotify.client_id.len={}",
                cfg.spotify.client_id.len()
            );
            Ok(cfg)
        }
        Err(e) => {
            log::error!("{CMD} load_config: FAILED - {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
/// Returns the config as PERSISTED (clamped), so the caller can adopt the
/// same value. Issue #297: the frontend previously stored its own unclamped
/// input, so the UI showed a value that was never written to disk.
///
/// Whole-document replace: the caller must already hold a complete, current
/// `AppConfig` (Settings clones the loaded store, so it does). A caller that
/// only knows part of the document must use [`update_config`] instead —
/// this command will happily wipe every field it did not receive (the
/// backend half of the #531 family, CfgDiag#0 / issue #535).
pub async fn save_config(
    app: AppHandle,
    config: AppConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfig, String> {
    log::info!(
        "{CMD} save_config: ENTRY - config.spotify.client_id.len={}",
        config.spotify.client_id.len()
    );

    // #215: serialization + atomic_write_json (fsync) holds the write lock
    // across IO. Offload the entire read-modify-write critical section to
    // the blocking pool so the async runtime is not blocked and the lock
    // is not held across an await.
    let state_clone = Arc::clone(state.inner());
    // Issue #297: `save_config` persists a CLAMPED copy, so store that same
    // value in AppState. Storing the raw input left the in-memory config (the
    // one the polling loop reads) disagreeing with config.json until restart.
    let config_clone = config::clamped_config(&config);
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        // Hold the write lock for the entire read-modify-write to prevent races
        // with concurrent reads from the polling loop. See bug #26.
        let mut config_guard = state_clone.config.get_mut();
        match config::save_config(&config_clone) {
            Ok(()) => {
                log::info!("{CMD} save_config: file saved successfully");
                // Issue #536: `save_config` stamps the binary-owned schema
                // version, so the in-memory copy must carry the same value
                // that reached disk (the #297 invariant).
                let mut persisted = config_clone.clone();
                config::stamp_schema_version(&mut persisted);
                *config_guard = Some(persisted.clone());
                Ok::<AppConfig, String>(persisted)
            }
            Err(e) => {
                log::error!("{CMD} save_config: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("save_config spawn_blocking panicked: {:?}", e))??;

    after_persist(&app, &persisted).await;
    log::info!("{CMD} save_config: SUCCESS");
    Ok(persisted)
}

#[tauri::command]
/// Apply a field-level patch to the stored config and return what was
/// actually persisted (clamped, with the binary-owned schema version).
///
/// CfgDiag#0 (issue #535): `save_config` replaces the whole document, so a
/// caller that only knows some of it silently resets the rest. This command
/// merges instead — a caller can name `teams.status_format` and be certain
/// the user's quiet hours, track rules, logging level and every other field
/// are still there afterwards.
///
/// The base is the in-memory config (what the polling loop reads and what
/// the last write stored); before the first load has run, it is read from
/// disk. Either way the read-modify-write happens inside the same single
/// write guard `save_config` uses, so a concurrent write cannot interleave.
pub async fn update_config(
    app: AppHandle,
    patch: ConfigPatch,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfig, String> {
    log::info!("{CMD} update_config: ENTRY");

    // #215 pattern: the whole read-merge-write critical section runs on the
    // blocking pool, holding the config write lock across the fsync.
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        let mut config_guard = state_clone.config.get_mut();
        let base = match config_guard.as_ref() {
            Some(current) => current.clone(),
            None => config::load_config()?,
        };

        let mut merged = base;
        config::apply_patch(&mut merged, &patch);

        let mut persisted = config::clamped_config(&merged);
        config::stamp_schema_version(&mut persisted);
        match config::save_config(&persisted) {
            Ok(()) => {
                *config_guard = Some(persisted.clone());
                Ok::<AppConfig, String>(persisted)
            }
            Err(e) => {
                log::error!("{CMD} update_config: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("update_config spawn_blocking panicked: {:?}", e))??;

    after_persist(&app, &persisted).await;
    log::info!("{CMD} update_config: SUCCESS");
    Ok(persisted)
}

/// Issue #876: imports the user's Outlook "Work hours" tab as a preview
/// list of [`QuietHoursEntry`] candidates. **Does NOT persist** — the
/// caller (Settings.svelte) shows the preview and, on confirm, applies
/// the new list through [`update_config`] so the same read-merge-write
/// lock every other config edit uses is honoured.
///
/// The scope `MailboxSettings.Read` is required. Existing sessions must
/// re-consent once (the scope is in `MICROSOFT_GRAPH_SCOPES` from 5.0
/// onward); a tenant that refuses the scope fails with a typed
/// [`TeamsApiError::Forbidden`] which the command translates into a
/// user-visible reconnect prompt. A missing token or a missing scope
/// returns an empty preview + a message — the Settings button stays
/// enabled so the user can fix the auth state and retry.
#[tauri::command]
pub async fn import_working_hours(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ImportWorkingHoursPreview, String> {
    log::info!("{CMD} import_working_hours: ENTRY");

    // Issue #876: gate on the granted scopes (mirrors the
    // `get_teams_granted_scopes` use in `Settings.svelte` for the
    // one-time reconnect banner). A token without `MailboxSettings.Read`
    // would 403 the GET, so check first and return a typed message
    // instead of a network round-trip + cryptic body.
    let access_token = {
        let guard = state.tokens.teams();
        match guard.as_ref() {
            Some(tokens) => tokens.access_token.clone(),
            None => {
                return Ok(ImportWorkingHoursPreview::missing_token());
            }
        }
    };
    let granted = teams::decode_teams_granted_scopes(&access_token);
    if !granted.iter().any(|s| s == "MailboxSettings.Read") {
        log::warn!(
            "{CMD} import_working_hours: MailboxSettings.Read not in granted scopes; \
             tenant has not yet consented to the new scope"
        );
        return Ok(ImportWorkingHoursPreview::missing_scope());
    }

    // The HTTP round-trip is blocking; offload so the IPC thread is
    // not frozen. `spawn_blocking` is the same offload the other
    // Graph-touching commands use.
    let working =
        tauri::async_runtime::spawn_blocking(move || teams::get_working_hours(&access_token))
            .await
            .map_err(|e| format!("import_working_hours spawn_blocking panicked: {:?}", e))?;

    match working {
        Ok(working) => {
            let entries = invert_working_hours(&working);
            log::info!(
                "{CMD} import_working_hours: produced {} candidate quiet-hours entries \
                 (work_start={}, work_end={}, work_days={:?}, tz_offset_min={})",
                entries.len(),
                working.start_minutes,
                working.end_minutes,
                working.days,
                working.time_zone_offset_minutes,
            );
            Ok(ImportWorkingHoursPreview::preview(entries, working))
        }
        Err(TeamsApiError::Forbidden(_, body)) => {
            log::warn!(
                "{CMD} import_working_hours: forbidden by Graph (tenant refused the scope): {}",
                truncate_for_preview(&body)
            );
            Ok(ImportWorkingHoursPreview::forbidden())
        }
        Err(e) => Err(e.user_message()),
    }
}

/// The preview payload the Settings UI renders before the user
/// confirms. `entries` is the list of [`QuietHoursEntry`] the Settings
/// component will splice into the user's `quiet_hours` table on apply;
/// `working` is the raw `WorkingHours` (start/end minutes + days +
/// offset) for the "this is what Outlook returned" hint. `message` is
/// a non-empty string for the four non-preview variants — the UI shows
/// it in the same banner the Settings page uses for `update_config`
/// errors, so the tone and the action (reconnect / try again / etc.)
/// are documented in one place.
///
/// `Serialize` is required because the command returns this struct
/// across the IPC bridge (`tauri::command` derives the response
/// codec); `ts_rs::TS` exports the matching TypeScript shape so the
/// frontend can render the entries without re-implementing the
/// per-field mapping. The `WorkingHours` shape is opaque on the
/// wire — the UI reads only its `start_minutes`, `end_minutes` and
/// `days` for the hint line, and `extra` is the documented
/// unknown-future-keys overflow (mirroring `AppConfig::extra`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ImportWorkingHoursPreview {
    pub entries: Vec<QuietHoursEntry>,
    #[ts(skip)]
    pub working: Option<crate::teams::WorkingHours>,
    pub message: Option<String>,
}

impl ImportWorkingHoursPreview {
    fn preview(entries: Vec<QuietHoursEntry>, working: crate::teams::WorkingHours) -> Self {
        Self {
            entries,
            working: Some(working),
            message: None,
        }
    }
    fn missing_token() -> Self {
        Self {
            entries: Vec::new(),
            working: None,
            message: Some(
                "Connect Microsoft Teams in Settings before importing working hours.".to_string(),
            ),
        }
    }
    fn missing_scope() -> Self {
        Self {
            entries: Vec::new(),
            working: None,
            message: Some(
                "Microsoft Teams has not granted the MailboxSettings.Read permission. \
                 Reconnect Teams in Settings to add it."
                    .to_string(),
            ),
        }
    }
    fn forbidden() -> Self {
        Self {
            entries: Vec::new(),
            working: None,
            message: Some(
                "Microsoft Teams refused the request: the account or tenant does not \
                 allow reading mailbox settings. Reconnect Teams in Settings."
                    .to_string(),
            ),
        }
    }
}

/// Truncates a Graph error body for the `forbidden` log line so a long
/// `insufficient_claims` payload does not flood the diagnostic snapshot.
fn truncate_for_preview(body: &str) -> String {
    const LIMIT: usize = 240;
    if body.len() <= LIMIT {
        body.to_string()
    } else {
        format!("{}…", &body[..LIMIT])
    }
}

/// Issue #876: inverts a [`crate::teams::WorkingHours`] into a list of
/// [`QuietHoursEntry`] preview entries. One inverted entry per contiguous
/// off-block, with over-midnight blocks split into a single wrap-around
/// entry per working day (the entry model treats a wrap as "on this day,
/// `start..end` OR `0..start`" — two off-blocks bracket the working hours
/// this way without a per-bridge cross-day entry). Days the user did not
/// declare as working become full-day entries (`0..=1440`).
///
/// Pure: no I/O, no `AppState`. The unit tests in this module drive
/// Mon-Fri 09:00-17:00 through every day of the week and assert the
/// exact expected set.
pub fn invert_working_hours(working: &crate::teams::WorkingHours) -> Vec<QuietHoursEntry> {
    use std::collections::BTreeSet;
    let working_days: BTreeSet<u8> = working.days.iter().copied().collect();
    let mut entries = Vec::with_capacity(7);

    // All-day working is treated as "no off-hours". An empty `days`
    // array (an Outlook user who cleared the Work hours tab) is the
    // same case — neither declares a working day, so we emit no
    // entries rather than marking every minute as off.
    if working_days.is_empty() {
        return entries;
    }

    let work_start = working.start_minutes;
    let work_end = working.end_minutes;

    for day in 1..=7u8 {
        if !working_days.contains(&day) {
            // Full day off: start=0, end=1440. The clamp normalises
            // both fields into the documented range (`clamp_quiet_hours_window`
            // runs at config-load time too).
            entries.push(make_off_entry(day, 0, 1440));
            continue;
        }
        if work_start == 0 && work_end == 1440 {
            // Working 24h: no off-hours. Unreachable from a sane
            // Outlook config, but `start >= end` would otherwise produce
            // a wrap that marks part of the day as off.
            continue;
        }
        if work_end > work_start {
            // Day-time working window. Off = [0, start_min] + [end_min, 1440]
            // which collapses into a single wrap entry.
            entries.push(make_off_entry(day, work_end, work_start));
        } else if work_end < work_start {
            // Overnight working window (e.g. 22:00-06:00). Off = [end_min,
            // start_min] during the day (no wrap).
            entries.push(make_off_entry(day, work_end, work_start));
        }
        // work_start == work_end: a zero-width window — emit no entry.
    }
    entries
}

fn make_off_entry(day: u8, start_minutes: u16, end_minutes: u16) -> QuietHoursEntry {
    // `clamp_quiet_hours_window` re-clamps on config load, but
    // pre-clamping here keeps the preview the Settings UI renders the
    // exact values that will be written to disk.
    let start = start_minutes.min(1439);
    let end = end_minutes.min(1440);
    QuietHoursEntry {
        enabled: true,
        start_minutes: start,
        end_minutes: end,
        days: vec![day],
        replacement_status: String::new(),
        presence_availability: String::new(),
        presence_activity: String::new(),
        pause_polling: false,
    }
}

/// Side effects that must follow a successful config write, shared by every
/// write command so they cannot drift apart.
///
/// Ordering is load-bearing: the logger is re-armed first (CfgDiag#4, issue
/// #539 — a `logging.enabled` / `log_level` change takes effect immediately
/// instead of at the next launch, and the level also governs whether the
/// OS-side effects below are logged), then the native locale (4.7.0, issue
/// #674 — see [`sync_native_locale`]), then the macOS activation policy,
/// then the OS autostart entry. `pub(crate)` (not private) because the #811
/// `set_autostart_enabled` command converges through this same path after it
/// persists the flag; the OS half it shares is
/// [`super::window::apply_os_autostart`], split out so this function never
/// re-enters the command and recurses.
#[cfg_attr(not(desktop), allow(unused_variables))]
pub(crate) async fn after_persist(app: &AppHandle, persisted: &AppConfig) {
    config::apply_log_level(&persisted.logging);
    sync_native_locale(app, persisted);

    // On macOS, sync the app's activation policy with the saved
    // `start_minimized` preference so the dock icon disappears when the
    // user wants tray-only behavior and reappears when they disable it.
    // Setting on every save (not just on toggle) keeps the policy
    // idempotent and avoids tracking previous state. See audit Q4.
    #[cfg(target_os = "macos")]
    {
        let policy = if persisted.teams.start_minimized {
            tauri::ActivationPolicy::Accessory
        } else {
            tauri::ActivationPolicy::Regular
        };
        // tauri::AppHandle::set_activation_policy returns () on success;
        // the underlying call logs its own errors via the tauri-runtime-wry
        // layer. We deliberately discard the unit value rather than wrapping
        // in `if let Err(...)`.
        let _ = app.set_activation_policy(policy);
    }

    // Sync autostart state with the OS autostart manager (issue #811: the
    // OS-only half — `apply_os_autostart`, not the `set_autostart_enabled`
    // command, which would persist again and recurse). The command is
    // async (it touches the autostart registry/file), so we await it.
    #[cfg(desktop)]
    {
        if let Err(e) = super::window::apply_os_autostart(app, persisted.autostart).await {
            log::warn!("{CMD} failed to sync autostart state: {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
// 4.7.0 (S5): config export / import.
//
// Both commands own their file dialog so the resolved path is the one the
// file work uses: a path chosen in the webview and handed back over IPC could
// be anything, and the settings file is worth keeping in one place.
//
// The dialog *titles* arrive from the caller: they are user-visible copy, and
// the frontend dictionaries are the only place UI text lives (the i18n rule in
// CLAUDE.md). Error strings stay English, as documented for Rust-side errors.
// ---------------------------------------------------------------------------

/// How many sidecar names an export tries before giving up. A collision means
/// another process is writing the same destination at the same instant; eight
/// attempts is already far past plausible.
const EXPORT_SIDECAR_ATTEMPTS: u32 = 8;

/// The private sidecar an export stages its bytes in:
/// `<dest>.<pid>.<attempt>.pj-export.tmp`.
///
/// Deliberately not the config writer's `<dest>.tmp` (issue #823): that name is
/// shared with whatever else the user keeps in the directory, and
/// `atomic_write_json` pre-clears it.
fn export_sidecar_path(dest: &Path, pid: u32, attempt: u32) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(format!(".{}.{}.pj-export.tmp", pid, attempt));
    dest.with_file_name(name)
}

/// Create `path` exclusively — no pre-clear, mode 0600 on Unix (the #135
/// pattern `atomic_write_json` uses). `AlreadyExists` is reported to the caller
/// rather than cleared away: the name belongs to whoever has it. That includes
/// a *directory* at the name — `O_CREAT|O_EXCL` reports `EEXIST` for any
/// existing entry — so the caller decides whether to try another name.
fn open_exclusive(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}

/// fsync the directory the export landed in, so the rename survives a crash
/// (the same guarantee `atomic_write_json` gives `config.json`).
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                if let Err(e) = dir.sync_all() {
                    log::warn!(
                        "{CMD} export: failed to fsync export dir '{}': {}",
                        parent.display(),
                        e
                    );
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Write an export payload to `dest` through a sidecar private to this call.
///
/// Issue #823: the destination is a path the *user* chose, so it must not go
/// through `config::atomic_write_json`. That writer derives its sidecar as
/// `path.with_extension("tmp")` and removes it unconditionally — correct for
/// `config.json`, which the app owns, but for an export it deleted an
/// unrelated `~/notes.tmp` and, when that path was a directory, failed with an
/// error naming a file the user never created. Here the sidecar carries this
/// process id plus an attempt counter (so a collision picks another suffix
/// instead of clearing anything), and the only path this function ever removes
/// is the one it just created.
fn write_export_file(dest: &Path, json: &str) -> Result<(), String> {
    let pid = std::process::id();
    for attempt in 0..EXPORT_SIDECAR_ATTEMPTS {
        let staged = export_sidecar_path(dest, pid, attempt);
        let mut file = match open_exclusive(&staged) {
            Ok(file) => file,
            // Somebody (or something) else holds this exact name — theirs, not
            // ours: try the next suffix rather than clearing it away. A
            // directory at the name lands here too, since `O_CREAT|O_EXCL`
            // reports `EEXIST` for it; the export then fails naming this call's
            // own sidecar, never a path the user chose.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "Failed to create export sidecar '{}': {}",
                    staged.display(),
                    e
                ))
            }
        };
        if let Err(e) = file.write_all(json.as_bytes()) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!(
                "Failed to write export sidecar '{}': {}",
                staged.display(),
                e
            ));
        }
        if let Err(e) = file.sync_all() {
            let _ = std::fs::remove_file(&staged);
            return Err(format!(
                "Failed to sync export sidecar '{}': {}",
                staged.display(),
                e
            ));
        }
        drop(file);
        if let Err(e) = std::fs::rename(&staged, dest) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!(
                "Failed to rename export sidecar to '{}': {}",
                dest.display(),
                e
            ));
        }
        sync_parent_dir(dest);
        return Ok(());
    }
    Err(format!(
        "Failed to create an export sidecar next to '{}': every candidate name is taken",
        dest.display()
    ))
}

/// Write a shareable copy of the current config to a user-chosen path.
///
/// The document is the persisted shape of the loaded config — clamped, with
/// every `client_secret` key stripped (`config::export_document`) — so the
/// Spotify client secret (keychain-only, issue #9) and any token material can
/// never leave the machine inside a file the user is told to keep or share.
///
/// Returns the path written, or `None` when the dialog was dismissed.
#[tauri::command]
pub async fn export_config(
    app: AppHandle,
    title: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    log::info!("{CMD} export_config: ENTRY");

    // Snapshot before the dialog: the config read guard must not be held
    // across a user-driven wait.
    let current = {
        let guard = state.config.get();
        match guard.as_ref() {
            Some(cfg) => cfg.clone(),
            None => config::load_config()?,
        }
    };
    let json = config::export_document(&current)?;
    let suggested = config::export_file_name(env!("CARGO_PKG_VERSION"), chrono::Utc::now());

    // Issue #885: the picker blocks until the user answers, and that wait is
    // unbounded, so only its terminal call runs on the blocking pool — the
    // same contract `ask_overwrite` documents below. The dialog itself is
    // built here, on the command thread.
    let picker = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(&suggested)
        .add_filter("JSON", &["json"]);
    let chosen = tauri::async_runtime::spawn_blocking(move || picker.blocking_save_file())
        .await
        .map_err(|e| format!("export_config spawn_blocking panicked: {:?}", e))?;
    let Some(chosen) = chosen else {
        log::info!("{CMD} export_config: CANCELLED - dialog dismissed");
        return Ok(None);
    };

    let mut path = chosen
        .into_path()
        .map_err(|e| format!("export_config: unusable destination: {}", e))?;
    // The native dialog does not append the filter's extension on every
    // platform; a file the user cannot tell is JSON is a support ticket.
    if path.extension().is_none() {
        path.set_extension("json");
    }
    // Crash-safe write private to the export — sidecar + fsync + rename, mode
    // 0600 as `save_config` uses — but through a sidecar this call names
    // itself, so a user-chosen destination is never routed through the config
    // writer (issue #823).
    write_export_file(&path, &json)?;

    let written = path.to_string_lossy().into_owned();
    log::info!(
        "{CMD} export_config: SUCCESS - {} bytes to {}",
        json.len(),
        written
    );
    Ok(Some(written))
}

/// What an import did: the document the user chose and the config now on disk.
///
/// Both halves are needed: the Settings card names the file it read (so a
/// user with several exports can tell which one landed), and the config is
/// what was actually persisted (the #297 invariant — the caller adopts that,
/// never its own pre-import copy).
#[derive(serde::Serialize)]
pub struct ImportOutcome {
    pub path: String,
    pub config: AppConfig,
}

/// The overwrite confirmation, shown as the plugin's native message dialog.
///
/// It runs here rather than in the webview because the ACL gates JS dialog
/// calls per window: granting `dialog:default` to the popped-out panes would
/// hand them the whole dialog surface (save/open included) just to show one
/// message box. A Rust-side call needs no capability and behaves identically in
/// the main window and a detached pane.
///
/// Every label is passed in already localized — the plugin's own defaults are
/// English. `OkCancelCustom` is used (not `YesNo`) because the frontend
/// dictionary's `common.yes` / `common.no` are the strings users have seen in
/// every other confirm in the app; the return value is "the custom OK was
/// pressed".
///
/// Blocking on purpose: this is called from the blocking pool (never the main
/// thread), the same `spawn_blocking` shape the file pickers above use.
fn ask_overwrite(
    app: &AppHandle,
    title: &str,
    body: &str,
    ok_label: &str,
    cancel_label: &str,
) -> bool {
    let confirmed = app
        .dialog()
        .message(body)
        .title(title)
        .buttons(tauri_plugin_dialog::MessageDialogButtons::OkCancelCustom(
            ok_label.to_string(),
            cancel_label.to_string(),
        ))
        .kind(tauri_plugin_dialog::MessageDialogKind::Warning)
        .blocking_show();
    log::info!("{CMD} import_config: overwrite confirmation answered: {confirmed}");
    confirmed
}

/// Top-level keys a genuine PresenceJam export always carries. A document with
/// none of them is not one of ours (issue #963).
const IMPORT_SECTION_KEYS: [&str; 11] = [
    "schema_version",
    "spotify",
    "teams",
    "polling",
    "logging",
    "updates",
    "notifications",
    "shortcuts",
    "status_rules",
    "presence_profiles",
    "active_profile",
];

/// Whether `value` is recognisably a PresenceJam configuration (issue #963).
///
/// `AppConfig`'s fields all carry `#[serde(default)]`, so *any* JSON object
/// deserializes — that is what makes this check necessary rather than implied
/// by the schema parse.
fn is_presencejam_document(value: &serde_json::Value) -> bool {
    value.is_object()
        && IMPORT_SECTION_KEYS
            .iter()
            .any(|key| value.get(*key).is_some())
}

/// Read the file the user picked, refusing anything that is not a PresenceJam
/// configuration at all (issue #963).
///
/// Without this check a mis-picked `.json` that shares none of the
/// application's sections still deserializes into a complete, all-defaults
/// config, and the import reports success while the user's Spotify client id,
/// status format, quiet hours, track rules, notification classes, shortcuts and
/// locale are replaced by defaults. A real export always carries the full
/// section set, so a genuine backup is never refused here.
///
/// A malformed file is deliberately left to `config::prepare_import`'s own
/// error, so the "not valid JSON" wording keeps one home.
fn read_import_source(path: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("import_config: failed to read '{}': {}", path.display(), e))?;
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) if !is_presencejam_document(&value) => {
            log::warn!("{CMD} import_config: REFUSED - not a PresenceJam configuration");
            Err(
                "Imported file is not a PresenceJam configuration (no recognisable section: expected schema_version, spotify, teams, polling, logging, updates, notifications, shortcuts, status_rules, presence_profiles or active_profile)"
                    .to_string(),
            )
        }
        _ => Ok(raw),
    }
}

/// The sidecar an imported document is staged in before it replaces the live
/// config: `<config.json>.import.tmp`. Beside the live file, so the final
/// rename stays on one volume and is atomic (issue #939).
fn staged_config_path(path: &Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".import.tmp");
    PathBuf::from(staged)
}

/// Stage `json` beside `path`, move the live file to the quarantine backup, then
/// install the staged copy over `path` (issue #939).
///
/// `config::import_config_document` moves the live file to `config.json.bak`
/// first and writes the imported document only afterwards: a write that fails —
/// disk full, quota, permission denied, an antivirus lock — or a process death
/// between the two steps returns an error with **no live config at all**. The
/// next launch then logs "Config file not found", boots on defaults, and the
/// user's settings exist only in a `.bak` nothing restores. Staging first means
/// a replacement that cannot be written fails before the live file moves; if
/// the final rename fails, the previous document is moved back before the error
/// returns.
///
/// The staged sidecar is app-owned, so a leftover from an import that died
/// mid-flight is pre-cleared exactly as `atomic_write_json` does for
/// `config.json.tmp` (#135 path A) — one crash must not become a permanent
/// import failure.
fn replace_config_file(path: &Path, json: &str) -> Result<(), String> {
    let staged = staged_config_path(path);

    if let Err(e) = std::fs::remove_file(&staged) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale import temp file '{}': {}",
                staged.display(),
                e
            ));
        }
    }

    let mut file = open_exclusive(&staged).map_err(|e| {
        format!(
            "Failed to create import temp file '{}': {}",
            staged.display(),
            e
        )
    })?;
    if let Err(e) = file.write_all(json.as_bytes()) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!(
            "Failed to write import temp file '{}': {}",
            staged.display(),
            e
        ));
    }
    if let Err(e) = file.sync_all() {
        let _ = std::fs::remove_file(&staged);
        return Err(format!(
            "Failed to sync import temp file '{}': {}",
            staged.display(),
            e
        ));
    }
    drop(file);

    // The outgoing file goes to the same backup path the corrupt-file
    // quarantine uses, so an import is never a one-way door. A missing current
    // file is not an error: a fresh install has nothing to back up.
    let backup = config::quarantine_backup_path(path);
    let had_live = path.exists();
    if had_live {
        if let Err(e) = std::fs::rename(path, &backup) {
            let _ = std::fs::remove_file(&staged);
            return Err(format!(
                "Failed to move the current config to '{}': {}",
                backup.display(),
                e
            ));
        }
        log::info!(
            "{CMD} import: previous config moved to '{}'",
            backup.display()
        );
    }

    if let Err(e) = std::fs::rename(&staged, path) {
        log::error!("{CMD} import: FAILED to install the imported config: {}", e);
        let _ = std::fs::remove_file(&staged);
        if had_live {
            match std::fs::rename(&backup, path) {
                Ok(()) => log::warn!("{CMD} import: the previous config was moved back into place"),
                Err(rollback) => log::error!(
                    "{CMD} import: rollback FAILED - the previous config is at '{}': {}",
                    backup.display(),
                    rollback
                ),
            }
        }
        return Err(format!(
            "Failed to install the imported config at '{}': {}",
            path.display(),
            e
        ));
    }

    sync_parent_dir(path);
    Ok(())
}

/// Replace the live document and adopt the reload under one config write
/// guard.
///
/// The guard is acquired before the file replacement and held until the
/// validated reload has been copied into `AppState`. This keeps a competing
/// guarded writer from replacing the imported file (or publishing its own
/// pre-import copy) between those two steps. The caller must run this helper
/// on the blocking pool: both the atomic file replacement and the reload do
/// blocking I/O.
fn replace_and_adopt_config(
    state: &AppState,
    destination: &Path,
    document: &str,
    reload: impl FnOnce() -> Result<AppConfig, String>,
) -> Result<AppConfig, String> {
    let mut config_guard = state.config.get_mut();
    replace_config_file(destination, document)?;
    let persisted = reload()?;
    *config_guard = Some(persisted.clone());
    Ok(persisted)
}

/// Replace the stored config with a document the user picks.
///
/// Validation happens before anything is written: a file that is not a
/// PresenceJam configuration is refused by [`read_import_source`] (issue #963),
/// and a document carrying a plaintext `client_secret` by
/// `config::prepare_import`. The user is asked before the current file is
/// replaced — a fresh install, having nothing to replace, is not asked — and the
/// outgoing copy is kept as `config.json.bak`. Returns `None` when the picker
/// was dismissed or the overwrite was declined (both are clean no-ops).
#[tauri::command]
pub async fn import_config(
    app: AppHandle,
    title: String,
    confirm_body: String,
    confirm_ok: String,
    confirm_cancel: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<ImportOutcome>, String> {
    log::info!("{CMD} import_config: ENTRY");

    // Same blocking-pool contract as the export picker (issue #885): building
    // the dialog is cheap, the wait on the user is not.
    let picker = app
        .dialog()
        .file()
        .set_title(title.clone())
        .add_filter("JSON", &["json"]);
    let chosen = tauri::async_runtime::spawn_blocking(move || picker.blocking_pick_file())
        .await
        .map_err(|e| format!("import_config spawn_blocking panicked: {:?}", e))?;
    let Some(chosen) = chosen else {
        log::info!("{CMD} import_config: CANCELLED - dialog dismissed");
        return Ok(None);
    };
    let source = chosen
        .into_path()
        .map_err(|e| format!("import_config: unusable source: {}", e))?;
    // Issue #963: read and identity-check the picked file before the
    // destination is even resolved — a file that is not a PresenceJam
    // configuration is refused with nothing on disk touched.
    let raw = read_import_source(&source)?;
    let source_path = source.to_string_lossy().into_owned();
    let destination = config::get_config_path()?;

    // Validation before anything is written: `prepare_import` refuses a
    // document carrying a plaintext client_secret and clamps everything it
    // accepts, so the replace below cannot introduce a value the UI could not
    // have saved.
    let prepared = config::prepare_import(&raw)?;

    // The overwrite question is a user-driven wait, so it runs on the blocking
    // pool and *outside* the config write guard: holding the guard across it
    // would stall the polling loop's config reads for as long as the dialog is
    // on screen. A fresh install has nothing to replace and is not asked.
    if destination.exists() {
        let dialog_app = app.clone();
        let confirmed = tauri::async_runtime::spawn_blocking(move || {
            ask_overwrite(
                &dialog_app,
                &title,
                &confirm_body,
                &confirm_ok,
                &confirm_cancel,
            )
        })
        .await
        .map_err(|e| format!("import_config spawn_blocking panicked: {:?}", e))?;
        if !confirmed {
            log::info!("{CMD} import_config: DECLINED - configuration left untouched");
            return Ok(None);
        }
    }

    // Issue #939/#946: the imported document is staged beside the live file and
    // only then installed, so a replace that cannot be written leaves the
    // previous `config.json` in place. Replacement, reload/validation and
    // AppState adoption stay in one blocking-pool critical section under the
    // config write guard; a competing writer cannot slip between the file and
    // the state it publishes.
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        replace_and_adopt_config(
            &state_clone,
            &destination,
            &prepared.document,
            config::load_config,
        )
    })
    .await
    .map_err(|e| format!("import_config spawn_blocking panicked: {:?}", e))??;

    after_persist(&app, &persisted).await;
    log::info!(
        "{CMD} import_config: SUCCESS - imported from {}",
        source_path
    );
    Ok(Some(ImportOutcome {
        path: source_path,
        config: persisted,
    }))
}

/// Installs the persisted locale on the native surfaces and repaints them when
/// it actually changed (4.7.0, issue #674).
///
/// Called from [`after_persist`], so **every** config write path converges —
/// not just the `set_locale` command: a Settings save carrying a stale draft,
/// or an imported config (the 4.7.0 export/import commands), would otherwise
/// move `config.json` and the webview to the new language while the tray and
/// the app menu kept rendering the old one.
///
/// A repaint is skipped when the locale did not change, because it clears the
/// throttled Spotify caches and re-fetches devices/queue; the language is what
/// the cache does not hold, so no other save needs to pay for it.
fn sync_native_locale(app: &AppHandle, persisted: &AppConfig) {
    if !crate::i18n::install_from_config(persisted) {
        return;
    }
    if let Err(e) = crate::menu::rebuild_app_menu(app) {
        log::warn!("{CMD} locale change: app menu rebuild failed: {}", e);
    }
    crate::tray::refresh_tray_for_locale(app);
    log::info!(
        "{CMD} locale change: native surfaces relabelled (locale={:?})",
        persisted.locale
    );
}

#[tauri::command]
/// Persist the UI locale and relabel the native surfaces immediately
/// (4.7.0, issue #674).
///
/// `AppConfig::locale` is the single source of truth for the language: the
/// webview dictionary store reads it at load and writes it here; the tray and
/// the native application menu are relabelled by the shared post-write path
/// ([`after_persist`] / [`sync_native_locale`]).
/// The value is canonicalised before it reaches disk — an unknown tag
/// (`"zz"`, `"pt-BR"`) is stored as `"en"` and the fallback is logged by
/// `i18n::resolve_tag`, so a stored tag and the rendered tables can never
/// disagree.
///
/// A locale change is cosmetic, so a failure to relabel one of the surfaces is
/// logged rather than rolled back: the config write is already committed and
/// the next rebuild (any poll, any tray click) renders the new language.
pub async fn set_locale(
    app: AppHandle,
    locale: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfig, String> {
    log::info!("{CMD} set_locale: ENTRY");

    // Canonicalise before the write so the persisted tag is exactly what the
    // tables render (`i18n::LOCALES`).
    let tag = crate::i18n::resolve_tag(Some(&locale));
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        // Same single write guard as `save_config`/`update_config`: the whole
        // read-modify-write runs on the blocking pool with the lock held
        // across the fsync (issue #215 pattern).
        let mut config_guard = state_clone.config.get_mut();
        let mut merged = match config_guard.as_ref() {
            Some(current) => current.clone(),
            None => config::load_config()?,
        };
        merged.locale = Some(tag.to_string());

        let mut persisted = config::clamped_config(&merged);
        config::stamp_schema_version(&mut persisted);
        match config::save_config(&persisted) {
            Ok(()) => {
                *config_guard = Some(persisted.clone());
                Ok::<AppConfig, String>(persisted)
            }
            Err(e) => {
                log::error!("{CMD} set_locale: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("set_locale spawn_blocking panicked: {:?}", e))??;

    // Converge every surface through the shared post-write path, so this
    // command cannot drift from a generic save (4.7.0, issue #674).
    after_persist(&app, &persisted).await;

    log::info!("{CMD} set_locale: SUCCESS - locale={}", tag);
    Ok(persisted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Production half of this module — everything before the inline test
    /// module, so a scan can never match the assertions themselves.
    fn prod_source(src: &str) -> &str {
        src.split("#[cfg(test)]\nmod tests")
            .next()
            .expect("config.rs has no #[cfg(test)] mod tests block")
    }

    /// Drops `//` line comments so prose that quotes a call cannot satisfy a
    /// scan. String literals are not parsed, so a `//` inside one can only lose
    /// trailing text on that line, never invent a call.
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|line| match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Comment-stripped, brace-counted body isolation for `sig`'s fn.
    /// Order-independent: never anchor on the next fn.
    fn body_of(prod: &str, sig: &str) -> String {
        let stripped = strip_line_comments(prod);
        let after_sig = stripped
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("config.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))].to_string()
    }

    /// 4.7.0 (issue #674): a locale that changes through *any* config write
    /// must reach the native surfaces. `after_persist` is that shared path, so
    /// an imported config (S5's `import_config`) or a Settings save carrying a
    /// stale draft cannot leave the tray and the app menu in the old language
    /// while `config.json` and the webview move on.
    ///
    /// Source-level by necessity: the relabel itself calls into two Tauri
    /// surfaces that need a live app handle, which no unit test can build. The
    /// behaviour behind it — which table gets installed, and whether a repaint
    /// is warranted — is covered by
    /// `i18n::tests::install_from_config_reports_only_real_locale_changes`.
    #[test]
    fn every_config_write_converges_the_native_locale() {
        let prod = prod_source(include_str!("config.rs"));

        let persisted = body_of(prod, "async fn after_persist(");
        assert!(
            persisted.contains("sync_native_locale("),
            "the shared post-write path must install the persisted locale"
        );

        let sync = body_of(prod, "fn sync_native_locale(");
        assert!(
            sync.contains("i18n::install_from_config("),
            "the sync helper must install the persisted locale"
        );
        assert!(
            sync.contains("menu::rebuild_app_menu("),
            "a changed locale must rebuild the native application menu, not just the installed table"
        );
        assert!(
            sync.contains("tray::refresh_tray_for_locale("),
            "a changed locale must repaint the tray, not just the installed table"
        );
    }

    /// Issue #811: `after_persist` must re-derive the OS entry through the
    /// OS-only half (`apply_os_autostart`), never through the persisting
    /// `set_autostart_enabled` command — calling the command from the
    /// post-write path would persist again and recurse.
    #[test]
    fn after_persist_syncs_autostart_without_reentering_the_command() {
        let prod = prod_source(include_str!("config.rs"));
        let persisted = body_of(prod, "async fn after_persist(");
        assert!(
            persisted.contains("apply_os_autostart("),
            "after_persist must sync the OS entry through the OS-only half (issue #811)"
        );
    }

    /// The `set_locale` command must converge through the same post-write path
    /// as every other config write instead of keeping its own copy of the
    /// relabel sequence.
    #[test]
    fn set_locale_routes_through_the_shared_post_write_path() {
        let prod = prod_source(include_str!("config.rs"));
        let body = body_of(prod, "pub async fn set_locale(");
        assert!(
            body.contains("after_persist(&app, &persisted).await"),
            "set_locale must run the shared post-write side effects"
        );
        assert!(
            !body.contains("rebuild_app_menu("),
            "set_locale must not keep a second relabel sequence of its own"
        );
    }

    /// Issue #823: an export destination belongs to the user, so the export
    /// must leave whatever already sits beside it alone — the sibling
    /// `<dest>.tmp` is exactly the path the config writer would have
    /// pre-cleared.
    #[test]
    fn export_leaves_a_tmp_sibling_and_a_sibling_directory_alone() {
        let dir = temp_dir("pj-test-export");
        let json = "{\"schema_version\":1}";

        // Sibling file with unrelated bytes: it must survive the export.
        let dest = dir.join("notes.json");
        let sibling = dir.join("notes.tmp");
        std::fs::write(&sibling, b"SENTINEL").unwrap();
        super::write_export_file(&dest, json).unwrap();
        assert_eq!(
            std::fs::read(&sibling).unwrap(),
            b"SENTINEL",
            "the export must not touch a `<dest>.tmp` sibling it did not create"
        );
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), json);

        // Sibling directory: neither an abort nor a removal.
        let dest2 = dir.join("journal.json");
        let sibling_dir = dir.join("journal.tmp");
        std::fs::create_dir(&sibling_dir).unwrap();
        super::write_export_file(&dest2, json).unwrap();
        assert!(
            sibling_dir.is_dir(),
            "a directory at the sibling path must not be removed"
        );
        assert_eq!(std::fs::read_to_string(&dest2).unwrap(), json);

        // And no staged sidecar is left behind.
        let strays: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".pj-export.tmp"))
            .collect();
        assert!(strays.is_empty(), "staged sidecars left behind: {strays:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The statement that carries `needle` when `needle` is an argument of a
    /// `spawn_blocking` call: from that call to the `;` ending the statement.
    /// `None` when the call is nowhere inside a `spawn_blocking` argument
    /// list — the failure the #885 guard exists to catch. Paren-counted, so a
    /// closure written either inline (`move || expr`, what rustfmt produces)
    /// or as a block passes.
    fn blocking_statement_of(body: &str, needle: &str) -> Option<String> {
        let pos = body.find(needle)?;
        let open = body[..pos].rfind("spawn_blocking(")?;
        let tail = &body[open..];
        let mut depth = 0i32;
        let mut end = None;
        for (i, ch) in tail.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                ';' if depth == 0 => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        Some(tail[..end?].to_string())
    }

    /// Issue #885: an open file dialog holds a runtime worker for as long as
    /// the user leaves it open, which on a low-core machine delays every other
    /// command that hops through the async runtime. Both pickers must
    /// therefore run on the blocking pool.
    ///
    /// Source-level by necessity: showing a native picker needs a real desktop
    /// session. The structural check — the call is an argument of the
    /// `spawn_blocking` call, and the join handle is awaited — is what makes
    /// this more than a proximity scan.
    #[test]
    fn file_pickers_run_on_the_blocking_pool() {
        let prod = prod_source(include_str!("config.rs"));

        for (sig, call) in [
            ("pub async fn export_config(", "blocking_save_file()"),
            ("pub async fn import_config(", "blocking_pick_file()"),
        ] {
            let body = body_of(prod, sig);
            let statement = blocking_statement_of(&body, call).unwrap_or_else(|| {
                panic!("{sig} calls {call} outside a spawn_blocking call (issue #885)")
            });
            assert!(
                statement.contains(".await"),
                "{sig} must await the picker task, or the command returns before the user answers"
            );
        }
    }

    /// A fresh temp directory for one test, unique per process and per call.
    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Issue #963: `AppConfig` is `#[serde(default)]` throughout, so any JSON
    /// object deserializes — including one carrying none of the application's
    /// sections, which would replace the user's settings with defaults while
    /// the import reported success.
    #[test]
    fn import_refuses_a_file_that_is_not_a_presencejam_config() {
        assert!(!is_presencejam_document(&serde_json::json!({})));
        assert!(!is_presencejam_document(&serde_json::json!({"foo": 1})));
        assert!(!is_presencejam_document(&serde_json::json!([1, 2, 3])));
        // One recognisable section is enough, whatever else the file carries.
        assert!(is_presencejam_document(&serde_json::json!({"spotify": {}})));
        assert!(is_presencejam_document(
            &serde_json::json!({"schema_version": 0})
        ));
    }

    /// The refusal runs on the command path and touches nothing: the live
    /// `config.json` stays byte-identical and no `.bak` appears (issue #963).
    #[test]
    fn refused_import_leaves_the_live_config_byte_identical() {
        let dir = temp_dir("pj-test-import-refuse");
        let live = dir.join("config.json");
        let previous = "{\n  \"spotify\": {\"client_id\": \"KEEP\"}\n}";
        std::fs::write(&live, previous).expect("live config");

        let picked = dir.join("picked.json");
        for document in ["{}", "{\"foo\":1}"] {
            std::fs::write(&picked, document).expect("picked file");
            let err = read_import_source(&picked)
                .expect_err("a file that is not a PresenceJam config must be refused");
            assert!(
                err.contains("not a PresenceJam configuration"),
                "the refusal must say what the file is not: {err}"
            );
            assert_eq!(
                std::fs::read_to_string(&live).expect("live config"),
                previous,
                "a refused import must leave the live config byte-identical"
            );
            assert!(
                !dir.join("config.json.bak").exists(),
                "a refused import must not quarantine the live config"
            );
        }

        // A genuine export is never refused — the export/import round trip.
        let exported = config::export_document(&config::AppConfig::default()).expect("export");
        std::fs::write(&picked, &exported).expect("picked export");
        assert_eq!(
            read_import_source(&picked).expect("export must import"),
            exported
        );

        // ...and `import_config` is what runs this check.
        let prod = prod_source(include_str!("config.rs"));
        let body = body_of(prod, "pub async fn import_config(");
        assert!(
            body.contains("read_import_source("),
            "import_config must refuse a non-PresenceJam file on its own path"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #939: a replace that cannot be written must leave the previous
    /// `config.json` in place. Staging first is what makes that true — the
    /// rename-then-write sequence this replaced moved the live file aside and
    /// only then failed, leaving the app with no config at all.
    #[test]
    fn failed_replace_leaves_the_previous_config_in_place() {
        let dir = temp_dir("pj-test-import-failed-replace");
        let live = dir.join("config.json");
        let previous = "{\"spotify\":{\"client_id\":\"KEEP\"}}";
        std::fs::write(&live, previous).expect("live config");
        let state = AppState::new();
        let mut previous_state = AppConfig::default();
        previous_state.spotify.client_id = "KEEP".to_string();
        *state.config.get_mut() = Some(previous_state);
        // An obstruction at the staged sidecar name fails the stage step —
        // which is the point: it happens before the live file is touched.
        let staged = staged_config_path(&live);
        std::fs::create_dir(&staged).expect("obstruction");

        let err = replace_and_adopt_config(
            &state,
            &live,
            "{\"spotify\":{\"client_id\":\"NEW\"}}",
            || -> Result<AppConfig, String> {
                panic!("a failed replace must not reload")
            },
        )
        .expect_err("the replace cannot be staged");
        assert!(err.contains("import temp file"), "unexpected error: {err}");
        assert_eq!(
            std::fs::read_to_string(&live).expect("live config"),
            previous,
            "a failed replace must leave the previous config.json in place"
        );
        assert!(
            !dir.join("config.json.bak").exists(),
            "the live config must not have been moved aside before the write"
        );
        assert!(staged.is_dir(), "the obstruction must not be removed");
        assert_eq!(
            state
                .config
                .get()
                .as_ref()
                .map(|cfg| cfg.spotify.client_id.as_str()),
            Some("KEEP"),
            "a failed import must not replace the previously adopted config"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #939 (unix): the real-world version of the same contract — a
    /// directory the app cannot write to yields an error while the previous
    /// config stays on disk, byte-identical.
    #[cfg(unix)]
    #[test]
    fn replace_in_an_unwritable_directory_keeps_the_previous_config() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("pj-test-import-unwritable");
        let inner = dir.join("cfg");
        std::fs::create_dir(&inner).expect("inner dir");
        let live = inner.join("config.json");
        let previous = "{\"spotify\":{\"client_id\":\"KEEP\"}}";
        std::fs::write(&live, previous).expect("live config");
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o555)).expect("chmod");

        // Root (or a filesystem that ignores the mode bits) would not be denied
        // here, and the contract can only be asserted where the write really
        // fails.
        let denied = std::fs::write(inner.join("probe"), b"x").is_err();
        if denied {
            let err = replace_config_file(&live, "{\"spotify\":{\"client_id\":\"NEW\"}}")
                .expect_err("an unwritable directory must fail the replace");
            assert!(err.contains("import temp file"), "unexpected error: {err}");
        }
        assert_eq!(
            std::fs::read_to_string(&live).expect("live config"),
            previous,
            "the previous config must survive a replace that could not be staged"
        );
        assert!(
            denied,
            "expected the 0o555 directory to deny this test's write"
        );

        let _ = std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A replace that can be written lands the new document exactly once and
    /// keeps the outgoing one as `config.json.bak`.
    #[test]
    fn replace_installs_the_document_once_and_quarantines_the_previous_one() {
        let dir = temp_dir("pj-test-import-replace");
        let live = dir.join("config.json");
        let previous = "{\"spotify\":{\"client_id\":\"OLD\"}}";
        let next = "{\n  \"spotify\": {\"client_id\": \"NEW\"}\n}";
        std::fs::write(&live, previous).expect("live config");

        replace_config_file(&live, next).expect("the replace must land");

        assert_eq!(std::fs::read_to_string(&live).expect("live config"), next);
        assert_eq!(
            std::fs::read_to_string(dir.join("config.json.bak")).expect("backup"),
            previous
        );
        assert!(
            !staged_config_path(&live).exists(),
            "the staged sidecar must not survive a successful replace"
        );

        // A fresh install has nothing to back up and is still replaced.
        let fresh = dir.join("fresh.json");
        replace_config_file(&fresh, next).expect("a replace into a missing file must succeed");
        assert_eq!(std::fs::read_to_string(&fresh).expect("fresh"), next);
        assert!(!staged_config_path(&fresh).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #946: a guarded writer that is ready while the import reloads
    /// must not acquire between the imported file replacement and AppState
    /// adoption. It may publish only after the import has returned the value
    /// that was on disk.
    #[test]
    fn import_adopts_the_reload_before_a_competing_writer_can_publish() {
        use std::sync::mpsc;
        use std::thread;

        struct WriterRelease(mpsc::Sender<()>);

        impl Drop for WriterRelease {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }

        let dir = temp_dir("pj-test-import-write-lock");
        let live = dir.join("config.json");
        let imported_document =
            "{\"spotify\":{\"client_id\":\"IMPORTED\"},\"schema_version\":1}";
        let prepared = config::prepare_import(imported_document).expect("valid import");
        std::fs::write(
            &live,
            "{\"spotify\":{\"client_id\":\"PREVIOUS\"},\"schema_version\":1}",
        )
        .expect("previous config");

        let state = Arc::new(AppState::new());
        let (published_on_disk, published_state) =
            thread::scope(|scope| {
                let (import_locked_tx, import_locked_rx) = mpsc::channel();
                let (writer_attempted_tx, writer_attempted_rx) = mpsc::channel();
                let (writer_adopted_tx, writer_adopted_rx) = mpsc::channel();
                let (release_writer_tx, release_writer_rx) = mpsc::channel();
                // This guard must live inside the scope: on an assertion panic it
                // releases the writer before scoped-thread joining begins.
                let release_writer = WriterRelease(release_writer_tx);

                let import_state = Arc::clone(&state);
                let import_live = live.clone();
                let import = scope.spawn(move || {
                    replace_and_adopt_config(
                        &import_state,
                        &import_live,
                        &prepared.document,
                        || {
                            // This callback runs only after the helper owns the
                            // config guard and has installed the imported file.
                            import_locked_tx
                                .send(())
                                .expect("test must observe the import lock");
                            writer_attempted_rx
                                .recv()
                                .expect("competing writer must attempt during the reload seam");
                            assert!(
                                import_state.config.try_get_mut().is_none(),
                                "the config write guard must still be held during import reload"
                            );
                            let raw =
                                std::fs::read_to_string(&import_live).expect("imported config");
                            serde_json::from_str(&raw).map_err(|error| error.to_string())
                        },
                    )
                });

                // Do not let scheduling decide who owns the config guard. The
                // reload callback cannot emit this until replace_and_adopt_config
                // has acquired it, so the writer is always launched under test.
                import_locked_rx
                    .recv()
                    .expect("import must acquire the config guard");
                let writer_state = Arc::clone(&state);
                let writer_live = live.clone();
                let writer = scope.spawn(move || {
                    let import_holds_guard = writer_state.config.try_get_mut().is_none();
                    writer_attempted_tx
                        .send(())
                        .expect("import reload must receive the competing writer attempt");
                    assert!(
                        import_holds_guard,
                        "competing writer must not acquire during import reload"
                    );
                    let mut guard = writer_state.config.get_mut();
                    writer_adopted_tx
                        .send(guard.as_ref().map(|cfg| cfg.spotify.client_id.clone()))
                        .expect("test must observe the writer's adopted predecessor");
                    release_writer_rx
                        .recv()
                        .expect("test must release the competing writer");

                    let mut competing = AppConfig::default();
                    competing.spotify.client_id = "COMPETING".to_string();
                    let document =
                        serde_json::to_string_pretty(&competing).expect("serialize competitor");
                    std::fs::write(&writer_live, document).expect("competing write");
                    *guard = Some(competing);
                });

                let imported = import
                    .join()
                    .expect("import task")
                    .expect("imported config must load and adopt");
                let writer_saw = writer_adopted_rx
                    .recv()
                    .expect("competing writer must acquire after import adoption");
                let on_disk_before_writer = serde_json::from_str::<AppConfig>(
                    &std::fs::read_to_string(&live).expect("config on disk"),
                )
                .expect("parse config on disk");

                assert_eq!(imported.spotify.client_id, "IMPORTED");
                assert_eq!(on_disk_before_writer.spotify.client_id, "IMPORTED");
                assert_eq!(
                    writer_saw.as_deref(),
                    Some("IMPORTED"),
                    "the writer must observe the imported state before replacing it"
                );
                drop(release_writer);
                writer.join().expect("competing writer task");

                let published_on_disk = serde_json::from_str::<AppConfig>(
                    &std::fs::read_to_string(&live).expect("published config on disk"),
                )
                .expect("parse published config on disk");
                let published_state = state
                    .config
                    .get()
                    .as_ref()
                    .map(|cfg| cfg.spotify.client_id.clone());
                (published_on_disk, published_state)
            });

        assert_eq!(published_on_disk.spotify.client_id, "COMPETING");
        assert_eq!(published_state.as_deref(), Some("COMPETING"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -------------------------------------------------------------------
    // Issue #876: Outlook `mailboxSettings.workingHours` → QuietHoursEntry.
    // The inverter is pure; the assertions below drive a recorded payload
    // through the parser (`parse_working_hours_body` is exercised in
    // `teams.rs`) and a `WorkingHours` fixture through the inverter, then
    // check the produced entries cover exactly the off-block set the user
    // would expect to see in the Settings preview.
    // -------------------------------------------------------------------

    /// Helper: builds a `QuietHoursEntry` for the assertions below so the
    /// field list lives in one place. Mirrors the production `make_off_entry`
    /// shape; assert-side helper only.
    fn off(day: u8, start_minutes: u16, end_minutes: u16) -> QuietHoursEntry {
        QuietHoursEntry {
            enabled: true,
            start_minutes,
            end_minutes,
            days: vec![day],
            replacement_status: String::new(),
            presence_availability: String::new(),
            presence_activity: String::new(),
            pause_polling: false,
        }
    }

    /// Issue #876: the canonical case from the issue text — Mon-Fri
    /// 09:00-17:00 working hours invert to exactly five wrap-around
    /// entries (one per working day) plus two full-day entries (Sat/Sun).
    /// 17:00 = 1020 minutes, 09:00 = 540 minutes.
    #[test]
    fn invert_working_hours_produces_expected_set_for_mon_fri_9_to_5() {
        use crate::teams::WorkingHours;
        let working = WorkingHours {
            start_minutes: 540,
            end_minutes: 1020,
            days: vec![1, 2, 3, 4, 5],
            time_zone_offset_minutes: -480,
        };
        let entries = invert_working_hours(&working);
        let expected = vec![
            off(1, 1020, 540), // Mon: 17:00 wrap-around (off 17:00-midnight + 00:00-09:00)
            off(2, 1020, 540), // Tue
            off(3, 1020, 540), // Wed
            off(4, 1020, 540), // Thu
            off(5, 1020, 540), // Fri
            off(6, 0, 1440),   // Sat full day
            off(7, 0, 1440),   // Sun full day
        ];
        assert_eq!(entries, expected);
    }

    /// Issue #876: when the user has cleared Outlook's Work hours tab
    /// (`daysOfWeek: []`), the inverter must produce no entries. Silent
    /// import with empty preview is the safe failure mode — overwriting
    /// the user's existing quiet_hours with seven empty days would
    /// disable suppression entirely.
    #[test]
    fn invert_working_hours_with_no_working_days_emits_nothing() {
        use crate::teams::WorkingHours;
        let working = WorkingHours {
            start_minutes: 540,
            end_minutes: 1020,
            days: Vec::new(),
            time_zone_offset_minutes: 0,
        };
        assert!(invert_working_hours(&working).is_empty());
    }

    /// Issue #876: overnight working hours (e.g. night-shift 22:00-06:00)
    /// produce a non-wrap entry on each working day — off = [06:00, 22:00]
    /// in the middle of the day.
    #[test]
    fn invert_working_hours_handles_overnight_window() {
        use crate::teams::WorkingHours;
        let working = WorkingHours {
            start_minutes: 22 * 60, // 22:00
            end_minutes: 6 * 60,    // 06:00 next day
            days: vec![1, 2, 3, 4, 5],
            time_zone_offset_minutes: 0,
        };
        let entries = invert_working_hours(&working);
        // Mon-Fri each get a single non-wrap entry off = [06:00, 22:00].
        assert_eq!(entries.len(), 7);
        for (i, entry) in entries.iter().enumerate().take(5) {
            assert_eq!(entry.start_minutes, 6 * 60, "entry {} start", i);
            assert_eq!(entry.end_minutes, 22 * 60, "entry {} end", i);
            assert_eq!(entry.days, vec![(i as u8) + 1]);
        }
        // Sat/Sun still full off.
        assert_eq!(entries[5], off(6, 0, 1440));
        assert_eq!(entries[6], off(7, 0, 1440));
    }

    /// Issue #876: a 24-hour working window collapses to no entries (no
    /// off-hours exist). Catches the off-by-one in the `work_end ==
    /// work_start` early-return below.
    #[test]
    fn invert_working_hours_24h_window_emits_nothing() {
        use crate::teams::WorkingHours;
        let working = WorkingHours {
            start_minutes: 0,
            end_minutes: 1440,
            days: vec![1, 2, 3, 4, 5, 6, 7],
            time_zone_offset_minutes: 0,
        };
        assert!(invert_working_hours(&working).is_empty());
    }

    /// Issue #876: a partial-week working schedule (alternating days)
    /// produces a mix of wrap and full-day entries that exactly covers
    /// the off-blocks the user expects.
    #[test]
    fn invert_working_hours_partial_week_alternates_wrap_and_full_day() {
        use crate::teams::WorkingHours;
        let working = WorkingHours {
            start_minutes: 540,
            end_minutes: 1020,
            days: vec![1, 3, 5], // Mon, Wed, Fri
            time_zone_offset_minutes: 0,
        };
        let entries = invert_working_hours(&working);
        assert_eq!(entries.len(), 7);
        let expected = vec![
            off(1, 1020, 540), // Mon: wrap
            off(2, 0, 1440),   // Tue: full off
            off(3, 1020, 540), // Wed: wrap
            off(4, 0, 1440),   // Thu: full off
            off(5, 1020, 540), // Fri: wrap
            off(6, 0, 1440),   // Sat: full off
            off(7, 0, 1440),   // Sun: full off
        ];
        assert_eq!(entries, expected);
    }
}
