//! Configuration load/save Tauri commands.
//!
//! See issue #76.

use crate::config::{self, AppConfig, ConfigPatch};
use crate::AppState;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

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

/// Side effects that must follow a successful config write, shared by every
/// write command so they cannot drift apart.
///
/// Ordering is load-bearing: the logger is re-armed first (CfgDiag#4, issue
/// #539 — a `logging.enabled` / `log_level` change takes effect immediately
/// instead of at the next launch, and the level also governs whether the
/// OS-side effects below are logged), then the native locale (4.7.0, issue
/// #674 — see [`sync_native_locale`]), then the macOS activation policy
/// (which borrows `app`, hence before the by-value `set_autostart_enabled`),
/// then the OS autostart entry.
#[cfg_attr(not(desktop), allow(unused_variables))]
async fn after_persist(app: &AppHandle, persisted: &AppConfig) {
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

    // Sync autostart state with the OS autostart manager. The command is
    // now async (it touches the autostart registry/file), so we await it.
    #[cfg(desktop)]
    {
        if let Err(e) = super::window::set_autostart_enabled(app.clone(), persisted.autostart).await
        {
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

    let chosen = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(&suggested)
        .add_filter("JSON", &["json"])
        .blocking_save_file();
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
    // Same crash-safe write as `save_config` (sidecar + fsync + rename, mode
    // 0600): a failed export must not leave a half-written file where the user
    // was told a complete copy lives.
    config::atomic_write_json(&path, &json)?;

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
/// thread), which is the same pattern the file picker above uses.
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

/// Replace the stored config with a document the user picks.
///
/// Validation happens in `config::import_config_document` before anything is
/// written — a document carrying a plaintext `client_secret` is refused — and
/// the user is asked before the current file is replaced, with the outgoing
/// copy kept as `config.json.bak`. Returns `None` when the picker was dismissed
/// or the overwrite was declined (both are clean no-ops).
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

    let chosen = app
        .dialog()
        .file()
        .set_title(title.clone())
        .add_filter("JSON", &["json"])
        .blocking_pick_file();
    let Some(chosen) = chosen else {
        log::info!("{CMD} import_config: CANCELLED - dialog dismissed");
        return Ok(None);
    };
    let source = chosen
        .into_path()
        .map_err(|e| format!("import_config: unusable source: {}", e))?;
    let raw = std::fs::read_to_string(&source).map_err(|e| {
        format!(
            "import_config: failed to read '{}': {}",
            source.display(),
            e
        )
    })?;
    let source_path = source.to_string_lossy().into_owned();
    let destination = config::get_config_path()?;

    // The validate → ask → replace sequence runs on the blocking pool, and
    // deliberately *without* the config write guard: the confirmation is a
    // user-driven wait, and holding the guard across it would stall the polling
    // loop's config reads for as long as the dialog is on screen.
    let dialog_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        config::import_config_document(&raw, &destination, || {
            ask_overwrite(
                &dialog_app,
                &title,
                &confirm_body,
                &confirm_ok,
                &confirm_cancel,
            )
        })
    })
    .await
    .map_err(|e| format!("import_config spawn_blocking panicked: {:?}", e))??;
    let Some(_written) = outcome else {
        log::info!("{CMD} import_config: DECLINED - configuration left untouched");
        return Ok(None);
    };

    // #215 pattern for the adoption only: the file write is done, and this guard
    // covers the load-then-store pair so a concurrent write cannot interleave.
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        let mut config_guard = state_clone.config.get_mut();
        // The authoritative view of what is now on disk: a real load re-derives
        // the keychain display fields, which never come from an imported file.
        let persisted = config::load_config()?;
        *config_guard = Some(persisted.clone());
        Ok::<AppConfig, String>(persisted)
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
}
