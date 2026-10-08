use super::clamp::{
    clamp_locale, clamp_logging, clamp_polling, clamp_presence_profiles, clamp_rules,
    clamp_shortcuts, clamp_teams,
};
use super::migrate::{
    default_schema_version, migrate_config, stamp_schema_version, SCHEMA_VERSION,
};
use super::schema::{AppConfig, ClientSecretState, LoggingConfig};
use super::snooze::{clamp_snooze, snooze_expired_deadline};
use super::transfer::strip_client_secret_from_extras;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::Emitter;
pub fn config_dir() -> Result<PathBuf, String> {
    // Maintained replacement for the unmaintained `dirs` crate (issue #418):
    // `directories::BaseDirs::new()` resolves the same platform config
    // roots (XDG_CONFIG_HOME/~/.config on Linux, ~/Library/Application
    // Support on macOS, %APPDATA% on Windows) and preserves the
    // `<config>/PresenceJam` layout and 0o700 creation below.
    let base_dir = directories::BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .ok_or_else(|| {
            "Failed to get config directory: BaseDirs::new() returned None".to_string()
        })?;

    let app_dir = base_dir.join("PresenceJam");

    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| {
            format!(
                "Failed to create config directory '{}': {}",
                app_dir.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o700));
        }
        log::info!("[CFG] Created config directory at '{}'", app_dir.display());
    }

    Ok(app_dir)
}

pub fn get_config_path() -> Result<PathBuf, String> {
    let dir = config_dir()?;
    Ok(dir.join("config.json"))
}

/// Set when `load_config` finds a corrupt config.json and quarantines it to
/// `<config>.bak` (issue #379). Diagnostics-visible via
/// [`config_was_quarantined`]; warn-log-only otherwise — no other channel is
/// touched by this slice. Owned by `AppCaches` (issue #758 slice 2) so each
/// test constructs isolated flags instead of serialising on a test lock.
pub fn config_was_quarantined(caches: &crate::state::AppCaches) -> bool {
    caches.quarantined_flag().load(Ordering::SeqCst)
}

/// Backup path alongside the original: `config.json` → `config.json.bak`.
///
/// Shared with the config-import command (4.7.0, S5), which moves the
/// outgoing file here before an imported document replaces it.
pub(crate) fn quarantine_backup_path(path: &std::path::Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}

/// The sidecar an imported document is staged in before it replaces the live
/// config: `<config.json>.import.tmp`.
///
/// Deliberately distinct from `atomic_write_json`'s `<config.json>.tmp`, so an
/// import in flight and an ordinary save can never consume or clear each
/// other's staged bytes. Beside the live file, so the final rename stays on
/// one volume and is atomic (issue #939).
pub(crate) fn staged_import_path(path: &std::path::Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".import.tmp");
    PathBuf::from(staged)
}

/// Replace the config at `path` with `json`, keeping the outgoing document as
/// `<path>.bak` (issue #939).
///
/// Order is the whole point. The incoming bytes are written and fsynced to a
/// same-directory sidecar FIRST, so a replacement that cannot be written — disk
/// full, quota, permission denied, an antivirus lock — fails with the live
/// `config.json` still in place. Only then is the live file moved aside and
/// the staged copy renamed over it; both of those are renames, so the window in
/// which no live config exists is a single syscall wide rather than spanning a
/// write.
///
/// If that final rename fails, the backup is moved back before the error
/// returns, so the user is left with the document they had rather than with
/// only a `.bak` that nothing in the app restores. Every failure path removes
/// the staged sidecar, and a stale one from a crashed import is pre-cleared
/// exactly as `atomic_write_json` does for `config.json.tmp` (#135 path A).
///
/// The command layer's `import_config` runs this inside the config write
/// guard (issue #946), so no competing writer can slip between the file
/// replacement and the reload that publishes it.
pub(crate) fn replace_with_backup(path: &std::path::Path, json: &str) -> Result<(), String> {
    let staged = staged_import_path(path);

    if let Err(e) = fs::remove_file(&staged) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale import temp file '{}': {}",
                staged.display(),
                e
            ));
        }
    }

    // 0600 at creation, never chmod-after-create, for the same reason
    // `atomic_write_json` does it: no window in which the config is
    // world-readable.
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staged)
        .map_err(|e| {
            format!(
                "Failed to create import temp file '{}': {}",
                staged.display(),
                e
            )
        })?;

    #[cfg(not(unix))]
    let mut file = fs::File::create(&staged).map_err(|e| {
        format!(
            "Failed to create import temp file '{}': {}",
            staged.display(),
            e
        )
    })?;

    if let Err(e) = file.write_all(json.as_bytes()) {
        let _ = fs::remove_file(&staged);
        return Err(format!(
            "Failed to write import temp file '{}': {}",
            staged.display(),
            e
        ));
    }
    if let Err(e) = file.sync_all() {
        let _ = fs::remove_file(&staged);
        return Err(format!(
            "Failed to sync import temp file '{}': {}",
            staged.display(),
            e
        ));
    }
    drop(file);

    // The incoming document is now fully durable on disk, so moving the live
    // file aside can no longer lose the user's settings.
    let backup = quarantine_backup_path(path);
    let had_live = path.exists();
    if had_live {
        if let Err(e) = fs::rename(path, &backup) {
            let _ = fs::remove_file(&staged);
            return Err(format!(
                "Failed to move the current config to '{}': {}",
                backup.display(),
                e
            ));
        }
        log::info!(
            "[CFG] import: previous config moved to '{}'",
            backup.display()
        );
    }

    if let Err(e) = fs::rename(&staged, path) {
        log::error!(
            "[CFG] import: FAILED to install the imported config at '{}': {}",
            path.display(),
            e
        );
        let _ = fs::remove_file(&staged);
        if had_live {
            match fs::rename(&backup, path) {
                Ok(()) => log::warn!("[CFG] import: the previous config was moved back into place"),
                Err(rollback) => log::error!(
                    "[CFG] import: rollback FAILED - the previous config is at '{}': {}",
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

    // Same parent-directory fsync `atomic_write_json` performs: the renames
    // above are only durable once the directory entry is flushed too.
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            if let Err(e) = dir.sync_all() {
                log::warn!(
                    "[CFG] Failed to fsync config dir '{}': {}",
                    parent.display(),
                    e
                );
            }
        }
    }
    Ok(())
}

/// Bare file name of the quarantine backup for `path` when one exists,
/// else `None`. Deliberately a bare name and never an absolute path, so the
/// diagnostics snapshot can surface it without breaching the #409
/// no-absolute-path rule (CfgDiag#2, issue #537).
pub(crate) fn quarantine_backup_name_for(path: &std::path::Path) -> Option<String> {
    let backup = quarantine_backup_path(path);
    if backup.is_file() {
        backup
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// [`quarantine_backup_name_for`] against this process's real config path.
/// `None` when nothing was quarantined or the `.bak` has since been removed.
pub fn config_quarantine_backup_name() -> Option<String> {
    quarantine_backup_name_for(&get_config_path().ok()?)
}

/// Rename a corrupt config file alongside itself (`<name>.bak`), raise the
/// diagnostics-visible quarantine flag, and warn. Never fails the load:
/// rename errors are logged and swallowed so the caller falls back to
/// defaults either way (issue #379).
pub(crate) fn quarantine_corrupt_config(
    caches: &crate::state::AppCaches,
    path: &std::path::Path,
    parse_err: impl std::fmt::Display,
) -> PathBuf {
    let backup = quarantine_backup_path(path);
    match fs::rename(path, &backup) {
        Ok(()) => log::warn!(
            "[CFG] corrupt config '{}' quarantined to '{}': {} — loading defaults",
            path.display(),
            backup.display(),
            parse_err
        ),
        Err(rename_err) => log::warn!(
            "[CFG] corrupt config '{}' failed to parse ({}) and quarantine rename to '{}' failed ({}); loading defaults",
            path.display(),
            parse_err,
            backup.display(),
            rename_err
        ),
    }
    caches.quarantined_flag().store(true, Ordering::SeqCst);
    backup
}

/// Every top-level key [`AppConfig`] has a typed field for (issue #926).
///
/// [`config_from_sections`] reads these one at a time and everything else lands
/// in [`AppConfig::extra`] — the same partition `#[serde(flatten)]` performs
/// when serde parses the document in one call, written out so that one bad
/// section can be replaced by its default without rejecting the rest.
/// `typed_config_keys_match_the_serialized_schema` fails if this list and the
/// struct ever disagree.
pub(crate) const TYPED_CONFIG_KEYS: [&str; 16] = [
    "spotify",
    "teams",
    "polling",
    "logging",
    "updates",
    "playback",
    "autostart",
    "notifications",
    "locale",
    "snooze_until",
    "status_rules",
    "presence_profiles",
    "active_profile",
    "shortcuts",
    "schema_version",
    "revision",
];

/// The JSON type of `value`, for a log line that names the shape of a bad root
/// without echoing a whole (possibly multi-megabyte) document.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Deserialize ONE typed field out of a config document, falling back to
/// `fallback` when that field alone does not match the schema (issue #926).
///
/// Per-field, not per-document. A key the file omits takes `fallback`
/// silently — what `#[serde(default)]` has always done — while a key that is
/// PRESENT but invalid takes it with a `[CFG]` warning naming the key, which is
/// the observable replacement for the old all-or-nothing parse. A `null` value
/// is "present but invalid" for every non-`Option` field and a value for an
/// `Option` one, so the field's own type decides, not a special case here.
fn field_or_fallback<T: serde::de::DeserializeOwned>(
    root: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: T,
) -> T {
    let Some(raw) = root.get(key) else {
        return fallback;
    };
    match serde_json::from_value::<T>(raw.clone()) {
        Ok(value) => value,
        Err(e) => {
            log::warn!(
                "[CFG] config field '{}' is invalid ({}) — using its default, the rest of the config is kept",
                key,
                e
            );
            fallback
        }
    }
}

/// Build an [`AppConfig`] from a config document's root object, one typed field
/// at a time (issue #926).
///
/// A section that no longer matches the schema costs exactly that section — its
/// default, warned about by [`field_or_fallback`] — instead of the whole
/// document, which is what one wrong-typed, out-of-range or misspelled value
/// used to cost (the file was quarantined and the app booted on defaults). The
/// document's scalar fields take the same route, so `"autostart": "yes"` cannot
/// take quiet hours down with it either.
///
/// Unknown top-level keys are still retained in [`AppConfig::extra`] (issue
/// #379), exactly as the `#[serde(flatten)]` field collected them under the
/// single-pass parse.
pub(crate) fn config_from_sections(root: serde_json::Map<String, serde_json::Value>) -> AppConfig {
    let mut config = AppConfig {
        spotify: field_or_fallback(&root, "spotify", Default::default()),
        teams: field_or_fallback(&root, "teams", Default::default()),
        polling: field_or_fallback(&root, "polling", Default::default()),
        logging: field_or_fallback(&root, "logging", Default::default()),
        updates: field_or_fallback(&root, "updates", Default::default()),
        playback: field_or_fallback(&root, "playback", Default::default()),
        autostart: field_or_fallback(&root, "autostart", Default::default()),
        notifications: field_or_fallback(&root, "notifications", Default::default()),
        locale: field_or_fallback(&root, "locale", Default::default()),
        snooze_until: field_or_fallback(&root, "snooze_until", Default::default()),
        status_rules: field_or_fallback(&root, "status_rules", Default::default()),
        presence_profiles: field_or_fallback(&root, "presence_profiles", Default::default()),
        active_profile: field_or_fallback(&root, "active_profile", Default::default()),
        shortcuts: field_or_fallback(&root, "shortcuts", Default::default()),
        schema_version: field_or_fallback(&root, "schema_version", default_schema_version()),
        revision: field_or_fallback(&root, "revision", 0),
        extra: BTreeMap::new(),
    };
    for (key, value) in root {
        if !TYPED_CONFIG_KEYS.contains(&key.as_str()) {
            config.extra.insert(key, value);
        }
    }
    config
}

/// Tighten a loose `config.json` to 0600 (issue #135 path A), best-effort
/// since issue #802.
///
/// Idempotent on a file that is already 0600. Unix-only: Windows' default ACL
/// is already user-only, so there is nothing to tighten there.
///
/// Best-effort, NOT a precondition: a mode that cannot be READ (EROFS on a
/// read-only or ostree mount, EPERM on a file owned by another user, an
/// ACL-managed path) or cannot be CHANGED is logged and ignored. Both used to
/// be `?`-propagated, so `load_config` failed outright on a perfectly readable
/// file — startup logged "no config found", `AppState.config` stayed `None`,
/// the Settings and Dashboard stores fell back to built-in defaults, and
/// because a Settings save posts the whole document, the next save persisted
/// those defaults over the user's real file. Hardening a file we can already
/// read is a courtesy; refusing to read it is a data-loss path.
#[cfg(unix)]
pub(crate) fn tighten_config_permissions(path: &std::path::Path) {
    let current = match fs::metadata(path) {
        Ok(metadata) => metadata.permissions(),
        Err(e) => {
            log::warn!(
                "[CFG] Could not read the mode of config file '{}': {} — loading it anyway",
                path.display(),
                e
            );
            return;
        }
    };
    let current_mode = current.mode() & 0o777;
    if current_mode == 0o600 {
        return;
    }
    log::warn!(
        "[CFG] Tightening config.json mode from {:o} to 0600 (issue #135)",
        current_mode
    );
    let mut tightened = current;
    tightened.set_mode(0o600);
    if let Err(e) = fs::set_permissions(path, tightened) {
        log::warn!(
            "[CFG] Could not chmod config file '{}' to 0600: {} — loading it anyway",
            path.display(),
            e
        );
    }
}

pub fn load_config(caches: &crate::state::AppCaches) -> Result<AppConfig, String> {
    load_config_from(caches, &get_config_path()?).map(|config| {
        with_keychain_flags(config, || {
            crate::keychain::cached_spotify_client_secret_presence()
        })
    })
}

/// Path-taking core of [`load_config`]: the file I/O, the section-by-section
/// parse and the normalization, with the keychain stamping left to the public
/// entry point — so this half is testable against real files with no keychain
/// probe, the same shape [`import_config_document`] uses.
pub(crate) fn load_config_from(
    caches: &crate::state::AppCaches,
    path: &std::path::Path,
) -> Result<AppConfig, String> {
    if !path.exists() {
        log::info!(
            "[CFG] Config file not found at '{}', using defaults",
            path.display()
        );
        return Ok(AppConfig::default());
    }

    // Issue #135 path A: tighten the mode of any pre-existing config.json that
    // was created loose by an older PresenceJam version (default umask 022 →
    // 0644). Best-effort since issue #802 — see `tighten_config_permissions`.
    #[cfg(unix)]
    tighten_config_permissions(path);

    let mut file = fs::File::open(path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;

    let mut config = match serde_json::from_str::<serde_json::Value>(&contents) {
        // Issue #926: the document IS an object — load it field by field, so a
        // section that no longer matches the schema costs exactly that
        // section's default and nothing else.
        Ok(serde_json::Value::Object(root)) => config_from_sections(root),
        // Anything else is not a config: a bare array/string/number/null
        // root, or text that is not JSON at all. Quarantine, exactly as the
        // single-pass parse answered those two shapes before.
        Ok(other) => {
            quarantine_corrupt_config(
                caches,
                path,
                format!("expected a JSON object, found {}", json_kind(&other)),
            );
            return Ok(AppConfig::default());
        }
        Err(e) => {
            // Issue #379: never lose the evidence — quarantine the corrupt
            // file to `<config>.bak` alongside the original and boot on
            // defaults. Observable via `config_was_quarantined()`.
            quarantine_corrupt_config(caches, path, &e);
            return Ok(AppConfig::default());
        }
    };
    // Issue #916: an unknown-key bucket is `#[serde(flatten)]` with no
    // entry-level filter, so a `client_secret` a hand-edit or another tool left
    // at any level was deserialized, handed to the webview by the `load_config`
    // command and re-serialized on the next save — a credential crossing the
    // IPC boundary in plaintext, in a file SECURITY.md promises is
    // keychain-only. The legacy migration owns the DISK copy (it moves the
    // value into the keychain, or deliberately leaves it on a conflict); this
    // keeps the value out of the document the webview receives. Every load path
    // funnels through here — the startup load and the `load_config` command
    // alike — so there is no second place to remember.
    let stripped_secrets = strip_client_secret_from_extras(&mut config);
    if stripped_secrets > 0 {
        log::warn!(
            "[CFG] config: stripped {} client_secret key(s) from unknown keys — the Spotify client secret is keychain-only (issue #9)",
            stripped_secrets
        );
    }
    // CfgDiag#1 (#536): the version dispatcher runs BEFORE the clamps, so a
    // migration can never have its rewritten values re-clamped away, and
    // `schema_version` is raised even for a file that was never saved by
    // this binary.
    let from_version = config.schema_version;
    migrate_config(&mut config, from_version);
    clamp_polling(&mut config.polling);
    clamp_teams(&mut config.teams);
    clamp_rules(&mut config.status_rules);
    clamp_logging(&mut config.logging);
    // Issue #869: enforce name uniqueness + ≤ 32 chars + active-profile
    // validity on every load, mirroring the other `clamp_*` calls. The
    // active-profile pointer is cleared if the named profile has been
    // removed (a hand-edited config or an upgrade that dropped profiles
    // cannot silently land on a phantom id).
    clamp_presence_profiles(&mut config.presence_profiles, &mut config.active_profile);
    // Issue #767: same two clamps as `clamped_config`, so the document a load
    // hands the webview already carries the canonical tag and the normalised
    // bindings. The reader does not persist anything here — the values reach
    // disk on the next guarded write, exactly like every other clamp.
    clamp_locale(&mut config);
    clamp_shortcuts(&mut config.shortcuts);
    // 4.7.0 (S9, issue #677): an expired snooze is reported here and REMOVED by
    // the guarded writers below, never by this reader.
    //
    // The distinction is load-bearing twice over. (a) `load_config` runs on
    // paths that hold no config-write guard — the startup load, the
    // `load_config` command, `update_config`'s cold read — so writing the file
    // from here could clobber a concurrent save and break the #297 invariant
    // that the file and the in-memory copy agree. (b) An expired value must stay
    // VISIBLE to the consumer that can persist its removal: `poll_once`'s
    // `SnoozeGate::Expired` arm and the tray's startup cleaner both read the
    // stored field. Clearing it here made the disk drift permanent — the
    // in-memory copy looked clean, so nothing ever rewrote `config.json`, and
    // the line below repeated on every launch.
    if snooze_expired_deadline(&config, chrono::Utc::now()) {
        log::info!("[CFG] snooze: the stored deadline had already passed — it is ignored and cleared on the next write");
    }

    log::info!("[CFG] Loaded configuration from '{}'", path.display());
    Ok(config)
}

/// The persisted `logging` section, read without a full config load
/// (4.7.0, S5).
///
/// The log plugin is registered on the Tauri builder *before* the `setup`
/// hook runs, so the rotating file target needs its size and retention
/// settings before [`load_config`] is reached. Deliberately narrow:
/// no migration, no quarantine side effects, and **no keychain probe** —
/// `load_config` runs moments later and a second probe at startup is a real
/// macOS prompt risk (see [`with_keychain_flags`]).
///
/// A missing, unreadable or unparsable file yields the defaults; the real
/// load still handles the corrupt-file case.
pub fn logging_config_for_startup() -> LoggingConfig {
    let mut logging = match get_config_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
    {
        Some(contents) => match serde_json::from_str::<AppConfig>(&contents) {
            Ok(cfg) => cfg.logging,
            Err(e) => {
                log::warn!(
                    "[CFG] startup log-rotation read: config unparsable ({}); using log defaults",
                    e
                );
                LoggingConfig::default()
            }
        },
        None => LoggingConfig::default(),
    };
    clamp_logging(&mut logging);
    logging
}

/// Populate derived/display fields that are not persisted to disk.
///
/// Covers the two Spotify keychain views: `client_secret_set` (the pre-#560
/// `Present`-only flag) and `client_secret_state` (the tri-state that can say
/// "the keychain could not answer"). See issues #9 and #560.
///
/// One presence result feeds both. A fresh warm keychain observation avoids
/// an OS round trip; a cold or expired cache falls back to the direct
/// tri-state probe, which still notices credentials changed through the OS UI.
pub(crate) fn with_keychain_flags(
    config: AppConfig,
    presence: impl FnOnce() -> crate::keychain::KeychainPresence,
) -> AppConfig {
    stamp_keychain_flags(config, presence())
}

pub(crate) fn stamp_keychain_flags(
    mut config: AppConfig,
    presence: crate::keychain::KeychainPresence,
) -> AppConfig {
    config.spotify.client_secret_set =
        matches!(presence, crate::keychain::KeychainPresence::Present);
    config.spotify.client_secret_state = ClientSecretState::from(&presence);
    config
}
pub(crate) fn atomic_write_json(path: &std::path::Path, json: &str) -> Result<(), String> {
    let temp_path = path.with_extension("tmp");

    // Issue #135 path A: create the temp file with mode 0600 atomically.
    // Pre-clear any stale sidecar from a previous crash (between temp-write
    // and rename). Without this pre-clear, create_new(true) would error with
    // AlreadyExists on a leftover `.tmp`, turning a one-off crash into a
    // permanent save failure until the user manually deletes the sidecar.
    // Deletion of a non-existent file is fine — we ignore NotFound.
    if let Err(e) = fs::remove_file(&temp_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }
    }
    // OpenOptions::create_new(true) prevents racing with a leftover sidecar;
    // .mode(0o600) sets the mode at file-creation time (no chmod-after-create
    // window where config.json could briefly sit world-readable). The
    // subsequent rename() preserves the source mode on POSIX. On Windows,
    // the new file inherits the user-only default ACL of the parent.
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| {
            format!(
                "Failed to create temp file '{}': {}",
                temp_path.display(),
                e
            )
        })?;

    #[cfg(not(unix))]
    let mut file = fs::File::create(&temp_path).map_err(|e| {
        format!(
            "Failed to create temp file '{}': {}",
            temp_path.display(),
            e
        )
    })?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write temp file '{}': {}", temp_path.display(), e))?;

    file.sync_all()
        .map_err(|e| format!("Failed to sync temp file '{}': {}", temp_path.display(), e))?;

    std::fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file to '{}': {}", path.display(), e))?;
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                if let Err(e) = dir.sync_all() {
                    log::warn!(
                        "[CFG] Failed to fsync config dir '{}': {}",
                        parent.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(())
}
/// The config as it will actually be persisted: `clamp_polling` applied.
///
/// `save_config` writes a clamped copy, so the caller must store THIS value
/// in `AppState` rather than its own unclamped input — otherwise a value the
/// UI can type (the number inputs' `min`/`max` attributes do not constrain a
/// typed value) lives in memory while a different one sits on disk, and the
/// two silently reconcile only on the next launch. See issue #297.
pub fn clamped_config(config: &AppConfig) -> AppConfig {
    let mut cfg = config.clone();
    clamp_polling(&mut cfg.polling);
    clamp_teams(&mut cfg.teams);
    clamp_rules(&mut cfg.status_rules);
    clamp_logging(&mut cfg.logging);
    // Issue #869: clamp the profile list + active id on every save
    // (mirrors `clamp_rules` and `clamp_teams`). The active-profile
    // pointer is cleared if its name no longer matches — a Settings
    // delete that removes the active profile must not leave a phantom
    // pointer behind.
    clamp_presence_profiles(&mut cfg.presence_profiles, &mut cfg.active_profile);
    // 4.7.0 (S9, issue #677): a write that carries an already-expired deadline
    // (a whole-document save from a stale draft, or a resume click that raced
    // its own deadline) normalizes it away, so the in-memory copy, the file on
    // disk and the tray can never disagree about a snooze being active.
    clamp_snooze(&mut cfg, chrono::Utc::now());
    // Issue #767: the two clamps for the patch fields that did not exist when
    // this function was written. Without them a `update_config` patch could
    // persist a locale tag the app cannot render, or a blank shortcut binding
    // that renders as set-up while registering nothing.
    clamp_locale(&mut cfg);
    clamp_shortcuts(&mut cfg.shortcuts);
    // Issue #916: the write path strips too, so a payload that carries a
    // `client_secret` in an unknown-key bucket cannot put a credential back
    // into `config.json` (or into an export) on the way out.
    strip_client_secret_from_extras(&mut cfg);
    cfg
}

/// The persisted document's markers, read once (issues #938 and #943): the
/// stored `schema_version` and the stored `revision`.
///
/// A missing, unreadable, unparsable or non-object file yields `(None, 0)`:
/// there is no version to protect and no revision to be behind, and a corrupt
/// file is about to be replaced by the save this is guarding anyway. A stored
/// `schema_version` that is not a `u32` is `None` for the same reason.
fn stored_document_markers(path: &std::path::Path) -> (Option<u32>, u64) {
    let Ok(contents) = fs::read_to_string(path) else {
        return (None, 0);
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return (None, 0);
    };
    (
        root.get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok()),
        root.get("revision")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    )
}

/// Marker the stale-revision error starts with (issue #943), so the webview's
/// config store can tell "the settings changed in another window" apart from
/// any other save failure and re-load the document instead of retrying the same
/// payload.
pub const STALE_REVISION_MARKER: &str = "stale-config-revision";

/// Emitted after every accepted save (issue #943): `{"revision": u64,
/// "config": <persisted document>}`.
///
/// The config-writing commands call [`emit_config_changed`] once a persist has
/// succeeded, so a second Settings webview adopts the stored state instead of
/// writing its own stale copy over it — the same shape the presence and tray
/// mirrors use. A tray snooze released this way reaches the Dashboard with no
/// remount.
pub const CONFIG_CHANGED_EVENT: &str = "config-changed";

/// Emit [`CONFIG_CHANGED_EVENT`] for the document that was just persisted,
/// returning its revision (issue #943).
///
/// `persisted` is what [`save_config_persisted`] returned — the clamped,
/// revision-stamped document that is on disk — so what the other window renders
/// matches the file. Emission is best-effort, like every other app-level emit in
/// this codebase: a window that is not listening loses nothing, it reads the
/// same document on its next load.
pub fn emit_config_changed(app: &tauri::AppHandle, persisted: &AppConfig) -> u64 {
    match serde_json::to_value(persisted) {
        Ok(document) => {
            let _ = app.emit(
                CONFIG_CHANGED_EVENT,
                crate::events::ConfigChanged {
                    revision: persisted.revision,
                    config: document,
                },
            );
        }
        Err(e) => log::warn!(
            "[CFG] config-changed: the persisted document could not be serialized ({}); not emitting",
            e
        ),
    }
    persisted.revision
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    save_config_persisted(config).map(|_| ())
}

/// Persist `config` and return the document that was written (issue #943).
///
/// [`save_config`] is this function with the returned document dropped, for
/// callers that do not keep the config in memory. A caller that DOES — the
/// commands layer stores the persisted value in `AppState`, see #297 — should
/// use this one: the written document carries the next `revision`, and a caller
/// still holding the pre-save copy would be rejected as stale by its own next
/// save once it starts sending that revision.
pub fn save_config_persisted(config: &AppConfig) -> Result<AppConfig, String> {
    save_config_to(&get_config_path()?, config)
}

/// Path-taking core of [`save_config_persisted`]: the normalization, the
/// stale-revision rejection, the newer-document refusal and the atomic write —
/// so the write path is testable against real files, the same shape
/// [`import_config_document`] uses.
pub(crate) fn save_config_to(
    path: &std::path::Path,
    config: &AppConfig,
) -> Result<AppConfig, String> {
    let (stored_version, stored_revision_of_file) = stored_document_markers(path);

    // Issue #943: a payload behind the document on disk is a second webview
    // writing its own stale copy — the write that silently reverted the other
    // window's change. Reject it instead: the caller re-loads, the user is told
    // which window moved, and the newer document survives.
    //
    // `revision == 0` means the payload carries no revision at all (a frontend
    // that does not send the field yet, or a fresh install), so it is stamped
    // upward rather than rejected: rejecting it would make every save from such
    // a client fail as soon as the first one succeeded, which is a worse failure
    // than the one being fixed. The guard applies the moment a client sends the
    // revision it loaded.
    if config.revision != 0 && config.revision < stored_revision_of_file {
        log::warn!(
            "[CFG] refusing a stale config write to '{}': the stored document is at revision {} and this copy is at {}",
            path.display(),
            stored_revision_of_file,
            config.revision
        );
        return Err(format!(
            "{STALE_REVISION_MARKER}: the settings were changed in another window (stored revision {stored_revision_of_file}, this copy is at revision {})",
            config.revision
        ));
    }

    // Issue #938: never rewrite a document a NEWER binary wrote. The marker is
    // the only record of which migrations have run, and this build sees none of
    // that document's unknown keys: writing back would relabel it at this
    // build's version — so the newer build's dispatcher skips its own
    // migrations on the next launch — and drop the keys those migrations read.
    // Leaving the file alone loses nothing, and the caller surfaces the error.
    if let Some(stored) = stored_version {
        if stored > SCHEMA_VERSION {
            log::warn!(
                "[CFG] refusing to overwrite config '{}': schema_version {} was written by a newer PresenceJam (this build writes {})",
                path.display(),
                stored,
                SCHEMA_VERSION
            );
            return Err(format!(
                "The stored configuration was written by a newer version of PresenceJam (schema {stored}); leaving it untouched"
            ));
        }
    }

    let mut cfg = clamped_config(config);
    // CfgDiag#1 (#536): the client's `schema_version` is a suggestion, not an
    // instruction — a stale payload can never lower the version, and since
    // issue #938 it cannot raise one above a newer document's either.
    stamp_schema_version(&mut cfg);
    // Issue #943: strictly increasing, and never below either side's value, so
    // two windows saving in sequence hand each other a rising token.
    cfg.revision = stored_revision_of_file
        .max(config.revision)
        .saturating_add(1);

    let json = serde_json::to_string_pretty(&cfg)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;

    atomic_write_json(path, &json)?;

    log::info!(
        "[CFG] Saved configuration to '{}' (revision {})",
        path.display(),
        cfg.revision
    );
    Ok(cfg)
}
