use super::clamp::{
    clamp_locale, clamp_logging, clamp_polling, clamp_presence_profiles, clamp_rules,
    clamp_shortcuts, clamp_teams,
};
use super::io::{clamped_config, replace_with_backup};
use super::migrate::{migrate_config, stamp_schema_version, SCHEMA_VERSION};
use super::schema::{AppConfig, ClientSecretState};
use std::collections::BTreeMap;
/// Timestamp suffix of an export file name — `YYYYMMDD-HHMMSS`, UTC.
pub(crate) fn export_timestamp(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y%m%d-%H%M%S").to_string()
}

/// Name an export is offered under:
/// `presencejam-config-<version>-<YYYYMMDD-HHMMSS>.json`.
pub fn export_file_name(version: &str, at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "presencejam-config-{}-{}.json",
        version,
        export_timestamp(at)
    )
}

/// Walk every `client_secret` key in `value`, calling `visit(dotted_path, value)`
/// for each (issue #916).
///
/// The ONE traversal behind [`client_secret_paths`] (export/import refusal) and
/// [`legacy_client_secret`] (the legacy-plaintext migration), so the two cannot
/// disagree about where a credential may hide. Nested objects AND arrays are
/// walked: a secret cannot escape by sitting inside the unknown-key retention
/// map, or inside a hand-written nested object, merely because the typed schema
/// has no such field.
pub(crate) fn walk_client_secret_keys(
    value: &serde_json::Value,
    visit: &mut impl FnMut(&str, &serde_json::Value),
) {
    fn walk(
        value: &serde_json::Value,
        prefix: &str,
        visit: &mut impl FnMut(&str, &serde_json::Value),
    ) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    if key == "client_secret" {
                        visit(&path, child);
                    }
                    walk(child, &path, visit);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, &format!("{}[{}]", prefix, index), visit);
                }
            }
            _ => {}
        }
    }
    walk(value, "", visit);
}

/// Collect the dotted paths of every `client_secret` key anywhere in `value`.
///
/// Only keys are matched — values are irrelevant to the decision, which is why
/// an explicit `null` or a nested object counts here (the import refusal wants
/// every shape) while [`legacy_client_secret`] wants a string.
pub(crate) fn client_secret_paths(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_client_secret_keys(value, &mut |path, _| out.push(path.to_string()));
    out
}

/// The plaintext credential the legacy migration acts on (issue #916): the
/// documented pre-v2.6.0 `spotify.client_secret` when the document has one,
/// otherwise the first `client_secret` key anywhere that holds a non-empty
/// string.
///
/// Before this, only `spotify.client_secret` was read, so a credential at any
/// other path was neither migrated to the keychain nor removed from disk — it
/// was silently dropped. `None` means there is genuinely nothing to migrate.
pub(crate) fn legacy_client_secret(root: &serde_json::Value) -> Option<String> {
    let mut found: Vec<(String, String)> = Vec::new();
    walk_client_secret_keys(root, &mut |path, value| {
        if let Some(text) = value.as_str().filter(|text| !text.is_empty()) {
            found.push((path.to_string(), text.to_string()));
        }
    });
    let documented = found
        .iter()
        .find(|(path, _)| path == "spotify.client_secret");
    documented.or(found.first()).map(|(_, text)| text.clone())
}

/// Remove every `client_secret` key anywhere in the tree; returns how many
/// were removed.
pub(crate) fn strip_client_secret_keys(value: &mut serde_json::Value) -> usize {
    let mut removed = 0;
    match value {
        serde_json::Value::Object(map) => {
            if map.remove("client_secret").is_some() {
                removed += 1;
            }
            for child in map.values_mut() {
                removed += strip_client_secret_keys(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items.iter_mut() {
                removed += strip_client_secret_keys(child);
            }
        }
        _ => {}
    }
    removed
}

/// Remove `client_secret` keys from ONE unknown-key map (issue #916): the
/// top-level entry, plus any nested inside a retained unknown value. Returns how
/// many keys were removed, so the caller logs a single line.
///
/// An unknown-key bucket is `#[serde(flatten)]` with no entry-level filter, so a
/// key the codebase treats as a credential everywhere else would otherwise be
/// deserialized, handed to the webview by the `load_config` command and
/// re-serialized on the next save.
pub(crate) fn strip_client_secret_from_extra(
    extra: &mut BTreeMap<String, serde_json::Value>,
) -> usize {
    let mut removed = if extra.remove("client_secret").is_some() {
        1
    } else {
        0
    };
    for value in extra.values_mut() {
        removed += strip_client_secret_keys(value);
    }
    removed
}

/// [`strip_client_secret_from_extra`] over every unknown-key bucket a config
/// carries: the document's own top-level map and each section's (issue #916;
/// the section maps are themselves issue #938).
pub(crate) fn strip_client_secret_from_extras(config: &mut AppConfig) -> usize {
    let mut removed = strip_client_secret_from_extra(&mut config.extra);
    // The list MUST stay exhaustive over every `extra` bucket the config
    // carries. Three of them were missed (#938): `playback`, each entry of
    // `presence_profiles`, and `teams.preferred_presence`. A `client_secret`
    // left in one of those buckets is deserialized, handed to the webview by
    // the `load_config` command and re-serialized on the next save — exactly
    // the IPC crossing issue #916 forbids, reachable through a section that
    // only gained its retention map later.
    for extra in [
        &mut config.spotify.extra,
        &mut config.teams.extra,
        &mut config.teams.preferred_presence.extra,
        &mut config.polling.extra,
        &mut config.logging.extra,
        &mut config.updates.extra,
        &mut config.playback.extra,
        &mut config.notifications.extra,
        &mut config.status_rules.extra,
        &mut config.shortcuts.extra,
    ] {
        removed += strip_client_secret_from_extra(extra);
    }
    for profile in &mut config.presence_profiles {
        removed += strip_client_secret_from_extra(&mut profile.extra);
    }
    removed
}

/// The document an export writes (4.7.0, S5): the persisted shape of `cfg`,
/// clamped exactly as [`save_config`] would write it, with every
/// `client_secret` key and both derived keychain views stripped.
///
/// The secret strip is a guard, not the normal path — the secret lives in the
/// OS keychain and there is no typed field to carry it — but a secret reaching
/// a file the user is explicitly told to keep or share would undo the keychain
/// migration. Token material never enters `AppConfig` at all.
///
/// `client_secret_set` / `client_secret_state` go too: they are display
/// projections of *this* machine's keychain (issue #560), so an exported file
/// that carried them would describe the exporting machine to whoever imports
/// it. `load_config` re-stamps both from the real keychain on the next read.
pub fn export_document(cfg: &AppConfig) -> Result<String, String> {
    let mut value = serde_json::to_value(clamped_config(cfg))
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;
    let stripped = strip_client_secret_keys(&mut value);
    if stripped > 0 {
        log::warn!(
            "[CFG] export: stripped {} client_secret key(s) from the exported document",
            stripped
        );
    }
    if let Some(spotify) = value
        .get_mut("spotify")
        .and_then(serde_json::Value::as_object_mut)
    {
        spotify.remove("client_secret_set");
        spotify.remove("client_secret_state");
    }
    // S9 (issue #975): the snooze deadline is RUNTIME state — "pause sync until
    // then" on THIS machine — not a setting. An export is advertised as a
    // shareable settings copy, so a file exported while sync was paused would
    // otherwise hand the recipient the exporter's still-future deadline: that
    // install performs no Spotify or Graph work until it passes, and nothing in
    // the import flow says a pause came with the file.
    if let Some(root) = value.as_object_mut() {
        root.remove("snooze_until");
        // An export is a document this app wrote, so it carries the schema
        // floor the binary is authoritative for (`stamp_schema_version`): the
        // floor only ever raises the value, so a newer source is left alone.
        let floor = root
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .max(u64::from(SCHEMA_VERSION));
        root.insert("schema_version".into(), serde_json::json!(floor));
    }
    serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))
}

/// An imported document that passed validation and normalization.
#[derive(Debug)]
pub struct PreparedImport {
    /// Migrated and clamped config, with the keychain views neutralized.
    pub config: AppConfig,
    /// The exact JSON [`import_config_document`] writes for it.
    pub document: String,
}

/// Validate and normalize an imported config document (4.7.0, S5).
///
/// Refuses a document carrying a `client_secret` key anywhere (the pre-#560
/// plaintext shape, or a hand-edited file) instead of quietly dropping it:
/// silently discarding a credential the user meant to import is worse than
/// telling them why it will not be imported. Every other field takes the same
/// route a file read off disk takes — the version migration and all the
/// clamps — so an import cannot introduce a value the UI could not have saved.
pub fn prepare_import(raw: &str) -> Result<PreparedImport, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("Imported file is not valid JSON: {}", e))?;
    if !value.is_object() {
        return Err(
            "Imported file is not a PresenceJam configuration (expected a JSON object)".to_string(),
        );
    }

    let secrets = client_secret_paths(&value);
    if !secrets.is_empty() {
        return Err(format!(
            "Imported file carries a plaintext client_secret ({}); PresenceJam keeps the Spotify client secret in the OS keychain, never in config.json",
            secrets.join(", ")
        ));
    }
    // No strip pass here: `client_secret_paths` matches the *key* regardless of
    // value, so every shape of it — a string, an explicit null, a nested object
    // — was already refused above. Nothing can reach the document below.

    let mut config: AppConfig = serde_json::from_value(value).map_err(|e| {
        format!(
            "Imported file does not match the PresenceJam configuration schema: {}",
            e
        )
    })?;

    // Same ordering as `load_config`: the version dispatcher runs before the
    // clamps, so a migration's rewritten values are never re-clamped away.
    let from_version = config.schema_version;
    migrate_config(&mut config, from_version);
    clamp_polling(&mut config.polling);
    clamp_teams(&mut config.teams);
    clamp_rules(&mut config.status_rules);
    clamp_logging(&mut config.logging);
    // Issue #869: clamp the profile list + active id on every load
    // (mirrors the other `clamp_*` calls). The active-profile pointer
    // is cleared if its name no longer matches — a hand-edited config
    // or an upgrade that dropped profiles cannot silently land on a
    // phantom id.
    clamp_presence_profiles(&mut config.presence_profiles, &mut config.active_profile);
    // Issue #767: an imported document's locale tag and shortcut bindings take
    // the same clamps as a load, so an import cannot introduce a value the UI
    // could not have saved.
    clamp_locale(&mut config);
    clamp_shortcuts(&mut config.shortcuts);
    // Deliberately NOT `stamp_schema_version`: `migrate_config` raises the
    // version to the floor and passes a *newer* file through at its own
    // version, exactly as `load_config` does. Stamping would relabel a
    // newer document as if this binary had produced it.

    // The two keychain views describe the machine the file came from, never
    // the importing one — `load_config` re-stamps them from the real keychain
    // as soon as the import lands.
    config.spotify.client_secret_set = false;
    config.spotify.client_secret_state = ClientSecretState::Absent;

    // Issue #975: an imported document must never start life paused. The
    // deadline belongs to the exporting machine's runtime state — the export
    // above no longer writes it — and an older or hand-edited file that still
    // carries a future one would silence this machine's polling until it
    // passed, with nothing in the UI explaining why.
    config.snooze_until = None;
    // Same floor as the export path: raising an older document to this binary's
    // schema is the migration the loader would apply anyway; a newer document is
    // never relabelled (`stamp_schema_version` only ever raises).
    stamp_schema_version(&mut config);

    // The rewritten document must not carry the key at all (the issue asserts on
    // its absence, not on a null value), exactly as the export path does.
    let mut value = serde_json::to_value(&config)
        .map_err(|e| format!("Failed to serialize imported config to JSON: {}", e))?;
    if let Some(root) = value.as_object_mut() {
        root.remove("snooze_until");
    }
    let document = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize imported config to JSON: {}", e))?;
    Ok(PreparedImport { config, document })
}

/// Replace the config at `path` with an imported document (4.7.0, S5).
///
/// `confirm_overwrite` is the user's decision, asked **after** the document has
/// been validated and only when there is a current file to replace: a decline is
/// a clean no-op that leaves the live file byte-identical and writes no `.bak`.
/// The dialog itself lives in the command (it needs an `AppHandle`); this shape
/// keeps the decision itself testable against real files.
///
/// The outgoing file is moved to `<path>.bak` first (the same backup path the
/// corrupt-file quarantine uses), so an import is never a one-way door; a
/// missing current file is not an error (a fresh install has nothing to back up)
/// and does not prompt. Refuses before touching disk, so a rejected import
/// leaves both the live config and the previous `.bak` untouched. `Ok(None)`
/// means the user declined.
pub fn import_config_document(
    raw: &str,
    path: &std::path::Path,
    confirm_overwrite: impl FnOnce() -> bool,
) -> Result<Option<AppConfig>, String> {
    let prepared = prepare_import(raw)?;

    if path.exists() && !confirm_overwrite() {
        log::info!(
            "[CFG] import: DECLINED by the user; '{}' left untouched",
            path.display()
        );
        return Ok(None);
    }
    // Issue #939: stage the incoming document to a same-directory sidecar and
    // only then move the live file aside (see `replace_with_backup`). Doing it
    // the other way round — rename first, write second — meant a write that
    // failed, or a process death between the two, left the user with NO live
    // config at all: the next launch booted on defaults and their settings
    // existed only in a `.bak` that nothing in the app restores.
    replace_with_backup(path, &prepared.document)?;
    log::info!(
        "[CFG] import: configuration imported into '{}'",
        path.display()
    );
    Ok(Some(prepared.config))
}
