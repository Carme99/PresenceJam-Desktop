//! Issue #870: the user-composed manual Teams status with an expiry.
//!
//! Distinct from the auto-posted Spotify status text the polling loop drives:
//! the user composes the message from the Dashboard composer, the tray
//! "Recent statuses" submenu, or the `--set-status` CLI flag, and the app
//! posts it as a Teams status message via the existing
//! [`set_teams_status_message`] / [`clear_teams_status_message`] pair. An
//! expiry is the contract — a manual status without an expiry is the same
//! shape the paused placeholder posts, but the user owns the wording.
//!
//! The recent-statuses ring (`RECENT_STATUSES`) feeds the tray submenu and
//! the Dashboard composer. It is intentionally small (5 entries — the tray
//! shows 3 and the Dashboard may want the same 5) and dedupes on identical
//! messages so a re-pick from the submenu does not crowd out other entries.
//!
//! The expiry bound lives in [`MAX_MANUAL_EXPIRY_MINUTES`] (5..=720). The
//! shared `clamp_polling`-style ceiling mirrors the preferred-presence one
//! so the two opt-ins read the same way in the Settings pane.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use crate::config::{clamp_rule_text, profanity_extra_words_for_filter};
use crate::profanity;
use crate::teams::{self, TeamsTokens};
use crate::AppState;

/// Issue #870: the maximum expiry minutes a Dashboard composer or the
/// `--set-status-expiry` CLI flag may request. Mirrors the
/// `preferred_presence.expiry_minutes` ceiling so the two opt-ins read
/// identically in the Settings pane and the dashboard composer.
pub const MAX_MANUAL_EXPIRY_MINUTES: u32 = 720;
/// Minimum expiry a manual status accepts (Graph rejects shorter intervals
/// in practice — anything under 5 minutes races the user's intent).
pub const MIN_MANUAL_EXPIRY_MINUTES: u32 = 5;
/// Length of the recent-statuses ring the tray submenu reads from. The tray
/// shows the most recent three; the Dashboard composer wants a bit more
/// history without re-querying.
pub const RECENT_STATUSES_CAPACITY: usize = 5;

/// Issue #870: the in-process record of the manual status currently
/// composed by the user. Distinct from `EXIT_SNAPSHOT.last_posted_status`
/// (which the poller owns) and from `state.tokens.teams` (the auth surface):
/// this is the *user's* status, with the expiry the user picked.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ManualStatus {
    /// The text we POSTed to Teams via `set_teams_status_message` —
    /// profanity-filtered (so the safe placeholder is what the Dashboard
    /// shows when the user's draft tripped the filter) and bounded by
    /// [`MAX_RULE_STATUS_CHARS`].
    pub message: String,
    /// RFC 3339 expiry the Graph POST carries.
    pub expires_at: DateTime<Utc>,
    /// RFC 3339 set-at, so the Dashboard can render "set 4 minutes ago".
    pub set_at: DateTime<Utc>,
}

/// Issue #870: the in-process ring of recently-used manual statuses, the
/// source of the tray's "Recent statuses" submenu and the Dashboard
/// composer's quick-pick list. Bounded so a long-running session cannot
/// accumulate every draft the user ever composed; the dashboard and the
/// tray only read the newest [`RECENT_STATUSES_CAPACITY`] anyway.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct RecentManualStatus {
    pub message: String,
    pub used_at: DateTime<Utc>,
}

static MANUAL_STATUS: Mutex<Option<ManualStatus>> = Mutex::new(None);
static RECENT_STATUSES: Mutex<Vec<RecentManualStatus>> = Mutex::new(Vec::new());

/// Issue #870: the current manual status the user has armed, or `None` if
/// the composer is empty / cleared / expired.
pub fn load_manual_status() -> Option<ManualStatus> {
    let guard = MANUAL_STATUS.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// Issue #870: the snapshot the Dashboard composer renders as the
/// quick-pick list. The tray submenu uses [`load_recent_statuses`] directly
/// because it builds the menu entries inline.
pub fn load_recent_statuses() -> Vec<RecentManualStatus> {
    let guard = RECENT_STATUSES.lock().unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

/// Issue #870: record the just-set manual status. Stores the user's text
/// (not the safe placeholder) so the Dashboard composer re-seeds from the
/// user's last draft, not from the censored form. The recent-statuses ring
/// dedupes on identical messages so a re-pick does not push another entry.
fn store_manual_status(status: Option<ManualStatus>, recent: Option<RecentManualStatus>) {
    if let Ok(mut guard) = MANUAL_STATUS.lock() {
        *guard = status;
    }
    if let Some(entry) = recent {
        if let Ok(mut guard) = RECENT_STATUSES.lock() {
            // Dedup by message; the newest wins so its `used_at` is the
            // the one the Dashboard chip and the tray submenu read.
            guard.retain(|existing| existing.message != entry.message);
            guard.push(entry);
            // Cap the ring at `RECENT_STATUSES_CAPACITY`, dropping the
            // oldest entries so a long-running session cannot leak memory.
            while guard.len() > RECENT_STATUSES_CAPACITY {
                guard.remove(0);
            }
        }
    }
}

/// Issue #870: clear the in-process manual-status record. The Graph clear is
/// the caller's responsibility — this only resets the local cache the
/// Dashboard reads.
fn clear_manual_status_record() {
    if let Ok(mut guard) = MANUAL_STATUS.lock() {
        *guard = None;
    }
}

/// Issue #870: bound the user-supplied expiry. Mirrors the
/// `preferred_presence.expiry_minutes` clamp so the two opt-ins read the
/// same way in the Settings pane.
fn clamp_expiry_minutes(minutes: u32) -> u32 {
    minutes.clamp(MIN_MANUAL_EXPIRY_MINUTES, MAX_MANUAL_EXPIRY_MINUTES)
}
/// Apply the manual-status filter from one config snapshot. Passing both the
/// locale and custom lexicon explicitly keeps this GUI path aligned with the
/// headless CLI and independent of the process-global native-language slot.
fn filter_manual_status(text: &str, config: Option<&crate::config::AppConfig>) -> String {
    let placeholder = config
        .map(|cfg| cfg.teams.profanity_placeholder.as_str())
        .unwrap_or_default();
    let extra_words = profanity_extra_words_for_filter(config);
    let locale = config.and_then(|cfg| cfg.locale.as_deref());
    profanity::filter_status_for_locale(text, placeholder, true, extra_words, locale)
}

/// Issue #870: the Dashboard composer / `--set-status` entry. Filters
/// profanity, clamps the text length, clamps the expiry, POSTs to Teams,
/// and records the result so the Dashboard and the tray can re-render.
///
/// Returns the persisted [`ManualStatus`] — the *filtered* text, so the
/// Dashboard shows what Teams actually shows, not the user's draft.
pub fn set_manual_status_inner(
    state: &AppState,
    app: &AppHandle,
    message: &str,
    expiry_minutes: u32,
) -> Result<ManualStatus, String> {
    // Step 1: trim + clamp. Empty input is a clear (the composer can submit
    // a blank draft to clear the manual status without an explicit clear
    // button); the clamp stays identical to the rule text bound so a 200-char
    // paste is the same shape Teams sees from a rule.
    let mut text = message.trim().to_string();
    clamp_rule_text(&mut text);
    let expiry_minutes = clamp_expiry_minutes(expiry_minutes);
    let now = Utc::now();
    let expires_at = now + ChronoDuration::minutes(expiry_minutes as i64);

    if text.is_empty() {
        // Issue #870: empty text == clear. The composer surfaces a
        // "Type a message" placeholder; submitting blank is the explicit
        // way to reach the clear path without an extra button click.
        clear_manual_status_inner(state, app)?;
        return Ok(ManualStatus {
            message: String::new(),
            expires_at,
            set_at: now,
        });
    }

    // Step 2: profanity filter. Locale and the user's lexicon come from the
    // same config snapshot as the headless CLI and polling paths; no
    // process-global locale is consulted by this native GUI consumer.
    let config_guard = state.config.get();
    let posted_text = filter_manual_status(&text, config_guard.as_ref());
    let filtered = posted_text != text;

    // Step 3: POST to Teams. The expiry is the ISO 8601 the docs document
    // for `setUserPreferredPresence` *and* `setPresenceMessage` (both are
    // `expirationDateTime` / `expirationDuration` ISO-8601; the helper
    // [`teams::manual_status_expiry_rfc3339`] wraps the conversion so the
    // formats cannot drift between callers).
    let tokens = state.tokens.teams().clone();
    let Some(tokens) = tokens else {
        return Err("Teams is not connected; cannot set a manual status".to_string());
    };
    let expiry_str = teams::manual_status_expiry_rfc3339(expires_at);
    teams::set_teams_status_message(&tokens.access_token, &posted_text, Some(&expiry_str))
        .map_err(|e| format!("failed to post manual status to Teams: {}", e))?;

    let manual = ManualStatus {
        message: posted_text.clone(),
        expires_at,
        set_at: now,
    };
    store_manual_status(
        Some(manual.clone()),
        Some(RecentManualStatus {
            message: text.clone(),
            used_at: now,
        }),
    );

    let _ = app.emit(
        "manual-status-updated",
        serde_json::json!({
            "manual_status": manual,
            "filtered": filtered,
            "user_text": text,
        }),
    );
    log::info!(
        "[STATUS] set_manual_status: posted {} chars (filtered={}), expires in {} min",
        manual.message.chars().count(),
        filtered,
        expiry_minutes
    );
    Ok(manual)
}

/// Issue #870: the Dashboard composer's Clear button, the tray "Clear
/// manual status" entry, and the `--clear-status` CLI flag's single entry
/// point. Idempotent — a second call when nothing is armed is a no-op that
/// returns `Ok(())`.
pub fn clear_manual_status_inner(state: &AppState, app: &AppHandle) -> Result<(), String> {
    let Some(tokens): Option<TeamsTokens> = state.tokens.teams().clone() else {
        // The Teams session is gone — the manual status cannot reach Teams,
        // and the local record can only reflect that. Clearing the local
        // record is fine; the next sign-in will start from scratch.
        clear_manual_status_record();
        let _ = app.emit(
            "manual-status-updated",
            serde_json::json!({ "cleared": true }),
        );
        return Ok(());
    };
    teams::clear_teams_status_message(
        &tokens.access_token,
        &crate::commands::sync::safe_placeholder_text(state),
        Some(&teams::placeholder_expiry_rfc3339()),
    )
    .map_err(|e| format!("failed to clear manual status on Teams: {}", e))?;
    clear_manual_status_record();
    let _ = app.emit(
        "manual-status-updated",
        serde_json::json!({ "cleared": true }),
    );
    log::info!("[STATUS] clear_manual_status: manual status cleared");
    Ok(())
}

/// Issue #870: at-expiry tick, called from the poll-loop head. Removes the
/// in-process record when its `expires_at` lapses; the Graph side already
/// cleared itself at the time of `set_teams_status_message`. The local
/// record has to clear too, otherwise the Dashboard composer would keep
/// showing a "manual status is armed" chip after the user-visible expiry.
pub fn tick_manual_status_expiry(app: &AppHandle, now: DateTime<Utc>) {
    let snapshot = load_manual_status();
    let Some(status) = snapshot else {
        return;
    };
    if status.expires_at > now {
        return;
    }
    clear_manual_status_record();
    let _ = app.emit(
        "manual-status-updated",
        serde_json::json!({ "expired": true, "expires_at": status.expires_at.to_rfc3339() }),
    );
    log::info!(
        "[STATUS] manual status expired (set_at={}), cleared local record",
        status.set_at.to_rfc3339()
    );
}

/// Issue #870: when the manual status has expired, the poller should treat
/// the spot it occupies the same as "no manual status" — both the write
/// gate (`manual_status_blocks_write`) and the Dashboard composer are
/// driven off this single helper so the two can drift no further than the
/// store.
pub fn manual_status_is_live(now: DateTime<Utc>) -> bool {
    match load_manual_status() {
        Some(status) => status.expires_at > now,
        None => false,
    }
}

/// Issue #870: tiny plumbing for tests + the manual status CLI parse.
/// Re-exported here so the dispatch contract's tests do not need to import
/// from the private `teams` module's tests.
pub fn placeholder_expiry_minutes() -> u32 {
    crate::commands::sync::placeholder_expiry_minutes()
}

/// Issue #870: the expiry-clamp helper, exposed for the `--set-status-expiry`
/// CLI flag. Lives here so the Dashboard composer (which already calls
/// `set_manual_status`) and the CLI preflight share one bound.
pub fn clamp_expiry_public(minutes: u32) -> u32 {
    clamp_expiry_minutes(minutes)
}

/// Issue #870: the CLI flag's record path. Wraps [`store_manual_status`] so
/// the `--set-status` and the Dashboard composer mutate the same record.
pub fn record_manual_status_cli(manual: ManualStatus, recent: RecentManualStatus) {
    store_manual_status(Some(manual), Some(recent));
}

/// Issue #870: the CLI flag's clear path. Wraps [`clear_manual_status_record`]
/// so the `--clear-status` and the Dashboard composer share the same record
/// reset.
pub fn clear_manual_status_record_cli() {
    clear_manual_status_record();
}

// -----------------------------------------------------------------------------
// IPC commands (Tauri surface for the Dashboard composer + tray submenu).
// -----------------------------------------------------------------------------

/// Issue #870: the Dashboard composer's `Set` button + the tray
/// "Set manual status" submenu's POST handler. Runs the same filter + clamp
/// + Graph POST pipeline the CLI flag runs; the only difference is the
/// error string format (Tauri commands return `String`, CLI runs print to
/// stderr).
#[tauri::command]
pub async fn set_manual_status(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
    message: String,
    expiry_minutes: u32,
) -> Result<ManualStatus, String> {
    super::require_main_window(&window)?;
    let state_inner = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        set_manual_status_inner(&state_inner, &app, &message, expiry_minutes)
    })
    .await
    .map_err(|e| format!("set_manual_status spawn_blocking panicked: {:?}", e))?
}

/// Issue #870: the Dashboard composer's `Clear` button + the tray
/// "Clear manual status" submenu's POST handler.
#[tauri::command]
pub async fn clear_manual_status_command(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    super::require_main_window(&window)?;
    let state_inner = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || clear_manual_status_inner(&state_inner, &app))
        .await
        .map_err(|e| format!("clear_manual_status spawn_blocking panicked: {:?}", e))?
}

/// Issue #870: the `SyncStatus` field exposes the manual status + the
/// recent ring, but the Dashboard composer wants a Tauri command (rather
/// than a freshly read SyncStatus) so it can re-fetch on focus without
/// re-rendering the full status pane.
#[tauri::command]
pub fn load_manual_status_command() -> ManualStatusSnapshot {
    ManualStatusSnapshot {
        manual_status: load_manual_status(),
        recent: load_recent_statuses(),
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ManualStatusSnapshot {
    pub manual_status: Option<ManualStatus>,
    pub recent: Vec<RecentManualStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MAX_RULE_STATUS_CHARS;

    /// Issue #870: a guard around the manual-status record so unit tests do
    /// not race each other when they mutate the global. Every test that
    /// touches [`set_manual_status_inner`] / [`clear_manual_status_inner`]
    /// must hold this guard for the duration of the test.
    pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: Mutex<()> = Mutex::new(());
        GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Issue #870: the documented clamp bounds — the Settings pane reads
    /// the same numbers, so a regression in either is a regression in both.
    #[test]
    fn clamp_expiry_bounds_match_the_settings_pane() {
        assert_eq!(clamp_expiry_minutes(0), MIN_MANUAL_EXPIRY_MINUTES);
        assert_eq!(clamp_expiry_minutes(60), 60);
        assert_eq!(
            clamp_expiry_minutes(MAX_MANUAL_EXPIRY_MINUTES + 60),
            MAX_MANUAL_EXPIRY_MINUTES
        );
        assert_eq!(clamp_expiry_minutes(u32::MAX), MAX_MANUAL_EXPIRY_MINUTES);
    }

    /// Issue #870: the ring dedupes on identical messages and is capped at
    /// [`RECENT_STATUSES_CAPACITY`]. Six pushes with the first one unique
    /// yield a ring of five distinct entries, ordered newest-first because
    /// the tray's "Recent statuses" submenu reads the array end-first.
    #[test]
    fn recent_statuses_ring_caps_and_dedupes() {
        let _guard = lock();
        // Wipe any state the parallel test runner inherited.
        if let Ok(mut g) = RECENT_STATUSES.lock() {
            g.clear();
        }
        let now = Utc::now();
        for i in 0..RECENT_STATUSES_CAPACITY + 1 {
            store_manual_status(
                None,
                Some(RecentManualStatus {
                    message: format!("status {}", i),
                    used_at: now + ChronoDuration::seconds(i as i64),
                }),
            );
        }
        let ring = load_recent_statuses();
        assert_eq!(ring.len(), RECENT_STATUSES_CAPACITY);
        // The oldest push (`status 0`) was evicted by the cap.
        assert!(ring.iter().all(|r| r.message != "status 0"));
        // Pushing an existing entry refreshes its `used_at` and keeps the
        // length — no duplicate slot.
        let before = ring.len();
        store_manual_status(
            None,
            Some(RecentManualStatus {
                message: ring.last().unwrap().message.clone(),
                used_at: now + ChronoDuration::seconds(99),
            }),
        );
        assert_eq!(load_recent_statuses().len(), before);
    }

    /// Issue #870: `manual_status_is_live` honours the expiry. A stored
    /// status whose `expires_at` has lapsed reads `false`, so the write gate
    /// and the Dashboard composer stop claiming a manual status is armed.
    #[test]
    fn manual_status_is_live_respects_expiry() {
        let _guard = lock();
        clear_manual_status_record();
        assert!(!manual_status_is_live(Utc::now()));
        let now = Utc::now();
        store_manual_status(
            Some(ManualStatus {
                message: "live".to_string(),
                expires_at: now + ChronoDuration::minutes(5),
                set_at: now,
            }),
            None,
        );
        assert!(manual_status_is_live(now));
        // 1 ms after expiry: not live.
        assert!(!manual_status_is_live(
            now + ChronoDuration::minutes(5) + ChronoDuration::milliseconds(1)
        ));
    }

    /// Issue #870: the text length bound is [`MAX_RULE_STATUS_CHARS`].
    /// `clamp_rule_text` is the same helper the rule text uses, so a
    /// regression here is a regression for the rules.
    #[test]
    fn set_manual_status_clamps_text_to_max_rule_status_chars() {
        let _guard = lock();
        let oversized = "x".repeat(MAX_RULE_STATUS_CHARS * 2);
        let expected = "x".repeat(MAX_RULE_STATUS_CHARS);
        let mut buf = oversized;
        clamp_rule_text(&mut buf);
        assert_eq!(buf.chars().count(), MAX_RULE_STATUS_CHARS);
        assert_eq!(buf, expected);
    }
    #[test]
    fn manual_status_filter_uses_config_locale_and_custom_copy() {
        for (locale, fallback, custom) in [
            ("de", "Hört gerade Spotify", "Eigener ✨ Status"),
            ("fr", "Écoute actuellement Spotify", "Statut ✨ personnel"),
        ] {
            let mut config = crate::config::AppConfig {
                locale: Some(locale.to_string()),
                ..Default::default()
            };
            for configured in ["", profanity::safe_placeholder_default()] {
                config.teams.profanity_placeholder = configured.to_string();
                assert_eq!(
                    filter_manual_status("fuck", Some(&config)),
                    fallback,
                    "empty and shipped-English placeholders use the config locale"
                );
            }

            config.teams.profanity_placeholder = custom.to_string();
            config.teams.profanity_extra_words = vec!["verboten".to_string()];
            assert_eq!(filter_manual_status("verboten", Some(&config)), custom);
            assert_eq!(
                filter_manual_status("Français ✨ intact", Some(&config)),
                "Français ✨ intact",
                "clean user copy remains byte-identical"
            );
        }
    }
}
