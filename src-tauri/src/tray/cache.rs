//! tray/cache.rs — split from tray/mod.rs (#756).

#![allow(unused_imports)]

use super::*;
use crate::events::PlaybackStateChanged;
use crate::i18n::Strings;
use crate::spotify::RepeatState;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// The throttle bucket a cache slot is in (issue #805): how many full throttle
/// windows have elapsed since it was filled, `0` while it is empty.
///
/// Pure in its timestamp, so "an unchanged rebuild inside the window is still
/// a no-op, one window later it repaints" is asserted without sleeping.
pub fn throttle_bucket(fetched_at: Option<Instant>, throttle: Duration) -> u64 {
    match fetched_at {
        None => 0,
        Some(at) => at.elapsed().as_secs() / throttle.as_secs().max(1),
    }
}

/// The throttle buckets of both caches (issue #805), read under short locks —
/// no HTTP, and no lock held past the read.
pub fn cache_buckets(caches: &crate::state::AppCaches) -> (u64, u64) {
    let devices_at = caches.devices_slot().lock().as_ref().map(|(at, _)| *at);
    let queue_at = caches.queue_slot().lock().as_ref().map(|(at, _)| *at);
    (
        throttle_bucket(devices_at, TRAY_SPOTIFY_FETCH_THROTTLE),
        throttle_bucket(queue_at, TRAY_SPOTIFY_FETCH_THROTTLE),
    )
}

/// Which entry point a rebuild came through (issue #882).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayPaint {
    /// An ordinary rebuild: the dedup key decides whether the menu is rebuilt,
    /// and a successful rebuild commits the snapshot.
    Deduped,
    /// The startup paint. Built on the main thread before the event loop runs:
    /// it renders the throttled caches without touching the network, and it
    /// does not commit the snapshot (see `update_tray_menu_startup`).
    Startup,
}

/// The fetch mode a rebuild runs under (issues #677, #882).
///
/// Pure, so "the startup paint performs no request" and "a snooze performs no
/// request" are asserted directly rather than through the rebuild's source.
pub fn paint_fetch_mode(paint: TrayPaint, snoozed: bool, action_refresh_due: bool) -> TrayFetch {
    match paint {
        // Issue #882: no network before the event loop exists.
        TrayPaint::Startup => TrayFetch::CacheOnly,
        TrayPaint::Deduped => {
            let base = tray_fetch_mode(snoozed);
            // A snooze outranks an action: "no Spotify request while snoozed" is
            // that feature's acceptance criterion (issue #677).
            if base == TrayFetch::CacheOnly {
                TrayFetch::CacheOnly
            } else if action_refresh_due {
                // Issue #883: the action asks for fresh Devices/Up Next lists.
                TrayFetch::RefreshNow
            } else {
                base
            }
        }
    }
}

/// Throttle window for the tray's Spotify devices/queue fetches
/// (issue #3.0-P3). The polling loop calls `update_tray_menu` on every
/// successful poll; without a cap the Devices/Up Next submenus would
/// hammer the Spotify API on every iteration. Fetched data is cached
/// here and re-used until the window lapses.
pub const TRAY_SPOTIFY_FETCH_THROTTLE: Duration = Duration::from_secs(60);

/// Cache slot for the throttled devices fetch: `(fetched_at, devices)`.
/// Fast path for the device submenu's click dispatch, which resolves the
/// stable `{ID_DEVICES}|{device id}` against this list and falls back to a
/// live re-fetch when stale (issue #388).
pub type DeviceCacheSlot = Option<(Instant, Vec<crate::spotify::DeviceInfo>)>;

/// Shortest gap between the post-action re-fetches of the Devices/Up Next lists
/// (issue #883). A player action wants those submenus to mirror what just
/// happened, but a burst of clicks must share one devices+queue pair: five
/// seconds is long enough to coalesce a burst, short enough that the menu still
/// matches the click the user just made.
pub const TRAY_POST_ACTION_FETCH_MIN: Duration = Duration::from_secs(5);

/// Whether a rebuild must re-fetch the Devices/Up Next lists on behalf of a
/// player action (issue #883). Pure in its instants, so the coalescing rule —
/// ten rapid clicks, one pair of requests — is unit-testable without waiting.
pub fn action_fetch_due(
    last_action: Option<Instant>,
    last_action_fetch: Option<Instant>,
    now: Instant,
    min_interval: Duration,
) -> bool {
    match (last_action, last_action_fetch) {
        // No action has asked for anything.
        (None, _) => false,
        // An action nothing has fetched for yet is due immediately.
        (Some(_), None) => true,
        // Only an action newer than the last post-action fetch — and far enough
        // after it — is worth two more requests; a burst inside `min_interval`
        // shares the pair already issued.
        (Some(action), Some(fetched)) => {
            action > fetched && now.duration_since(fetched) >= min_interval
        }
    }
}

/// Returns cached devices when the throttle window hasn't elapsed, else
/// fetches fresh ones. On a fetch failure the stale cache is returned so
/// the submenu doesn't flicker to "(no devices)" on a transient error.
///
/// The cache mutex is held only to clone the snapshot and to store the
/// fresh result — the HTTP fetch runs outside any lock so a cold/stale
/// cache never blocks tray interactions. If two threads race on a stale
/// cache both will fetch; the last writer wins. This benign double-fetch
/// wastes one request but cannot corrupt state. See issue #217.
/// `min_interval` is this fetch's throttle (issue #883): the full window for an
/// ordinary rebuild, `Duration::ZERO` for the one a player action asks for.
pub fn cached_devices(
    caches: &crate::state::AppCaches,
    access_token: &str,
    min_interval: Duration,
) -> Vec<crate::spotify::DeviceInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = caches.devices_slot().lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= min_interval,
        None => true,
    };
    if !needs_fetch {
        return snapshot.unwrap().1;
    }
    // Throttled fetch OUTSIDE any lock — never hold the devices slot across HTTP.
    match crate::spotify::get_devices(access_token) {
        Ok(devices) => {
            // Re-acquire only to store the fresh result.
            *caches.devices_slot().lock() = Some((Instant::now(), devices.clone()));
            devices
        }
        Err(e) => {
            log::warn!("[TRAY] cached_devices: failed to fetch devices: {}", e);
            snapshot.map(|(_, devices)| devices).unwrap_or_default()
        }
    }
}

/// Returns cached queue when the throttle window hasn't elapsed, else
/// fetches fresh. Falls back to the stale cache on failure.
///
/// Same lock discipline as `cached_devices`: snapshot, drop, fetch outside
/// lock, re-acquire to store. Benign double-fetch on a race. See issue #217.
pub fn cached_queue(
    caches: &crate::state::AppCaches,
    access_token: &str,
    min_interval: Duration,
) -> Option<crate::spotify::QueueInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = caches.queue_slot().lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= min_interval,
        None => true,
    };
    if !needs_fetch {
        return snapshot.map(|(_, queue)| queue);
    }
    // Throttled fetch OUTSIDE any lock — never hold the queue slot across HTTP.
    match crate::spotify::get_queue(access_token) {
        Ok(queue) => {
            // Re-acquire only to store.
            *caches.queue_slot().lock() = Some((Instant::now(), queue.clone()));
            Some(queue)
        }
        Err(e) => {
            log::warn!("[TRAY] cached_queue: failed to fetch queue: {}", e);
            snapshot.map(|(_, queue)| queue)
        }
    }
}

/// Where a rebuild may take its Devices / Up Next lists from (4.7.0, S9 /
/// issue #677).
///
/// A snooze means "the user asked for no activity", and that covers the app's
/// own background requests: the acceptance for the feature is that a snoozed
/// session produces no Spotify GET at all. `update_tray_menu` is called from
/// the polling driver and from the snooze click arm, so the fetch mode has to
/// be a property of the rebuild rather than of its caller — otherwise a
/// countdown repaint would silently re-fetch devices every 60 s (the throttle
/// window) and break that guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayFetch {
    /// Normal rebuild: reuse the throttled cache, fetching when it lapsed.
    Refresh,
    /// Issue #883: a player action just changed the playback/device state these
    /// lists mirror, so this one rebuild bypasses the throttle. A burst of
    /// clicks still shares one pair of requests — see [`action_fetch_due`].
    RefreshNow,
    /// A snooze is active: render the last cached lists (or nothing) and issue
    /// no request. The submenus go stale by design until the snooze ends.
    CacheOnly,
}

/// The fetch mode a rebuild must use (4.7.0, S9 / issue #677).
///
/// Pure, so the "a snooze means no Spotify request" decision is asserted
/// directly instead of only through the rebuild's source text.
pub fn tray_fetch_mode(snoozed: bool) -> TrayFetch {
    if snoozed {
        TrayFetch::CacheOnly
    } else {
        TrayFetch::Refresh
    }
}

/// The Devices list for a rebuild, honouring [`TrayFetch`]. A missing access
/// token is an empty submenu either way — there is nothing to fetch with.
pub fn devices_for_menu(
    caches: &crate::state::AppCaches,
    access_token: Option<&str>,
    fetch: TrayFetch,
) -> Vec<crate::spotify::DeviceInfo> {
    match (access_token, fetch) {
        (Some(token), TrayFetch::Refresh) => {
            cached_devices(caches, token, TRAY_SPOTIFY_FETCH_THROTTLE)
        }
        (Some(token), TrayFetch::RefreshNow) => cached_devices(caches, token, Duration::ZERO),
        (_, TrayFetch::CacheOnly) => caches
            .devices_slot()
            .lock()
            .clone()
            .map(|(_, devices)| devices)
            .unwrap_or_default(),
        (None, _) => Vec::new(),
    }
}

/// The Up Next snapshot for a rebuild, honouring [`TrayFetch`]. Same contract
/// as [`devices_for_menu`].
pub fn queue_for_menu(
    caches: &crate::state::AppCaches,
    access_token: Option<&str>,
    fetch: TrayFetch,
) -> Option<crate::spotify::QueueInfo> {
    match (access_token, fetch) {
        (Some(token), TrayFetch::Refresh) => {
            cached_queue(caches, token, TRAY_SPOTIFY_FETCH_THROTTLE)
        }
        (Some(token), TrayFetch::RefreshNow) => cached_queue(caches, token, Duration::ZERO),
        (_, TrayFetch::CacheOnly) => caches.queue_slot().lock().clone().map(|(_, queue)| queue),
        (None, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{DE, EN, FR};
    /// Issue #582: a check mark can only carry on/off, while `repeat_state`
    /// has three documented values — so the label must name the mode, or a
    /// user cannot tell "repeat one" from "repeat the playlist". Issue #674:
    /// the label is read from the installed table.
    #[test]
    fn repeat_menu_label_spells_out_the_mode() {
        assert_eq!(repeat_menu_label(&EN, RepeatState::Off), "Repeat: Off");
        assert_eq!(
            repeat_menu_label(&EN, RepeatState::Context),
            "Repeat: Context"
        );
        assert_eq!(repeat_menu_label(&EN, RepeatState::Track), "Repeat: Track");

        assert_eq!(repeat_menu_label(&DE, RepeatState::Off), "Wiederholen: Aus");
        assert_eq!(
            repeat_menu_label(&DE, RepeatState::Context),
            "Wiederholen: Kontext"
        );
        assert_eq!(
            repeat_menu_label(&DE, RepeatState::Track),
            "Wiederholen: Titel"
        );
        assert_eq!(
            repeat_menu_label(&FR, RepeatState::Track),
            "Répéter : titre"
        );
    }
    /// Issue #805: the dedup key carries the throttle bucket of both caches.
    /// Without it the early return also skipped the devices/queue fetches, so
    /// a session where nothing else moved showed whatever the last rebuild had
    /// rendered — possibly hours old, listing devices that had long since
    /// disconnected. The bucket makes a quiet session repaint exactly once per
    /// throttle window while every rebuild inside the window still dedupes.
    #[test]
    fn stale_caches_force_a_tray_rebuild_once_per_throttle_window() {
        // The bucket is a pure function of the cache's age, so the window
        // boundary is asserted without sleeping.
        assert_eq!(
            throttle_bucket(None, TRAY_SPOTIFY_FETCH_THROTTLE),
            0,
            "an empty cache must not force a rebuild on its own"
        );
        assert_eq!(
            throttle_bucket(Some(Instant::now()), TRAY_SPOTIFY_FETCH_THROTTLE),
            0
        );
        assert_eq!(
            throttle_bucket(
                Some(Instant::now() - TRAY_SPOTIFY_FETCH_THROTTLE),
                TRAY_SPOTIFY_FETCH_THROTTLE
            ),
            1,
            "a cache exactly one window old is due for a re-fetch"
        );
        assert_eq!(
            throttle_bucket(
                Some(Instant::now() - 10 * TRAY_SPOTIFY_FETCH_THROTTLE),
                TRAY_SPOTIFY_FETCH_THROTTLE
            ),
            10
        );

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
        let at = |devices: u64, queue: u64| {
            tray_snapshot_for(
                true,
                true,
                Some(&track),
                None,
                devices,
                queue,
                None,
                caches.shuffle_flag().load(Ordering::Acquire),
                last_repeat_state(&caches),
            )
        };

        // Identical track, window, modes and caches: still a no-op.
        assert!(
            !tray_state_changed(Some(&at(0, 0)), &at(0, 0)),
            "a rebuild inside the throttle window must still dedupe"
        );
        // One window later either cache is due, and that must repaint — the
        // repaint is what re-runs the fetchers.
        assert!(
            tray_state_changed(Some(&at(0, 0)), &at(1, 0)),
            "a devices cache one throttle window old must repaint (issue #805)"
        );
        assert!(
            tray_state_changed(Some(&at(0, 0)), &at(0, 1)),
            "a queue cache one throttle window old must repaint (issue #805)"
        );
        assert!(
            !tray_state_changed(Some(&at(1, 1)), &at(1, 1)),
            "the bucket alone must not make every rebuild a repaint"
        );
    }
}
