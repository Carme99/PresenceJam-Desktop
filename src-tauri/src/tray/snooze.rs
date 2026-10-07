//! tray/snooze.rs — split from tray/mod.rs (#756).

#![allow(unused_imports)]

use super::*;
use crate::i18n::Strings;
use crate::spotify::RepeatState;
use std::time::Instant;
use tauri::{
    menu::{MenuItemBuilder, PredefinedMenuItem, Submenu, SubmenuBuilder},
    AppHandle, Manager,
};

/// The active snooze as one rebuild renders it (4.7.0, S9 / issue #677).
#[derive(Debug, Clone, Copy)]
pub struct TraySnooze {
    pub(crate) status: crate::config::SnoozeStatus,
    /// Whole minutes the countdown shows (rounded up, never below 1).
    minutes_left: i64,
}

/// The snooze half of the dedup key: the deadline plus the minute bucket the
/// countdown is on, so the rebuilt status line cannot be deduped away while the
/// sleep runs (4.7.0, S9 / issue #677).
pub fn snooze_dedup_key(status: &crate::config::SnoozeStatus) -> String {
    format!(
        "{}|{}",
        crate::config::snooze_store_form(status.deadline),
        crate::config::snooze_minutes_left(status.remaining_seconds)
    )
}

/// Resolves the snooze a rebuild must render from the mounted config
/// (4.7.0, S9 / issue #677). Absent config, absent field and an already-passed
/// deadline all mean "not snoozed", which is what makes the tray agree with the
/// poller without a second source of truth.
pub fn resolve_snooze(
    config: Option<&std::sync::Arc<crate::config::AppConfig>>,
) -> Option<TraySnooze> {
    let status = crate::config::snooze_status(config?, chrono::Utc::now())?;
    Some(TraySnooze {
        minutes_left: crate::config::snooze_minutes_left(status.remaining_seconds),
        status,
    })
}

/// The snooze a rebuild must render, read out of the mounted config under a
/// scoped guard (issue #886).
///
/// Only `snooze_until` (plus the clock) is needed, so that one field is copied
/// out instead of cloning the whole `AppConfig` — which allocates its status
/// rules on every poll, including the ones the dedup key discards. The guard
/// lives only for this call, so none survives into the blocking Spotify HTTP
/// below.
pub fn snooze_from_app_state(state: &crate::AppState) -> Option<TraySnooze> {
    let config = state.config.get();
    resolve_snooze(config.as_ref())
}

/// Builds the snooze submenu (4.7.0, S9 / issue #677): the three presets, plus
/// a "Resume sync now" entry while a snooze is active.
///
/// The three presets stay enabled during a snooze on purpose — picking another
/// one re-snoozes from now, which is how a user extends a 30-minute snooze to
/// "until tomorrow" without first resuming. Every label comes from the
/// installed i18n table, so the submenu localizes with the rest of the tray.
///
/// No IO: the caller has already resolved the snooze (and, while one is active,
/// is required to be in [`TrayFetch::CacheOnly`] mode — this builder performs
/// no fetch of its own).
pub fn build_snooze_submenu(
    app: &AppHandle,
    s: &Strings,
    snooze: Option<&TraySnooze>,
) -> Result<Submenu<tauri::Wry>, String> {
    let thirty = MenuItemBuilder::with_id(ID_SNOOZE_30, s.snooze_30_minutes)
        .build(app)
        .map_err(|e| e.to_string())?;
    let hour = MenuItemBuilder::with_id(ID_SNOOZE_1H, s.snooze_1_hour)
        .build(app)
        .map_err(|e| e.to_string())?;
    let tomorrow = MenuItemBuilder::with_id(ID_SNOOZE_TOMORROW, s.snooze_until_tomorrow)
        .build(app)
        .map_err(|e| e.to_string())?;
    let mut leading: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        vec![&thirty, &hour, &tomorrow];
    // Issue #867: surface "Until this meeting ends" only while a busy meeting
    // is in progress (calendar cache is non-empty AND has a busy event
    // covering `now`). Outside of a meeting the entry is hidden so the
    // user can't pick a deadline that's effectively the same as "30
    // minutes" with an opaque end time.
    let meeting_active = {
        let state = app.state::<std::sync::Arc<crate::AppState>>();
        state.calendar.meeting_active(chrono::Utc::now())
    };
    // Issue #867: a single allocated `MenuItem` for the meeting entry, kept
    // alive for the duration of `leading`'s borrow. Boxed so the conditional
    // push does not need a separate `Option` shim and the references remain
    // `Send`/`Sync`-safe to copy into the `items` slice below.
    let meeting_entry: Option<tauri::menu::MenuItem<tauri::Wry>> = if meeting_active {
        Some(
            MenuItemBuilder::with_id(ID_SNOOZE_NEXT_MEETING, s.snooze_until_next_meeting_ends)
                .build(app)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    if let Some(ref entry) = meeting_entry {
        leading.push(entry);
    }
    let submenu = SubmenuBuilder::new(app, s.snooze_pause_menu)
        .items(&leading)
        .build()
        .map_err(|e| e.to_string())?;

    if snooze.is_some() {
        let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
        let resume = MenuItemBuilder::with_id(ID_SNOOZE_RESUME, s.snooze_resume_now)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&separator).map_err(|e| e.to_string())?;
        submenu.append(&resume).map_err(|e| e.to_string())?;
    }
    Ok(submenu)
}

/// Issue #869: the "Active profile" submenu. The spec says the tray must
/// expose a profile picker beside the snooze items so the runtime overlay
/// path (`effective_config`) has a UI surface to flip from. The picker
/// has three states, matching the three resolve paths `effective_config`
/// implements:
///
/// 1. `active_profile == None` → the "Base configuration" entry is the
///    only enabled one. Picking it is a no-op (already on base), and the
///    rest are unchecked picks that activate the matching profile.
/// 2. `active_profile == Some(name)` → the matching entry gets a check
///    mark. Picking it is also a no-op.
/// 3. The list is empty (the documented pre-5.0 default and a hand-
///    deleted config) → the submenu shows a single disabled placeholder.
///
/// The id format `{PROFILE_ITEM_PREFIX}|{name}` lets the click handler
/// resolve the picked name without a parallel name→id map. `name` has
/// already been deduped and truncated by `clamp_presence_profiles`, so
/// the only remaining edge case is the single-pipe `name`, which the
/// clamp forbids via the `snooze_pause_menu` style regex — see
/// `clamp_profile_id`. Picking a profile that has been deleted between
/// the rebuild and the click is logged and ignored by the handler.
pub fn build_profile_submenu(
    app: &AppHandle,
    s: &Strings,
    profiles: &[crate::config::PresenceProfile],
    active: Option<&str>,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = SubmenuBuilder::new(app, s.profile_menu)
        .build()
        .map_err(|e| e.to_string())?;

    // The "Base configuration" entry is the only path that clears the
    // active profile. Checked when `active == None`, disabled when there
    // is no profile list to switch off of.
    let base_checked = active.is_none();
    let base_enabled = !profiles.is_empty() || active.is_some();
    let base_item = CheckMenuItemBuilder::with_id(ID_PROFILE_BASE, s.profile_base)
        .checked(base_checked)
        .enabled(base_enabled)
        .build(app)
        .map_err(|e| e.to_string())?;
    submenu.append(&base_item).map_err(|e| e.to_string())?;

    if profiles.is_empty() {
        let empty =
            MenuItemBuilder::with_id(format!("{PROFILE_ITEM_PREFIX}empty"), s.profile_empty)
                .enabled(false)
                .build(app)
                .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
        return Ok(submenu);
    }

    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    submenu.append(&separator).map_err(|e| e.to_string())?;

    for profile in profiles {
        let checked = active == Some(profile.name.as_str());
        let picker = CheckMenuItemBuilder::with_id(
            format!("{PROFILE_ITEM_PREFIX}{}", profile.name),
            profile.name.clone(),
        )
        .checked(checked)
        .build(app)
        .map_err(|e| e.to_string())?;
        submenu.append(&picker).map_err(|e| e.to_string())?;
    }
    Ok(submenu)
}

/// Issue #870: the "Recent statuses" submenu. The spec caps the visible
/// entries at three; the Dashboard composer reads the broader
/// [`crate::commands::status::RECENT_STATUSES_CAPACITY`] ring. The labels
/// use the recent-status entry's exact text (filtered through the same
/// `MAX_RULE_STATUS_CHARS` bound as the composer), so a user picking
/// "Right back at 2" from the tray gets the same text the Dashboard would
/// have posted.
///
/// The "Clear manual status" entry sits at the bottom, only while a
/// manual status is armed — picking it routes through the same helper the
/// Dashboard composer's Clear button uses.
pub fn build_manual_status_submenu(
    app: &AppHandle,
    s: &Strings,
    recent: &[crate::commands::status::RecentManualStatus],
    armed: Option<&crate::commands::status::ManualStatus>,
) -> Result<Submenu<tauri::Wry>, String> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    let shown = recent.iter().take(3);
    for (idx, entry) in shown.enumerate() {
        let item = MenuItemBuilder::with_id(
            format!("{MANUAL_STATUS_ITEM_PREFIX}{idx}"),
            entry.message.clone(),
        )
        .build(app)
        .map_err(|e| e.to_string())?;
        items.push(Box::new(item));
    }
    let has_items = !items.is_empty();
    let submenu = SubmenuBuilder::new(app, s.manual_status_recent_menu)
        .build()
        .map_err(|e| e.to_string())?;
    if !has_items {
        let empty = MenuItemBuilder::with_id(
            format!("{MANUAL_STATUS_ITEM_PREFIX}none"),
            s.manual_status_recent_empty,
        )
        .enabled(false)
        .build(app)
        .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for item in items
            .iter()
            .map(|b| b.as_ref() as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        {
            submenu.append(item).map_err(|e| e.to_string())?;
        }
    }
    if armed.is_some() {
        let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
        let clear = MenuItemBuilder::with_id(ID_MANUAL_STATUS_CLEAR, s.manual_status_clear)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&separator).map_err(|e| e.to_string())?;
        submenu.append(&clear).map_err(|e| e.to_string())?;
    }
    Ok(submenu)
}

/// What a snooze menu id asked for (4.7.0, S9 / issue #677). Parsed rather
/// than switched on the raw id so the click arm stays a dispatch table and the
/// mapping is unit-testable without a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozeMenuSelection {
    Preset(crate::config::SnoozePreset),
    Resume,
}

/// Parses a snooze menu-item id. `None` for anything that is not one of the
/// five known ids — a stale menu from an older build must not snooze by
/// accident.
pub fn parse_snooze_menu_id(id: &str) -> Option<SnoozeMenuSelection> {
    match id.strip_prefix(SNOOZE_ITEM_PREFIX)? {
        SNOOZE_30_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::ThirtyMinutes,
        )),
        SNOOZE_1H_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::OneHour,
        )),
        SNOOZE_TOMORROW_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::UntilTomorrow,
        )),
        // Issue #867: the meeting-bound preset is parsed here so a stale
        // menu from a build that never had it cannot pick the wrong
        // deadline. The click handler still queries the calendar cache to
        // compute the actual end-of-meeting instant.
        SNOOZE_NEXT_MEETING_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::UntilNextMeetingEnds,
        )),
        SNOOZE_RESUME_SUFFIX => Some(SnoozeMenuSelection::Resume),
        _ => None,
    }
}

/// Persists a snooze deadline (or clears it) into `AppConfig::snooze_until`
/// (4.7.0, S9 / issue #677).
///
/// The write half only — callers log the action, so the copy matches what
/// happened (a preset, an explicit resume, or the startup cleanup of a deadline
/// that expired while the app was closed). Follows
/// `commands::config::update_config` exactly: hold the config write guard across
/// the atomic write, persist the CLAMPED copy with the binary-owned
/// `schema_version` stamped, and store THAT value — otherwise the in-memory
/// config (what the poller reads) and config.json disagree until the next launch
/// (issues #297 / #536).
///
/// `None` clears the field, which is both "Resume sync now" and the state the
/// frontend writes when its chip's Resume button is used.
pub fn store_snooze(
    app: &AppHandle,
    until: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), String> {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let mut guard = state.config.get_mut();
    let base = match guard.as_ref() {
        Some(current) => (**current).clone(),
        None => crate::config::load_config()?,
    };
    let mut next = base;
    next.snooze_until = until.map(crate::config::snooze_store_form);

    let mut persisted = crate::config::clamped_config(&next);
    crate::config::stamp_schema_version(&mut persisted);
    crate::config::save_config(&persisted)?;
    *guard = Some(std::sync::Arc::new(persisted));

    Ok(())
}

/// Persists a user-chosen snooze (or its removal) and logs the action
/// (4.7.0, S9 / issue #677). The tray's three click paths all land here.
pub fn write_snooze(
    app: &AppHandle,
    until: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), String> {
    store_snooze(app, until)?;
    match until {
        Some(deadline) => log::info!(
            "[TRAY] snooze: polling paused until {} (local {}, {} min)",
            crate::config::snooze_store_form(deadline),
            deadline.with_timezone(&chrono::Local).format("%H:%M"),
            crate::config::snooze_minutes_left((deadline - chrono::Utc::now()).num_seconds()),
        ),
        None => log::info!("[TRAY] snooze: resumed by the user"),
    }
    Ok(())
}

/// Issue #869: switch the active presence profile from the tray. Mirrors
/// [`write_snooze`] exactly — same read-merge-write lock, same `clamped_config`
/// path, same store-what-was-persisted discipline. `Some(name)` activates the
/// named profile (the click handler has already validated the name against the
/// clamped list); `None` clears the active profile back to the base
/// configuration.
///
/// The write goes through `clamped_config` so the active-profile pointer is
/// re-validated against the stored list — a profile deleted between the menu
/// rebuild and the click is dropped here too, not just by the click handler.
pub fn write_active_profile(app: &AppHandle, name: Option<String>) -> Result<(), String> {
    store_active_profile(app, name.clone())?;
    match name {
        Some(profile) => log::info!("[TRAY] profile: active profile is now {:?}", profile),
        None => log::info!("[TRAY] profile: cleared — using base configuration"),
    }
    Ok(())
}

/// Issue #869: the write half of the tray's profile picker. Holds the config
/// write guard across the atomic save and stores the clamped copy so the
/// in-memory config (what the poller reads) and `config.json` agree on the
/// next launch — exactly the discipline `store_snooze` and
/// `commands::config::update_config` enforce.
pub fn store_active_profile(app: &AppHandle, name: Option<String>) -> Result<(), String> {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let mut guard = state.config.get_mut();
    let base = match guard.as_ref() {
        Some(current) => (**current).clone(),
        None => crate::config::load_config()?,
    };
    let mut next = base;
    next.active_profile = name;
    let mut persisted = crate::config::clamped_config(&next);
    crate::config::stamp_schema_version(&mut persisted);
    crate::config::save_config(&persisted)?;
    *guard = Some(std::sync::Arc::new(persisted));
    Ok(())
}

/// Clears a stored snooze deadline that has already passed, once, as the app
/// comes up (4.7.0, S9 / issue #677).
///
/// `config::load_config` is a READER on paths that hold no config-write guard,
/// so it reports an expired deadline without touching the document (a reader
/// that fixed the field in memory would hide the expiry from every writer that
/// can correct `config.json` — see that fn). This is the writer: a guarded
/// store through [`store_snooze`], run at startup, so a snooze that expired
/// while the app was closed does not sit in `config.json` logging a clamp line
/// on every launch. The poller's own expiry arm covers a deadline that lapses
/// while the app is running; either writer alone is sufficient.
pub fn clear_expired_snooze_at_startup(app: &AppHandle) {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let expired = state
        .config
        .get()
        .as_ref()
        .is_some_and(|cfg| crate::config::snooze_expired_deadline(cfg, chrono::Utc::now()));
    if !expired {
        return;
    }
    match store_snooze(app, None) {
        Ok(()) => {
            log::info!("[TRAY] snooze: cleared the expired deadline left by the previous session")
        }
        Err(e) => log::warn!(
            "[TRAY] snooze: could not clear the expired deadline ({}); the poller will retry",
            e
        ),
    }
}

/// The tray's status line while a snooze is active (4.7.0, S9 / issue #677):
/// the word, the remaining time and the local deadline.
///
/// This replaces [`sync_status_line`] rather than prefixing it: a snoozed
/// session is deliberately not syncing anything, so any words about the current
/// track would be a claim about work that is not happening. The countdown needs
/// no locale-specific plural handling — the unit is a table entry, and the
/// minute count is rounded so it never reads "0 min left" while the snooze is
/// still live (see `config::snooze_minutes_left`).
///
/// Localized from the table it is handed, like every other label here; the
/// `→` and `—` separators are locale-neutral, matching `sync_status_line`.
pub fn snooze_status_line(strings: &Strings, snooze: &TraySnooze) -> String {
    format!(
        "{} — {} {} (→ {})",
        strings.snooze_paused,
        snooze.minutes_left,
        strings.snooze_minutes_left,
        snooze
            .status
            .deadline
            .with_timezone(&chrono::Local)
            .format("%H:%M")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{DE, EN, FR};
    use crate::tray::testkit::MODE_ATOM_LOCK;
    use crate::tray::testkit::{body_of, tray_prod_source};
    /// A config carrying `snooze_until` as stored.
    fn snoozed_config(stored: &str) -> std::sync::Arc<crate::config::AppConfig> {
        std::sync::Arc::new(crate::config::AppConfig {
            snooze_until: Some(stored.to_string()),
            ..crate::config::AppConfig::default()
        })
    }
    /// A deadline `seconds` from now, in the stored spelling.
    fn deadline_in(seconds: i64) -> String {
        crate::config::snooze_store_form(chrono::Utc::now() + chrono::TimeDelta::seconds(seconds))
    }
    /// Every id the submenu can mint maps to exactly one action, and anything
    /// else is refused — a stale menu from an older build must not snooze.
    #[test]
    fn snooze_menu_ids_parse_to_their_action() {
        use crate::config::SnoozePreset;
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_30),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::ThirtyMinutes))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_1H),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::OneHour))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_TOMORROW),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::UntilTomorrow))
        );
        // Issue #867: the meeting-bound preset id parses to the new variant.
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_NEXT_MEETING),
            Some(SnoozeMenuSelection::Preset(
                SnoozePreset::UntilNextMeetingEnds
            ))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_RESUME),
            Some(SnoozeMenuSelection::Resume)
        );
        // The prefix alone, an unknown suffix and a foreign id all resolve to
        // nothing rather than to a default preset.
        for unknown in [
            SNOOZE_ITEM_PREFIX,
            "snooze|2h",
            "play_pause",
            "devices|abc",
            "",
        ] {
            assert_eq!(parse_snooze_menu_id(unknown), None, "id {:?}", unknown);
        }
    }
    /// The status line states the remaining time and the local deadline, in
    /// every locale (issue #677 + the #674 table contract).
    #[test]
    fn snooze_status_line_counts_down_in_every_locale() {
        let snooze = TraySnooze {
            status: crate::config::SnoozeStatus {
                // A fixed instant so the rendered `HH:MM` is stable; the local
                // hour depends on the runner's zone, so it is rebuilt here from
                // the same instant rather than hard-coded.
                deadline: chrono::Utc::now() + chrono::TimeDelta::minutes(30),
                remaining_seconds: 30 * 60,
            },
            minutes_left: 30,
        };
        let local = snooze
            .status
            .deadline
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string();

        assert_eq!(
            snooze_status_line(&EN, &snooze),
            format!("Snoozed — 30 min left (→ {})", local)
        );
        assert_eq!(
            snooze_status_line(&DE, &snooze),
            format!("Sync pausiert — 30 Min. verbleibend (→ {})", local)
        );
        assert_eq!(
            snooze_status_line(&FR, &snooze),
            format!("Synchro en pause — 30 min restant (→ {})", local)
        );
        // The countdown is the ONLY thing the line says — a snoozed session is
        // not syncing, so it must not name a track.
        assert!(!snooze_status_line(&EN, &snooze).contains("Syncing"));
    }
    /// A snooze is resolved from the stored value alone, and everything that is
    /// not a live deadline resolves to "not snoozed" — which is what keeps the
    /// tray and the poller agreeing without a second source of truth.
    #[test]
    fn resolve_snooze_accepts_only_a_live_deadline() {
        assert!(resolve_snooze(None).is_none(), "no config loaded");
        assert!(resolve_snooze(Some(&std::sync::Arc::new(
            crate::config::AppConfig::default()
        )))
        .is_none());
        assert!(resolve_snooze(Some(&snoozed_config(&deadline_in(-60)))).is_none());
        assert!(resolve_snooze(Some(&snoozed_config("not a timestamp"))).is_none());

        let live = resolve_snooze(Some(&snoozed_config(&deadline_in(30 * 60))))
            .expect("a future deadline is an active snooze");
        assert_eq!(live.minutes_left, 30, "a fresh 30-minute snooze reads 30");
        assert!(live.status.remaining_seconds > 0);
    }
    /// The dedup key carries the snooze, so a snooze that starts, ends or ticks
    /// over to the next minute repaints instead of early-returning as a no-op
    /// (the countdown would otherwise freeze at the minute the menu was built).
    #[test]
    fn snooze_change_forces_a_tray_rebuild() {
        let caches = crate::state::AppCaches::new();
        note_playback_modes(&caches, false, RepeatState::Off);
        let track = crate::spotify::TrackInfo {
            title: "Title".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: None,
            duration_ms: 0,
            volume_percent: None,
            supports_volume: None,
            actions: None,
        };
        let at = |snooze: Option<String>| {
            tray_snapshot_for(
                true,
                true,
                Some(&track),
                snooze,
                0,
                0,
                None,
                caches.shuffle_flag().load(Ordering::Acquire),
                last_repeat_state(&caches),
            )
        };

        let none = at(None);
        let snoozed = at(Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline: chrono::Utc::now() + chrono::TimeDelta::minutes(30),
            remaining_seconds: 30 * 60,
        })));
        assert!(
            tray_state_changed(Some(&none), &snoozed),
            "starting a snooze must repaint (issue #677)"
        );
        assert!(
            tray_state_changed(Some(&snoozed), &none),
            "ending a snooze must repaint (issue #677)"
        );

        // The minute bucket is part of the key: same deadline, next minute.
        let deadline = chrono::Utc::now() + chrono::TimeDelta::minutes(30);
        let minute_29 = Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline,
            remaining_seconds: 29 * 60,
        }));
        let minute_30 = Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline,
            remaining_seconds: 30 * 60,
        }));
        assert!(
            tray_state_changed(Some(&at(minute_30)), &at(minute_29.clone())),
            "the countdown must move once a minute, not once per snooze"
        );
        assert!(
            !tray_state_changed(Some(&at(minute_29.clone())), &at(minute_29)),
            "a rebuild with nothing changed still dedupes"
        );
    }
    /// A snooze means "no Spotify request", and the tray's own rebuild is one
    /// of the two things that still runs while snoozed — so its fetch mode must
    /// follow the snooze rather than the caller.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the snooze-aware fetch wiring — that the rebuild derives fetch mode from paint+snooze for both submenu sources — the rebuild needs a live menu + AppHandle; the fetch-mode decision (tray_fetch_mode/paint_fetch_mode) is driven behaviourally; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn snooze_makes_the_tray_rebuild_cache_only() {
        assert_eq!(tray_fetch_mode(true), TrayFetch::CacheOnly);
        assert_eq!(tray_fetch_mode(false), TrayFetch::Refresh);

        // Structural: the rebuild derives its fetch choice from the snooze and
        // hands it to BOTH submenu sources — a literal `Refresh` in either call
        // would re-fetch devices/queue every throttle window while snoozed.
        let prod = tray_prod_source();
        let body = body_of(prod, "fn rebuild_tray_menu(");
        assert!(
            body.contains("paint_fetch_mode(paint, snooze.is_some(), action_refresh_due)"),
            "the rebuild's fetch mode must come from the paint and the snooze"
        );
        // …and the snooze rule itself is unchanged: an ordinary rebuild while
        // snoozed is cache-only (issue #677), while the startup paint is
        // cache-only whatever the snooze says (issue #882).
        let decider = body_of(prod, "fn paint_fetch_mode(");
        assert!(
            decider.contains("tray_fetch_mode(snoozed)"),
            "an ordinary rebuild must take its fetch mode from the snooze (issue #677)"
        );
        for call in [
            "devices_for_menu(caches, access_token.as_deref(), fetch)",
            "queue_for_menu(caches, access_token.as_deref(), fetch)",
        ] {
            assert!(
                body.contains(call),
                "`{}` must honour the snooze-aware fetch mode",
                call
            );
        }
        assert!(
            !body.contains("cached_devices(caches,") && !body.contains("cached_queue(caches,"),
            "the rebuild must go through the fetch-mode helpers, not the raw fetchers"
        );
    }
    /// The snooze must be resolved BEFORE the dedup comparison — a key computed
    /// after the early return would never see it.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the snooze-before-dedup ordering — that the snooze resolves before the dedup guard — the rebuild needs a live window + config; the order is pinned textually at the builder body; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn snooze_is_resolved_before_the_dedup_guard() {
        let prod = tray_prod_source();
        let body = body_of(prod, "fn rebuild_tray_menu(");
        // Issue #886: the snooze comes from a scoped read of the mounted config
        // (`snooze_from_app_state`) instead of a whole-`AppConfig` clone.
        let resolve = body
            .find("snooze_from_app_state(")
            .expect("update_tray_menu must resolve the snooze");
        let snapshot = body
            .find("tray_snapshot_for(")
            .expect("update_tray_menu must build the dedup key");
        let guard = body
            .find("tray_state_changed(")
            .expect("update_tray_menu must keep the dedup guard");
        assert!(
            resolve < snapshot && snapshot < guard,
            "the snooze must be part of the dedup key, evaluated before the guard"
        );
        assert!(
            body.contains("snooze_key"),
            "the dedup key must carry the snooze (deadline + minute bucket)"
        );
    }
    /// A snooze click writes `config.json`, so it must happen off the
    /// menu-event thread, and the repaint must follow the write.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the snooze click-arm offload — that the arm writes through write_snooze and repaints off the menu thread — the arm needs a live AppHandle (config write + repaint); the classification itself is covered by tray_click_target_orders_every_arm; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn snooze_click_arms_write_off_thread_and_repaint() {
        let prod = tray_prod_source();
        let body = body_of(prod, "pub fn handle_menu_event(");
        let arm_pos = body
            .find("TrayClickTarget::Snooze =>")
            .expect("handle_menu_event must handle the Snooze decision");
        let arm_end = body[arm_pos..]
            .find("TrayClickTarget::Profile =>")
            .map(|i| arm_pos + i)
            .unwrap_or(body.len());
        let arm = &body[arm_pos..arm_end];
        assert!(
            arm.contains("std::thread::spawn"),
            "the snooze arm must not write config.json on the menu-event thread"
        );
        assert!(
            arm.contains("write_snooze("),
            "the snooze arm must go through the config-writing helper"
        );
        assert!(
            arm.contains("repaint_tray_from_state("),
            "the snooze arm must repaint after the write"
        );
        // The worker must own the id: the menu-event borrow cannot outlive it
        // (a `&str` borrowed from `event` fails to compile in the spawn).
        assert!(
            arm.contains("id.to_string()"),
            "the snooze arm must copy the id before spawning the worker"
        );
        assert!(
            !arm.contains("tokens.spotify()"),
            "the snooze arm must not touch the Spotify token path"
        );
    }
    /// The tray's snooze writer follows the same store-what-was-persisted
    /// discipline as `commands::config::update_config` (issues #297 / #536) — an
    /// in-memory value that disagrees with disk would show a countdown for a
    /// snooze the next launch does not honour.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the store-what-was-persisted discipline — that the in-memory store follows the successful save — the atomic save needs a live filesystem + config guard; the order is pinned textually at the writer body; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn store_snooze_persists_the_clamped_stamped_value() {
        let prod = tray_prod_source();
        let body = body_of(prod, "fn store_snooze(");
        for marker in [
            "state.config.get_mut()",
            "crate::config::clamped_config(&next)",
            "crate::config::stamp_schema_version(&mut persisted)",
            "crate::config::save_config(&persisted)",
            "*guard = Some(std::sync::Arc::new(persisted))",
        ] {
            assert!(
                body.contains(marker),
                "store_snooze must contain `{}`",
                marker
            );
        }
        // A failed write must leave the in-memory config alone, so the tray
        // never renders a snooze that is not on disk.
        assert!(
            body.find("save_config(&persisted)?").unwrap_or(usize::MAX)
                < body
                    .find("*guard = Some(std::sync::Arc::new(persisted))")
                    .unwrap_or(0),
            "the store must happen only after a successful save"
        );
        // The write half holds no log lines: the copy belongs to the caller, so
        // the startup cleanup cannot masquerade as a user resume.
        assert!(
            !body.contains("log::info!"),
            "store_snooze must not log — the caller owns the message"
        );
    }
    /// The tray must log what it did: the deadline on the way in and the
    /// explicit resume on the way out (issue #677's logging contract).
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the snooze log lines — that start and resume log their own copies — log text is not observable behaviour a hermetic test can assert on; the write path itself is exercised elsewhere; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn write_snooze_logs_both_edges() {
        let prod = tray_prod_source();
        let body = body_of(prod, "fn write_snooze(");
        assert!(
            body.contains("store_snooze(app, until)?"),
            "write_snooze must be the logging wrapper around the writer"
        );
        assert!(
            body.contains("[TRAY] snooze: polling paused until"),
            "the snooze start must be logged with its deadline"
        );
        assert!(
            body.contains("[TRAY] snooze: resumed by the user"),
            "the manual resume must be logged"
        );
    }
    /// Issue #869: the tray's profile picker mirrors `write_snooze`'s
    /// discipline — a write helper that takes the logger's copy and a
    /// store helper that holds the config write guard across the atomic
    /// save. Splitting the two means the same guarded path can be reused
    /// by the CLI without re-doing the lock dance.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the profile writer discipline + log lines — that the guarded writer stores after save and logs both edges — the atomic save + log text need a live filesystem + AppHandle; the order is pinned textually; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn write_active_profile_uses_the_guarded_writer_and_logs_both_edges() {
        let prod = tray_prod_source();
        let body = body_of(prod, "fn write_active_profile(");
        assert!(
            body.contains("store_active_profile(app, name.clone())?"),
            "write_active_profile must be the logging wrapper around the writer"
        );
        assert!(
            body.contains("[TRAY] profile: active profile is now"),
            "an active profile switch must be logged"
        );
        assert!(
            body.contains("[TRAY] profile: cleared — using base configuration"),
            "clearing the active profile must be logged with its own copy"
        );

        let store = body_of(prod, "fn store_active_profile(");
        for marker in [
            "state.config.get_mut()",
            "crate::config::clamped_config(&next)",
            "crate::config::stamp_schema_version(&mut persisted)",
            "crate::config::save_config(&persisted)?",
            "*guard = Some(std::sync::Arc::new(persisted))",
        ] {
            assert!(
                store.contains(marker),
                "store_active_profile must contain `{}`",
                marker
            );
        }
    }
    /// Issue #869: the menu build wires the profile submenu next to the
    /// snooze items and the click handler dispatches both the base
    /// sentinel and the per-profile prefixed ids through the same off-
    /// thread writer.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the profile submenu + click-arm wiring — that the build carries the submenu and the dispatcher handles the Profile decision — the build and arm need a live menu + AppHandle; the id classification is covered by tray_click_target_orders_every_arm; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn profile_submenu_is_in_the_main_menu_and_its_click_arm_is_off_thread() {
        let prod = tray_prod_source();
        let build_profile = body_of(prod, "fn build_profile_submenu(");
        assert!(
            build_profile.contains("ID_PROFILE_BASE"),
            "the picker must carry the base sentinel id"
        );
        assert!(
            build_profile.contains("PROFILE_ITEM_PREFIX"),
            "the picker must prefix per-profile ids"
        );
        // The dedup key carries the active id, so a profile switch repaints.
        let snapshot = body_of(prod, "fn tray_snapshot_for(");
        assert!(
            snapshot.contains("active_profile_key"),
            "the dedup key must carry the active profile id so a switch repaints"
        );
        // The click handler validates the picked name and offloads the write.
        // Classification lives in the pure `tray_click_target` decision fn
        // (pinned behaviourally in tray/mod.rs) — the dispatcher only needs
        // the decision variant here, not the raw id shapes.
        let setup = body_of(prod, "pub fn handle_menu_event(");
        assert!(
            setup.contains("TrayClickTarget::Profile =>"),
            "the click dispatcher must handle the Profile decision"
        );
        assert!(
            setup.contains("write_active_profile(&app_handle, target)"),
            "the click handler must call the write helper, never inline a config write"
        );
        assert!(
            setup.contains("repaint_tray_from_state(&app_handle, \"profile\")"),
            "the click handler must repaint after the write so the checkmark moves"
        );
        // Wired into the menu build.
        let rebuild = body_of(prod, "fn rebuild_tray_menu(");
        assert!(
            rebuild.contains("build_profile_submenu(app, s, &profiles, active.as_deref())"),
            "rebuild_tray_menu must build the profile submenu"
        );
        assert!(
            rebuild.contains("&profile_submenu,"),
            "rebuild_tray_menu must add the profile submenu to the main menu"
        );
    }
    /// A deadline that expired while the app was closed is cleared from
    /// `config.json` at startup, so the clamp log line cannot repeat on every
    /// launch and the persisted document matches what the app honours.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the startup-cleanup wiring — that setup runs the expired-deadline cleanup before the first paint — setup_tray needs a live Tauri App; the order is pinned textually; scoped to its own
    /// fn body so prose elsewhere cannot satisfy it.
    #[test]
    fn startup_cleanup_clears_an_expired_deadline_through_the_guarded_writer() {
        let prod = tray_prod_source();
        let body = body_of(prod, "fn clear_expired_snooze_at_startup(");
        assert!(
            body.contains("crate::config::snooze_expired_deadline(cfg"),
            "the cleaner must ask the shared predicate whether the deadline is dead"
        );
        assert!(
            body.contains("store_snooze(app, None)"),
            "the cleaner must go through the guarded writer, not write the file itself"
        );
        assert!(
            body.contains("cleared the expired deadline left by the previous session"),
            "the cleanup must be logged, and with its own copy"
        );
        // Wired into startup: a cleaner nothing calls would leave the stale
        // deadline on disk forever.
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains("clear_expired_snooze_at_startup(app.handle())"),
            "setup_tray must run the cleanup once, before the first menu build"
        );
        let cleanup = setup
            .find("clear_expired_snooze_at_startup(")
            .expect("call site");
        let menu = setup.find("update_tray_menu_startup(").expect("tray paint");
        assert!(
            cleanup < menu,
            "the cleanup must precede the first menu build, so the tray never \
             renders a state the config no longer holds"
        );
    }
}
