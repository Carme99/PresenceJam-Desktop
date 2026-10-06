//! tray/actions.rs — split from tray/mod.rs (#756).

#![allow(unused_imports)]

use super::*;
use crate::events::PlaybackStateChanged;
use crate::i18n::Strings;
use crate::spotify::RepeatState;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Module-level mutex that serialises the two writers to the tray
/// (polling thread and frontend command). Issue #71.
pub static TRAY_WRITE_LOCK: std::sync::OnceLock<parking_lot::Mutex<()>> =
    std::sync::OnceLock::new();

pub fn tray_write_lock() -> &'static parking_lot::Mutex<()> {
    TRAY_WRITE_LOCK.get_or_init(|| parking_lot::Mutex::new(()))
}

/// Coalescing guard for the delayed one-shot refresh kicked after a
/// successful tray player action: rapid next/previous clicks must not pile
/// up unbounded 2 s-sleep threads each firing blocking Spotify+Teams HTTP.
/// First claimant spawns; losers skip (their track change is covered by the
/// in-flight refresh's unconditional GET plus the polling loop).
pub static DELAYED_REFRESH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Runs a Spotify player action from a tray click using the stored access
/// token. On success the tray menu is force-refreshed and the action's
/// deterministic outcome is recorded for the items that mirror it:
/// `resulting_playing` for the Play/Pause item (`None` for actions that do
/// not change the playing state), `resulting_modes` for the shuffle/repeat
/// toggles (issue #582). Both are applied BEFORE `force_tray_refresh`, so the
/// rebuild renders the state the API just accepted; recording them in the
/// caller after this function returned would repaint the replaced state and
/// the tray's dedup key would not change again until the next track ends. On
/// failure the error is logged and emitted on the `playback-error` event so
/// the frontend can surface it; a `NoActiveDevice` error (404) is logged
/// distinctly — the Devices submenu offers transfer in that case. Returns
/// `true` when the action succeeded.
/// See issues #3.0-P3 and #582.
pub fn run_player_action(
    app: &AppHandle,
    label: &str,
    resulting_playing: Option<bool>,
    resulting_modes: Option<(bool, RepeatState)>,
    action: impl Fn(&str) -> Result<(), crate::spotify::SpotifyApiError>,
) -> bool {
    // Issue #586: route the tray's player actions through the shared
    // refresh-aware policy instead of a raw `state.tokens.spotify()`
    // snapshot — a stale access token is refreshed proactively, and an
    // `ExpiredToken` response gets one refresh + retry, exactly as the
    // command layer does.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    match crate::commands::playback::player_with_refresh_typed(state.inner(), app, label, action) {
        Ok(()) => {
            log::info!("[TRAY] {}: success", label);
            if let Some(playing) = resulting_playing {
                note_playing_state(playing);
            }
            if let Some((shuffle, repeat)) = resulting_modes {
                note_playback_modes(shuffle, repeat);
            }
            force_tray_refresh_from_app(app);
            // Immediate Teams catch-up after a successful player action:
            // wait 2 s for Spotify's currently-playing to catch up after
            // a skip, then run a one-shot poll (no-op when sync is off).
            // Coalesced: rapid clicks skip while a delayed refresh is
            // already pending; its unconditional GET covers their tracks.
            if DELAYED_REFRESH_IN_FLIGHT
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let app_clone = app.clone();
                let label_owned = label.to_string();
                std::thread::spawn(move || {
                    // RAII: a panic in run_oneshot must not wedge future
                    // refreshes (a manual clear on each return path would).
                    struct ResetOnDrop;
                    impl Drop for ResetOnDrop {
                        fn drop(&mut self) {
                            DELAYED_REFRESH_IN_FLIGHT.store(false, Ordering::Release);
                        }
                    }
                    let _reset = ResetOnDrop;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let state = app_clone.state::<std::sync::Arc<crate::AppState>>();
                    if !state.polling.is_syncing() {
                        log::debug!("[TRAY] {}: delayed refresh skipped (sync off)", label_owned);
                        return;
                    }
                    crate::polling::run_oneshot(state.inner(), &app_clone);
                    let is_syncing = state.polling.is_syncing();
                    let current_track = state.polling.current_track().clone();
                    if let Err(e) = update_tray_menu(&app_clone, is_syncing, current_track) {
                        log::warn!(
                            "[TRAY] {}: delayed refresh tray update failed: {}",
                            label_owned,
                            e
                        );
                    }
                });
            } else {
                log::debug!(
                    "[TRAY] {}: delayed refresh already pending, coalesced",
                    label
                );
            }
            true
        }
        Err(crate::spotify::SpotifyApiError::NoActiveDevice) => {
            log::warn!(
                "[TRAY] {}: no active device — pick one from the Devices menu",
                label
            );
            let _ = app.emit(
                "playback-error",
                "No active playback device - pick one from the tray Devices menu",
            );
            false
        }
        Err(e) => {
            log::error!("[TRAY] {}: failed: {}", label, e);
            let _ = app.emit("playback-error", e.to_string());
            false
        }
    }
}

/// Upper bound on the wait for the frontend's Pause/Resume toggle to land
/// (issue #588). The frontend owns the actual `start_syncing` /
/// `stop_syncing` call, so the flag settles only after that round-trip.
pub const TOGGLE_SETTLE_TIMEOUT: Duration = Duration::from_secs(3);

/// Poll cadence while waiting for the toggle to land.
pub const TOGGLE_SETTLE_POLL: Duration = Duration::from_millis(150);

/// Repaints the tray from authoritative backend state on the CURRENT
/// thread. Callers must already be off the menu/app-event thread — the
/// rebuild performs blocking Spotify HTTP whenever the fetch throttle has
/// lapsed. `context` only labels the failure log.
pub fn repaint_tray_from_state(app: &AppHandle, context: &str) {
    let Some(state) = app.try_state::<std::sync::Arc<crate::AppState>>() else {
        log::debug!(
            "[TRAY] {}: AppState not registered yet, skipping repaint",
            context
        );
        return;
    };
    let is_syncing = state.polling.is_syncing();
    let current_track = state.polling.current_track().clone();
    if let Err(e) = update_tray_menu(app, is_syncing, current_track) {
        log::warn!("[TRAY] {}: tray repaint failed: {}", context, e);
    }
}

/// Repaints the tray from backend state on a worker thread.
///
/// Issues #587/#592: every caller runs on a menu/app-event thread (tray
/// click arms, app-menu items, the single-instance raise), and
/// `update_tray_menu` fetches Spotify devices/queue with a 10 s timeout
/// whenever the 60 s throttle has lapsed — doing that inline wedges the
/// native menu (the freeze issue #386 removed from the player arms).
pub fn refresh_tray_from_state(app: &AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || repaint_tray_from_state(&app_handle, "refresh_tray_from_state"));
}

/// Repaints the tray after a locale change (issue #674).
///
/// A language switch changes only labels, so the dedup snapshot would
/// otherwise hide it — `force_tray_refresh` nudges that snapshot and rebuilds
/// from the freshly installed table. It runs on a worker thread for the same
/// reason as [`refresh_tray_from_state`]: the rebuild may perform blocking
/// Spotify HTTP, which must never run on a menu/app-event thread.
pub fn refresh_tray_for_locale(app: &AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || force_tray_refresh_from_app(&app_handle));
}

/// Waits — bounded by [`TOGGLE_SETTLE_TIMEOUT`] — for the frontend's
/// Pause/Resume toggle to move the running flag away from `before`.
/// Returns `true` when it moved, `false` when the window elapsed or state
/// was unavailable; the caller repaints either way.
pub fn await_sync_toggle(app: &AppHandle, before: bool) -> bool {
    let deadline = Instant::now() + TOGGLE_SETTLE_TIMEOUT;
    while Instant::now() < deadline {
        std::thread::sleep(TOGGLE_SETTLE_POLL);
        let Some(state) = app.try_state::<std::sync::Arc<crate::AppState>>() else {
            return false;
        };
        if state.polling.is_syncing() != before {
            return true;
        }
    }
    false
}

/// Production entry point for [`force_tray_refresh`]: binds the managed-state
/// lookup and the tray rebuild to a live `AppHandle`.
///
/// `try_state` — not `state` — is the lookup: `state()` panics with "state()
/// called before manage()", so a lookup that can run before `app.manage()`
/// must not use it. See the seam's doc comment for whether this function's
/// current callers can (they cannot — this is defence-in-depth).
pub fn force_tray_refresh_from_app(app: &AppHandle) {
    force_tray_refresh(
        &|| {
            app.try_state::<std::sync::Arc<crate::AppState>>()
                .map(|state| state.inner().clone())
        },
        &mut |is_syncing, current_track| {
            let _ = update_tray_menu(app, is_syncing, current_track);
        },
    );
}

/// Forces the next `update_tray_menu` call to rebuild and asks it for fresh
/// Devices/Up Next lists: records the action (issue #883 — the caches stay, and
/// the rebuild re-fetches under `TRAY_POST_ACTION_FETCH_MIN`), nudges the dedup
/// snapshot so the rebuild can't early-return, then rebuilds immediately.
/// User-initiated tray actions call this so the menu reflects the new
/// playback/device state right away — the dedup key alone wouldn't change on
/// e.g. a pause or a transfer.
///
/// The snapshot is nudged (not cleared) with the *current* track key so the
/// re-seed logic in `update_tray_menu` (which only fires on a genuine track
/// change) doesn't clobber the toggle state set by the action. See
/// issue #3.0-P3.
///
/// Issue #937 asked for this path to tolerate a second launch arriving before
/// `app.manage(state.clone())` has run in the setup closure. As it turns out
/// this function has no such caller, and it did not crash before this change:
/// its only two production callers — [`run_player_action`] (tray menu events)
/// and [`refresh_tray_for_locale`] (the `save_config` IPC) — both run after
/// setup has managed the state. The forwarded-second-launch path reaches
/// [`refresh_tray_from_state`] → `repaint_tray_from_state`, which has used
/// `try_state` on `main` already. So this is defence-in-depth, not a fix for
/// an observed crash: the lookup cannot panic, and no future caller has to
/// remember that it cannot.
///
/// The managed-state lookup is a parameter rather than an inline
/// `app.try_state()` so the unit test can drive both arms — a lookup that
/// reports "not managed yet" (`None`) and one that returns a managed state —
/// without a GUI runtime. That is also why `managed_state` returning `None`
/// is the arm under test: it is exactly what a pre-`manage()` caller would
/// see. The skip is logged at `warn!`, not `debug!`: the default
/// tauri-plugin-log level filters `debug!`, so a `debug!` line would be
/// invisible to anyone actually debugging a real skip.
///
/// The binder in [`force_tray_refresh_from_app`] is deliberately UNPINNED:
/// nothing in the test suite constrains it to `try_state` rather than
/// `state()`, because Tauri offers no hermetic `AppHandle` constructor
/// outside the `test` feature that #937's rework removed (that feature is
/// what breaks the Windows test binary's loader). A revert of the binder
/// would restore the panic at the call, with no test failing — acceptable
/// only because no caller reaches this function before `manage()`.
///
/// The seams are `&dyn Fn` rather than generic parameters so the #883/#886
/// source guards in this module keep resolving to this body, and so the
/// regression test drives this function itself instead of a stand-in.
pub fn force_tray_refresh(
    managed_state: &dyn Fn() -> Option<std::sync::Arc<crate::AppState>>,
    rebuild: &mut dyn FnMut(bool, Option<crate::spotify::TrackInfo>),
) {
    let Some(state) = managed_state() else {
        log::warn!(
            "[TRAY] force_tray_refresh: AppState not managed yet — skipping the forced \
             tray rebuild (issue #937)"
        );
        return;
    };
    let is_syncing = state.polling.is_syncing();
    let current_track = state.polling.current_track().clone();
    // S9 (issue #677): the snooze is read here only for the dedup key below —
    // while one is active the rebuild renders the CACHED Devices/Up Next lists
    // instead of fetching (`TrayFetch::CacheOnly`) and an action cannot override
    // that. The nudge below still forces the repaint, which is all a snooze-time
    // repaint needs.
    let snooze = snooze_from_app_state(&state);
    // Issue #883: the caches stay. Emptying them was how this helper asked for
    // fresh Devices/Up Next lists, and it bypassed `TRAY_SPOTIFY_FETCH_THROTTLE`
    // on every player action; the action is recorded instead, and the rebuild
    // re-fetches under `TRAY_POST_ACTION_FETCH_MIN` — which coalesces a burst
    // into one pair of requests. The nudge below still forces the repaint.
    *LAST_TRAY_ACTION.lock() = Some(Instant::now());
    let snooze_key = snooze.as_ref().map(|sn| snooze_dedup_key(&sn.status));
    // Nudge the dedup snapshot (not clear it) with the *current* track key so
    // the rebuild below can't early-return while the re-seed logic stays
    // inert: a cleared snapshot would look like a genuine track change and
    // clobber the toggle state the action just recorded. Flipping the sync
    // bit is enough — the real snapshot is committed by that rebuild.
    let (devices_bucket, queue_bucket) = cache_buckets();
    // Issue #869: the nudge snapshot's profile key reads the post-action
    // value so the rebuild the function triggers sees a fresh dedup key —
    // the real write happens inside `store_active_profile`.
    // Issue #937: this used to be a second `app.state::<>()` call on the same
    // guard-clause risk. The single lookup above is already in hand, so the
    // key is read from it — one lookup, and this site can no longer panic.
    let nudge_profile_key = state
        .config
        .get()
        .as_ref()
        .and_then(|c| c.active_profile.clone());
    let mut nudge = tray_snapshot_for(
        is_syncing,
        false,
        current_track.as_ref(),
        snooze_key,
        devices_bucket,
        queue_bucket,
        nudge_profile_key,
    );
    nudge.is_syncing = !is_syncing;
    *last_tray_state().lock() = Some(nudge);
    rebuild(is_syncing, current_track);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{DE, EN, FR};
    use crate::tray::testkit::{body_of, prod_source, tray_prod_source};
    /// Issue #586: tray player actions used to snapshot
    /// `state.tokens.spotify()` — an expired access token meant a failed
    /// click with no refresh and no retry, while the refresh-aware policy in
    /// `commands/playback.rs` had no callers. Both tray paths must now route
    /// through that shared policy.
    #[test]
    fn tray_player_actions_use_refresh_aware_token() {
        let src = tray_prod_source();
        let prod = prod_source(src);
        let action_body = body_of(prod, "fn run_player_action(");
        assert!(
            action_body.contains("player_with_refresh_typed("),
            "run_player_action must route through the shared refresh-aware policy (issue #586)"
        );
        assert!(
            !action_body.contains("tokens.spotify()"),
            "run_player_action must not snapshot the raw access token (issue #586)"
        );
        let setup_body = body_of(prod, "pub fn handle_menu_event(");
        let play_pos = setup_body
            .find("ID_PLAY_PAUSE =>")
            .expect("handle_menu_event must handle ID_PLAY_PAUSE");
        let play_end = setup_body[play_pos..]
            .find("ID_PREVIOUS =>")
            .map(|i| play_pos + i)
            .unwrap_or(setup_body.len());
        let play_arm = &setup_body[play_pos..play_end];
        assert!(
            play_arm.contains("player_with_refresh_typed("),
            "the Play/Pause arm must read playback state through the refresh-aware policy (issue #586)"
        );
        assert!(
            !play_arm.contains("tokens.spotify()"),
            "the Play/Pause arm must not snapshot the raw access token (issue #586)"
        );
    }
    /// Issue #591: the tray must state whether the app is syncing instead of
    /// leaving it to be inferred from the Pause/Resume verb. Issue #674: the
    /// status word comes from the table the caller hands in, so the same
    /// helper renders German/French without a second code path.
    #[test]
    fn sync_status_line_reports_backend_state_in_every_locale() {
        let track = crate::spotify::TrackInfo {
            title: "Track".to_string(),
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
        assert_eq!(
            sync_status_line(&EN, false, true, Some(&track)),
            "Not syncing",
            "a stopped poller outranks any remembered track"
        );
        assert_eq!(
            sync_status_line(&EN, false, false, None),
            "Not syncing",
            "no track and no sync is still Not syncing"
        );
        assert_eq!(
            sync_status_line(&EN, true, false, None),
            "Syncing — no track",
            "syncing with nothing playing must say so rather than claim a track"
        );
        assert_eq!(
            sync_status_line(&EN, true, true, Some(&track)),
            "Syncing — Artist — Track"
        );
        assert_eq!(
            sync_status_line(&EN, true, false, Some(&track)),
            "Paused — Artist — Track",
            "a same-track pause is reported from the live playing state"
        );

        // Issue #674: the selected table is what the user reads — Deutsch and
        // Français must not fall back to the English status words.
        assert_eq!(
            sync_status_line(&DE, true, true, Some(&track)),
            format!("{} — Artist — Track", DE.status_syncing)
        );
        assert_eq!(
            sync_status_line(&DE, true, false, Some(&track)),
            format!("{} — Artist — Track", DE.status_paused)
        );
        assert_eq!(
            sync_status_line(&DE, false, false, None),
            DE.status_not_syncing
        );
        assert_eq!(
            sync_status_line(&FR, true, false, None),
            FR.status_syncing_no_track
        );
    }
    /// Issue #883: every tray player action used to empty both throttled caches,
    /// so each click paid two fresh Spotify fetches and the 60 s throttle that
    /// protects the API was bypassed entirely. The action is now recorded and the
    /// rebuild re-fetches under a short minimum interval, which a burst of clicks
    /// shares instead of multiplying.
    #[test]
    fn post_action_refresh_coalesces_a_burst_of_clicks() {
        let min = TRAY_POST_ACTION_FETCH_MIN;
        let t0 = Instant::now();
        let click = t0 + Duration::from_millis(10);

        // No action: the ordinary throttle decides.
        assert!(!action_fetch_due(None, None, t0, min));
        // An action nothing has fetched for yet is due immediately, and stays
        // due until a fetch is issued on its behalf.
        assert!(action_fetch_due(Some(t0), None, t0, min));
        assert!(action_fetch_due(Some(click), None, click, min));

        // The pair of requests that action asked for, recorded as it is issued.
        let fetched = click + Duration::from_millis(1);
        assert!(
            !action_fetch_due(Some(click), Some(fetched), fetched, min),
            "the fetch already in flight covers the action that asked for it"
        );

        // Ten rapid clicks inside the interval share that single pair.
        let mut pairs = 1;
        for n in 1..10u64 {
            let at = t0 + Duration::from_millis(n * 300);
            if action_fetch_due(Some(at), Some(fetched), at + Duration::from_millis(1), min) {
                pairs += 1;
            }
        }
        assert_eq!(
            pairs, 1,
            "ten clicks inside the interval must share one devices+queue pair (issue #883)"
        );

        // A click that lands after the interval is worth a fresh pair…
        assert!(action_fetch_due(
            Some(t0 + Duration::from_secs(30)),
            Some(fetched),
            fetched + min,
            min
        ));
        // …while a fetch that already followed the action is not repeated.
        assert!(!action_fetch_due(
            Some(click),
            Some(fetched),
            fetched + min,
            min
        ));

        // The wiring: the caches must no longer be emptied on a player action,
        // and the rebuild must pass the action's fetch mode to both submenu
        // sources (it marks the fetch in flight before issuing it).
        let prod = tray_prod_source();
        let force = body_of(prod, "fn force_tray_refresh(");
        assert!(
            force.contains("LAST_TRAY_ACTION.lock() = Some(Instant::now())"),
            "a player action must be recorded, not enforced by emptying the caches"
        );
        assert!(
            !force.contains("DEVICES_CACHE.lock() = None")
                && !force.contains("QUEUE_CACHE.lock() = None"),
            "the caches must stay: emptying them bypassed the fetch throttle (issue #883)"
        );
        let body = body_of(prod, "fn rebuild_tray_menu(");
        let mark = body
            .find("LAST_ACTION_FETCH.lock() = Some(Instant::now())")
            .expect("a post-action rebuild must mark its fetch in flight");
        let devices = body
            .find("devices_for_menu(access_token.as_deref(), fetch)")
            .expect("the rebuild must build the devices submenu from the fetch mode");
        assert!(
            mark < devices,
            "the in-flight mark must be recorded before the requests run (issue #883)"
        );
    }
    /// Issue #937: `force_tray_refresh` used to look the managed state up with
    /// `app.state::<Arc<AppState>>()`, which panics with "state() called before
    /// manage()" when the lookup runs before the setup closure has run
    /// `app.manage(state.clone())`.
    ///
    /// To be accurate about what this pins: no current caller can produce
    /// that window. `force_tray_refresh` is reached from `run_player_action`
    /// (tray menu events) and `refresh_tray_for_locale` (the `save_config`
    /// IPC), both downstream of `manage()`; the forwarded-second-launch path
    /// goes through `refresh_tray_from_state` → `repaint_tray_from_state`,
    /// which already used `try_state` on `main`. The change is
    /// defence-in-depth, and this test is its regression cover.
    ///
    /// The function takes the managed-state lookup and the tray rebuild as
    /// seams, so both arms are driven here directly: a lookup reporting "not
    /// managed yet" (`None` — what a pre-`manage()` caller would see) must
    /// return without touching the tray, and a managed state must rebuild
    /// exactly once, from the values it read.
    #[test]
    fn force_tray_refresh_tolerates_missing_state_issue_937() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        use std::sync::Arc;

        // Arm 1: an unmanaged state. A lookup that returns `None` is exactly
        // what `try_state` reports before `app.manage()` has run.
        let mut rebuilds: Vec<(bool, Option<crate::spotify::TrackInfo>)> = Vec::new();
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            force_tray_refresh(&|| None, &mut |is_syncing, track| {
                rebuilds.push((is_syncing, track));
            });
        }));
        assert!(
            unwound.is_ok(),
            "force_tray_refresh must survive an unmanaged AppState (issue #937): \
             pre-fix this path called app.state::<Arc<AppState>>(), which panics \
             with 'state() called before manage()' and would take the tray worker \
             thread down with it"
        );
        assert!(
            rebuilds.is_empty(),
            "with no managed AppState there is nothing to repaint from, so the tray \
             must not be rebuilt (issue #937) — got {} rebuild(s)",
            rebuilds.len()
        );

        // Arm 2: the normal launch. A managed state proceeds, and the rebuild
        // sees the sync flag the function read out of it.
        let state = Arc::new(crate::AppState::new());
        state.polling.set_syncing(true);
        let mut rebuilds: Vec<(bool, Option<crate::spotify::TrackInfo>)> = Vec::new();
        force_tray_refresh(&|| Some(state.clone()), &mut |is_syncing, track| {
            rebuilds.push((is_syncing, track));
        });
        assert_eq!(
            rebuilds.len(),
            1,
            "a managed AppState must produce exactly one forced rebuild (issue #937)"
        );
        assert!(
            rebuilds[0].0,
            "the rebuild must be driven by the managed state's sync flag, not a \
             default (issue #937)"
        );
    }
}
