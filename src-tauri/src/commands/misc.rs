//! Miscellaneous Tauri commands that don't fit the auth/sync/window/onboarding
//! split.
//!
//! See issue #76. Currently holds:
//! - `preview_status` — Settings-page preview renderer (issue #74)
//! - `update_tray_menu_state` — tray menu state update from the frontend

use crate::spotify::TrackInfo;
use crate::tray;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.MISC]";

/// Renders a status-format template against a sample track so the Svelte
/// Settings page can show a live preview without needing a real playing
/// track. Keeps the Rust `format_status` as the single source of truth for
/// the `{artist}` / `{track}` / `{album}` / `{emoji}` substitution rules.
/// See issue #74.
///
/// Issue #342: when the filter is enabled the formatted sample is routed
/// through `filter_status` exactly like the runtime polling loop does, so
/// the preview demonstrates the effective fallback (a whitespace-only
/// placeholder renders the canonical default, not the raw format). The
/// optional profane sample lets the user see that fallback path with a
/// clean-looking template.
///
/// Issue #538: the caller also passes the user's lexicon
/// (`teams.profanity_extra_words`), so the preview reflects the EXTRA words
/// too — a hint that claims they are applied must not be previewed against a
/// matcher that ignores them.
///
/// #215 decision: stays synchronous — pure string substitution, no disk,
/// network, or keychain IO. Offloading to spawn_blocking would add
/// overhead with no benefit.
#[tauri::command]
pub fn preview_status(
    format: String,
    filter_enabled: Option<bool>,
    placeholder: Option<String>,
    profane_sample: Option<bool>,
    extra_words: Option<Vec<String>>,
) -> String {
    log::debug!("{CMD} preview_status: ENTRY - format.len={}", format.len());
    let result = if filter_enabled.unwrap_or(false) {
        let sample = TrackInfo {
            title: if profane_sample.unwrap_or(false) {
                "Shit".to_string()
            } else {
                "Sample Track".to_string()
            },
            artist: "Sample Artist".to_string(),
            album: "Sample Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
        };
        let formatted = crate::spotify::format_status(&sample, &format);
        let effective_placeholder = placeholder
            .as_deref()
            .unwrap_or(crate::profanity::safe_placeholder_default());
        crate::profanity::filter_status(
            &formatted,
            effective_placeholder,
            sample.is_playing,
            extra_words.as_deref().unwrap_or(&[]),
        )
    } else {
        crate::spotify::preview_status_with_sample(&format)
    };
    log::debug!("{CMD} preview_status: SUCCESS");
    result
}

/// Rebuilds the tray menu from authoritative backend state (issue #592).
///
/// The frontend used to pass its own `is_syncing` / `current_track` mirrors
/// here. Those mirrors are event-driven (and absent entirely on a view that
/// never mounted), so committing them made the tray's dedup baseline a tuple
/// the backend never produced: a stale mirror could erase the track row or
/// show "Pause Sync" while `polling.is_syncing` was false, and the truthful
/// rebuild stayed suppressed until the dedup key changed. The command is now
/// a push-only refresh trigger — it takes no state, and any payload the
/// caller still sends is ignored.
#[tauri::command]
pub async fn update_tray_menu_state(app: AppHandle) -> Result<(), String> {
    log::info!("{CMD} update_tray_menu_state: ENTRY");
    // #215: tray::update_tray_menu builds the native menu and may fetch
    // Spotify devices/queue via blocking HTTP (cached_devices / cached_queue
    // in tray.rs call spotify::get_devices with 10 s timeout). Offload to
    // the blocking pool so the UI thread is not frozen.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app_clone.try_state::<Arc<crate::AppState>>() else {
            return Err("AppState not registered".to_string());
        };
        let is_syncing = state
            .polling
            .is_syncing(std::sync::atomic::Ordering::Acquire);
        let current_track = state.polling.current_track().clone();
        tray::update_tray_menu(&app_clone, is_syncing, current_track)
    })
    .await
    .map_err(|e| format!("update_tray_menu_state spawn_blocking panicked: {:?}", e))?
    .map_err(|e| {
        log::error!("{CMD} update_tray_menu_state: FAILED - {}", e);
        e
    })?;
    log::info!("{CMD} update_tray_menu_state: SUCCESS");
    Ok(())
}

/// Restarts the app process. Invoked by the frontend after an update has
/// been downloaded and installed so the new version takes effect.
/// `AppHandle::restart` never returns (it exits the process), so the
/// `!` tail expression coerces into the `Result<(), String>` signature.
///
/// #215: offloaded to spawn_blocking as it touches process state. The
/// blocking thread will exit the process; the async wrapper simply awaits
/// the blocking task (which never returns on success).
#[tauri::command]
pub async fn relaunch_app(window: tauri::Window, app: AppHandle) -> Result<(), String> {
    // Issue #241: process restart is main-window-only (UpdatePrompt is gated
    // to the main window in +layout.svelte); a detached window must never
    // restart the app out from under the user.
    super::require_main_window(&window)?;
    log::info!("{CMD} relaunch_app: ENTRY");
    tauri::async_runtime::spawn_blocking(move || {
        // Issue #806: a payload still staged for "Install on quit" must not
        // survive this restart — `app.restart()` fires `RunEvent::Exit`,
        // which runs `install_pending_on_exit` and would install the staged
        // version on top of the one just installed. Discard it first; the
        // helper is a no-op when nothing is staged.
        crate::updater_bg::discard_staged_update(&app);
        app.restart();
        // app.restart() never returns; this is unreachable, but keep a
        // fallback error shape for the type checker.
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("relaunch_app spawn_blocking panicked: {:?}", e))?
}

#[cfg(test)]
mod tests {
    use super::preview_status;

    /// The canonical fallback the frontend shows when the status text is
    /// filtered (`profanity::SAFE_PLACEHOLDER_DEFAULT`) — user-visible copy,
    /// so it is spelled out here rather than borrowed from the constant.
    const CANONICAL: &str = "Currently Listening to Spotify";
    const ON: Option<bool> = Some(true);
    const OFF: Option<bool> = None;

    /// Drive `preview_status` the way the Settings page does: the format plus
    /// its four optional knobs.
    fn preview(
        format: &str,
        filter: Option<bool>,
        placeholder: Option<&str>,
        profane: Option<bool>,
        extra: Option<Vec<String>>,
    ) -> String {
        preview_status(
            format.to_string(),
            filter,
            placeholder.map(|p| p.to_string()),
            profane,
            extra,
        )
    }

    /// Issue #74/#761: the live preview must render exactly what the poller
    /// would post, so every placeholder the format editor advertises resolves
    /// against the sample. This executes the command itself — the
    /// substitution rules live in `spotify::format_status`, but the command
    /// owns which sample and which branch the preview uses.
    #[test]
    fn preview_renders_every_advertised_placeholder() {
        let table = "{emoji} {artist} — {track} ({album})";
        assert_eq!(
            preview(table, OFF, None, OFF, None),
            "🎵 Sample Artist — Sample Track (Sample Album)"
        );

        // The shipped default template renders as-is with the filter off,
        // even when the caller asks for the profane sample: the toggle, not
        // the sample, is what selects the branch.
        let shipped = "🎵 {artist} - {track} 🎧";
        assert_eq!(
            preview(shipped, Some(false), None, ON, None),
            "🎵 Sample Artist - Sample Track 🎧"
        );
    }

    /// Issue #342: with the filter enabled the preview has to demonstrate the
    /// fallback the runtime applies, so the user is never shown a status the
    /// poller would refuse to post.
    #[test]
    fn preview_shows_the_filtered_fallback_for_a_profane_sample() {
        // `{track}` renders the profane sample title; no placeholder was
        // configured, so the canonical default stands in.
        let profane = "{track}";
        assert_eq!(preview(profane, ON, None, ON, None), CANONICAL);

        // #342: a whitespace-only placeholder is not a placeholder — the same
        // canonical default renders instead of blank status text.
        assert_eq!(preview(profane, ON, Some("   "), ON, None), CANONICAL);

        // A real placeholder is used, and its `{emoji}` token is substituted
        // with the sample's playing state — the branch the runtime fallback
        // shares, so the preview shows what would really be posted.
        assert_eq!(
            preview(profane, ON, Some("{emoji} Hidden"), ON, None),
            "🎵 Hidden"
        );

        // A clean sample passes through the enabled filter untouched.
        let clean = "{artist} - {track}";
        assert_eq!(
            preview(clean, ON, None, OFF, None),
            "Sample Artist - Sample Track"
        );
    }

    /// Issue #538: the caller hands the preview the user's own lexicon, so a
    /// hint that claims extra words are applied is never previewed against a
    /// matcher that ignores them. The same text has to flip between the
    /// filtered fallback and the raw render depending on that lexicon.
    #[test]
    fn preview_applies_the_users_extra_words() {
        let format = "{artist} - {track} darn";
        let extra = Some(vec!["darn".to_string()]);

        assert_eq!(preview(format, ON, None, OFF, extra.clone()), CANONICAL);
        assert_eq!(
            preview(format, ON, None, OFF, None),
            "Sample Artist - Sample Track darn"
        );
    }
}
