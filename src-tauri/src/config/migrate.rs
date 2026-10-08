use super::io::{atomic_write_json, get_config_path};
use super::schema::AppConfig;
use super::transfer::{legacy_client_secret, strip_client_secret_keys};
use std::fs;
use std::sync::atomic::Ordering;
use tauri::Emitter;
/// The config schema version THIS binary writes (CfgDiag#1, issue #536).
/// Bump whenever the persisted shape gains or changes a field that needs a
/// migration.
///
/// Deliberately separate from [`default_schema_version`]: a file with no
/// `schema_version` key predates 4.3.0 and is therefore a *v1* file, so the
/// dispatcher must still run for it. Issue #869: bumped from 2 to 3 for the
/// `presence_profiles` / `active_profile` additions — both are
/// `#[serde(default)]`, so pre-5.0 documents still load as `presence_profiles
/// = vec![]` / `active_profile = None` without a migration step, but the
/// version marker is bumped so a future dispatcher can tell which binary
/// authored a given file.
pub const SCHEMA_VERSION: u32 = 3;

pub(crate) fn default_schema_version() -> u32 {
    1
}

/// Make the binary — never the client — authoritative for `schema_version`
/// (CfgDiag#1, issue #536; issue #938 for the newer-document half).
///
/// A stale frontend payload (or a wizard literal that still sends `1`) can no
/// longer erase the record that a migration already ran: the marker never goes
/// below [`SCHEMA_VERSION`]. It never goes DOWN at all — a document written by a
/// NEWER binary keeps its own version, because this build cannot know which of
/// that version's migrations have already run, and relabelling it would make the
/// newer build's dispatcher skip them on its next launch. [`save_config`]
/// refuses such a document outright rather than writing over it.
pub fn stamp_schema_version(cfg: &mut AppConfig) {
    cfg.schema_version = cfg.schema_version.max(SCHEMA_VERSION);
}

/// Version-directed fixups run by `load_config` BEFORE the clamps (issue
/// #536). Fail-safe by construction: a file written by a newer binary keeps
/// its own (higher) version and is passed through untouched, so unknown
/// fields are never relabelled as if this binary had produced them.
pub(crate) fn migrate_config(cfg: &mut AppConfig, from: u32) {
    match from {
        // v1 → v2 (4.6): the three additions of CfgDiag#3 (#538) are all
        // additive with serde defaults, so there is nothing to backfill —
        // the step exists so a future breaking change has a home.
        1 => {}
        n if n > SCHEMA_VERSION => log::warn!(
            "[CFG] config written by a newer binary (schema {} > {}); passing through unknown fields",
            n,
            SCHEMA_VERSION
        ),
        _ => {}
    }
    cfg.schema_version = cfg.schema_version.max(SCHEMA_VERSION);
}
/// Frontend event emitted (once per process) when the legacy-plaintext
/// migration finds a *different* secret already in the OS keychain.
///
/// The Settings view should listen for this event and prompt the user to
/// run Settings → Reconnect Spotify. See issue #376.
pub const SPOTIFY_SECRET_CONFLICT_EVENT: &str = "spotify-secret-conflict";
/// One-shot startup migration for the legacy `spotify.client_secret`
/// field (≤ v2.5.0): write it to
/// the OS keychain and strip the plaintext from the file. Idempotent
/// and safe to call on every startup.
///
/// Conflict policy: if the keychain already holds a *different*
/// secret, the migration is a no-op (we don't clobber a working
/// keychain entry with another install's plaintext, and we do NOT delete
/// the plaintext unilaterally — the user may need it). The user resolves
/// the conflict via Settings → Reconnect Spotify. See audit Q3 and
/// issues #9 and #376.
///
/// Bounded notification: the conflict is surfaced exactly once per process
/// (see `migrate_legacy_client_secret_with_app`; a process-wide flag guards
/// the emit) — there is no retry loop or timeout that auto-deletes the
/// plaintext. Manual step: after Reconnect Spotify stores the current
/// secret in the keychain, the next launch either completes the migration
/// (keychain empty / identical value → plaintext stripped) or re-emits
/// this event while the stale plaintext is still present.
/// Log-only variant kept for backward compatibility (no `AppHandle`
/// available at some call sites). Prefer
/// `migrate_legacy_client_secret_with_app`, which additionally surfaces a
/// keychain conflict to the UI via [`SPOTIFY_SECRET_CONFLICT_EVENT`].
pub fn migrate_legacy_client_secret() {
    run_legacy_secret_migration();
}

/// Startup migration with user-visible conflict surfacing (issue #376).
///
/// Runs the same migration as [`migrate_legacy_client_secret`] and returns
/// the observable outcome so the caller can persist it (issue #813); when
/// the outcome is [`LegacySecretOutcome::ConflictKeychainDiffers`], emits a
/// one-time [`SPOTIFY_SECRET_CONFLICT_EVENT`] so Settings can prompt
/// Settings → Reconnect Spotify (payload carries the manual step).
/// All other outcomes are silent apart from the usual `[CFG]` logs.
pub fn migrate_legacy_client_secret_with_app(
    caches: &crate::state::AppCaches,
    app: &tauri::AppHandle,
) -> LegacySecretOutcome {
    let outcome = run_legacy_secret_migration();
    if outcome == LegacySecretOutcome::ConflictKeychainDiffers {
        emit_spotify_secret_conflict_once(caches, app);
    }
    outcome
}
/// Observable outcome of one [`run_legacy_secret_migration`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySecretOutcome {
    /// No `spotify.client_secret` plaintext field (or it was empty):
    /// nothing to do. Also returned when the config file is missing,
    /// unreadable, or unparsable, or a keychain write / file rewrite
    /// failed part-way (plaintext left on disk in those cases).
    NoLegacyField,
    /// Plaintext migrated into an empty keychain (or the keychain
    /// already held the identical value) and the strip pass ran.
    Migrated,
    /// Keychain already holds a *different* secret: plaintext deliberately
    /// left on disk. The caller must surface this via
    /// [`SPOTIFY_SECRET_CONFLICT_EVENT`].
    ConflictKeychainDiffers,
}
/// Pure decision step of the migration: given the legacy plaintext (if any)
/// and the current keychain read, decide the outcome without touching disk
/// or the keychain. Unit-tested directly (issue #376).
pub(crate) fn decide_legacy_secret_outcome(
    plaintext: Option<&str>,
    keychain: &Result<String, String>,
) -> LegacySecretOutcome {
    let plaintext = match plaintext {
        Some(s) if !s.is_empty() => s,
        _ => return LegacySecretOutcome::NoLegacyField,
    };
    match keychain {
        Ok(existing) if existing == plaintext => LegacySecretOutcome::Migrated,
        Ok(_) => LegacySecretOutcome::ConflictKeychainDiffers,
        Err(_) => LegacySecretOutcome::Migrated,
    }
}
/// Per-`AppState` guard so the conflict event fires at most once per launch,
/// no matter how often the migration entry points are called (issue #758
/// slice 2: owned by `AppCaches`, not a process static).
fn emit_spotify_secret_conflict_once(
    caches: &crate::state::AppCaches,
    app: &tauri::AppHandle,
) -> bool {
    if caches.conflict_sent_flag().swap(true, Ordering::AcqRel) {
        return false;
    }
    log::warn!(
        "[CFG] migrate_legacy_client_secret: EMIT {} event (prompt Settings → Reconnect Spotify)",
        SPOTIFY_SECRET_CONFLICT_EVENT
    );
    let _ = app.emit(
        SPOTIFY_SECRET_CONFLICT_EVENT,
        crate::events::SpotifySecretConflict {
            action: "reconnect-spotify".to_string(),
            message: "The Spotify client secret in config.json differs from the one in the OS keychain. Open Settings → Reconnect Spotify to resolve. The legacy plaintext is left untouched until then.".to_string(),
        },
    );
    true
}

/// Bare file name of the sidecar that keeps a conflicting legacy plaintext
/// (issue #803): `config.json.legacy-secret`, beside `config.json`.
pub(crate) const LEGACY_SECRET_SIDECAR_NAME: &str = "config.json.legacy-secret";

/// Write a copy of a conflicting legacy `client_secret` beside `config.json`
/// and return the sidecar's BARE file name (issue #803).
///
/// The migration deliberately leaves the plaintext in `config.json` when the
/// keychain already holds a different value — but `save_config` serialises
/// `AppConfig`, which has no `client_secret` field, so the next save from
/// anywhere (a Settings toggle, a tray snooze, the poller's snooze cleanup)
/// removed the only remaining copy while the app kept authenticating with the
/// stale keychain value. The user could not recover it afterwards: it was shown
/// nowhere in the UI and the file no longer held it. The sidecar is a copy the
/// app never rewrites, so the "the plaintext is not deleted" promise holds past
/// the next write.
///
/// The document holds exactly the one key plus a note, and is never read back
/// by the app — it is the user's copy, for Settings → Reconnect Spotify — which
/// is why this logs the FILE NAME and never the value.
///
/// The write goes through the same atomic-replace, fsync-the-directory helper
/// the config writer uses, so the sidecar is created 0600 on Unix (create_new +
/// mode) and user-only by default ACL on Windows, with no window in which it is
/// world-readable.
pub(crate) fn write_legacy_secret_sidecar(
    config_path: &std::path::Path,
    secret: &str,
) -> Result<String, String> {
    let sidecar = config_path.with_file_name(LEGACY_SECRET_SIDECAR_NAME);
    let document = serde_json::to_string_pretty(&serde_json::json!({
        "client_secret": secret,
        "note": "Legacy Spotify client secret kept from config.json: the OS keychain already held a different value. Resolve via Settings → Reconnect Spotify, then delete this file.",
    }))
    .map_err(|e| format!("Failed to serialize the legacy-secret sidecar: {}", e))?;
    atomic_write_json(&sidecar, &document)?;
    let name = sidecar
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| "Legacy-secret sidecar has no file name".to_string())?;
    log::warn!(
        "[CFG] migrate_legacy_client_secret: the conflicting plaintext is kept in '{}' as well — resolve it via Settings → Reconnect Spotify, then delete that file",
        name
    );
    Ok(name)
}

/// Executes the migration IO and returns its observable outcome.
fn run_legacy_secret_migration() -> LegacySecretOutcome {
    let path = match get_config_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: config path unavailable: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if !path.exists() {
        return LegacySecretOutcome::NoLegacyField; // Fresh install — nothing to migrate.
    }
    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: read failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    // Parse as raw Value so the pre-v2.6.0 nested `spotify.client_secret` field
    // can be inspected and removed BEFORE the typed parse. (`SpotifyConfig`
    // declares no such field, so `serde_json::from_str::<AppConfig>` would drop
    // the value into the section's unknown-key bucket — see issue #938 — and
    // leave it in the file the migration is supposed to clean.)
    let mut root: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: parse failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    let plaintext = legacy_client_secret(&root);
    let keychain_read = crate::keychain::get_spotify_client_secret();
    let outcome = decide_legacy_secret_outcome(plaintext.as_deref(), &keychain_read);
    match (&outcome, &keychain_read) {
        (LegacySecretOutcome::NoLegacyField, _) => {
            log::debug!("[CFG] migrate_legacy_client_secret: no legacy plaintext field");
            return outcome;
        }
        (LegacySecretOutcome::ConflictKeychainDiffers, Ok(existing)) => {
            // Conflict check: if keychain already holds a *different* secret,
            // don't clobber it. Leave the plaintext in place; the user can
            // resolve via Settings → Reconnect Spotify (surfaced via
            // `spotify-secret-conflict`; see `migrate_legacy_client_secret_with_app`).
            let plaintext = plaintext.unwrap_or_default();
            log::warn!(
                "[CFG] migrate_legacy_client_secret: keychain holds a different secret; leaving config.json untouched (user should Reconnect)"
            );
            log::warn!(
                "[CFG] migrate_legacy_client_secret: plaintext.len={}, keychain.len={}",
                plaintext.len(),
                existing.len()
            );
            // Issue #803: the plaintext stays in `config.json` (that is the
            // documented promise), but a copy also goes into a sidecar the app
            // never rewrites, because the next unrelated save would otherwise be
            // its last appearance anywhere.
            if let Err(e) = write_legacy_secret_sidecar(&path, &plaintext) {
                log::warn!(
                    "[CFG] migrate_legacy_client_secret: could not write the legacy-secret sidecar: {}",
                    e
                );
            }
            return outcome;
        }
        _ => {
            // Keychain empty (the typical pre-v2.6.0-upgrader case), or it
            // already holds the identical value (strip-only). Write the
            // plaintext into the keychain only when the keychain is empty.
            if keychain_read.is_err() {
                log::info!("[CFG] migrate_legacy_client_secret: keychain empty, writing plaintext into keychain");
                // `plaintext` is `Some(non-empty)` here: `decide_*` only
                // returns `Migrated` for `Some(non-empty)` input.
                let plaintext = plaintext.unwrap_or_default();
                if let Err(e) = crate::keychain::store_spotify_client_secret(&plaintext) {
                    log::warn!(
                        "[CFG] migrate_legacy_client_secret: keychain write failed: {} (plaintext left in config.json)",
                        e
                    );
                    return LegacySecretOutcome::NoLegacyField;
                }
            } else {
                log::info!(
                    "[CFG] migrate_legacy_client_secret: keychain already holds this value, stripping plaintext only"
                );
            }
        }
    }
    // Strip EVERY `client_secret` key, not only the documented pre-v2.6.0
    // `spotify.client_secret`: a hand-edited or third-party file can nest the
    // same credential under any path, and the migration has just taken
    // responsibility for the value it read (issue #916).
    strip_client_secret_keys(&mut root);
    let new_contents = match serde_json::to_string_pretty(&root) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: re-serialise failed: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if let Err(e) = atomic_write_json(&path, &new_contents) {
        log::warn!(
            "[CFG] migrate_legacy_client_secret: atomic rewrite failed: {} (keychain has the value, plaintext remains on disk)",
            e
        );
    } else {
        log::info!(
            "[CFG] migrate_legacy_client_secret: SUCCESS — plaintext stripped from config.json"
        );
    }
    LegacySecretOutcome::Migrated
}
