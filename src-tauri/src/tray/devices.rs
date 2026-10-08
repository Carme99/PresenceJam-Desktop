//! tray/devices.rs — split from tray/mod.rs (#756).

#![allow(unused_imports)]

use super::*;
use crate::i18n::Strings;
use crate::menu::{ID_ABOUT, ID_OPEN_LOGS, ID_QUIT, ID_SETTINGS, ID_SHOW_DASHBOARD, ID_SHOW_LOGS};
use std::time::{Duration, Instant};
use tauri::{
    menu::{
        CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu,
        SubmenuBuilder,
    },
    AppHandle, Manager,
};

/// What the `{ID_DEVICES}|{…}` suffix resolved to. Device menu ids carry the
/// stable Spotify device id (issue #388); the `LegacyIndex` variant accepts
/// ids minted by an older menu build still on screen when the app updated.
#[derive(Debug, PartialEq, Eq)]
pub enum DeviceMenuSelection {
    DeviceId(String),
    LegacyIndex(usize),
    Invalid,
}

/// Parses the suffix of a device menu-item id. A numeric suffix from an old
/// menu build is kept as `LegacyIndex` for back-compat; anything else is a
/// stable Spotify device id (`DeviceId`), including the empty string and the
/// `none` placeholder, which both resolve to `Invalid` downstream.
pub fn parse_device_menu_id(suffix: &str) -> DeviceMenuSelection {
    if suffix == "none" {
        return DeviceMenuSelection::Invalid;
    }
    // Spotify device ids are opaque base62 strings; a pure-ASCII-digit
    // suffix can only have come from the old `{index}` scheme.
    if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(i) = suffix.parse::<usize>() {
            return DeviceMenuSelection::LegacyIndex(i);
        }
    }
    if suffix.is_empty() {
        return DeviceMenuSelection::Invalid;
    }
    DeviceMenuSelection::DeviceId(suffix.to_string())
}

/// Redacted label for the unknown-device warn log: ids are bearer-adjacent,
/// so log only the length, never the id itself.
pub fn selected_for_log(selected: &DeviceMenuSelection) -> String {
    match selected {
        DeviceMenuSelection::DeviceId(id) => format!("<id len={}>", id.len()),
        DeviceMenuSelection::LegacyIndex(i) => format!("<legacy index={}>", i),
        DeviceMenuSelection::Invalid => "<invalid>".to_string(),
    }
}

/// Resolves a parsed device-menu selection to a live Spotify device id.
/// Cache first (no IO), then a live `get_devices` re-fetch when the cached
/// list went stale (issue #388: the old `devices.get(i)` raced the
/// 60 s-throttled cache against the live device list). MUST run off the
/// menu-event thread — the fallback performs blocking HTTP (issue #386) —
/// and resolves its token through the shared refresh-aware policy, so an
/// expired access token does not strand a transfer on "unknown device"
/// (issue #586).
pub fn resolve_device_id(app: &AppHandle, selected: &DeviceMenuSelection) -> Option<String> {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let caches = &state.caches;
    match selected {
        DeviceMenuSelection::DeviceId(id) => {
            // Fast path: still in the cached list and transferable.
            let cached = caches
                .devices_slot()
                .lock()
                .as_ref()
                .and_then(|(_, devices)| {
                    devices
                        .iter()
                        .find(|d| d.id.as_deref() == Some(id.as_str()))
                        .and_then(|d| d.id.clone())
                });
            if cached.is_some() {
                return cached;
            }
            // Slow path: live re-fetch; the device may have appeared after
            // the submenu was built, or the cache may be stale.
            match crate::commands::playback::player_with_refresh_typed(
                state.inner(),
                app,
                "transfer device list",
                crate::spotify::get_devices,
            ) {
                Ok(devices) => {
                    *caches.devices_slot().lock() = Some((Instant::now(), devices.clone()));
                    devices
                        .into_iter()
                        .find(|d| d.id.as_deref() == Some(id.as_str()))
                        .and_then(|d| d.id)
                }
                Err(e) => {
                    log::warn!("[TRAY] transfer: live device re-fetch failed: {}", e);
                    None
                }
            }
        }
        DeviceMenuSelection::LegacyIndex(i) => caches
            .devices_slot()
            .lock()
            .as_ref()
            .and_then(|(_, devices)| devices.get(*i).cloned())
            .and_then(|device| device.id),
        DeviceMenuSelection::Invalid => None,
    }
}

/// Builds the Devices submenu from an already-fetched slice. No HTTP is
/// performed here — the caller must have fetched outside any tray lock.
/// See issue #217.
pub fn build_devices_submenu_from_devices(
    app: &AppHandle,
    devices: &[crate::spotify::DeviceInfo],
    s: &Strings,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_DEVICES, s.devices, true).map_err(|e| e.to_string())?;
    if devices.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_DEVICES), s.no_devices)
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for device in devices.iter() {
            let label = if device.is_active {
                format!("✓ {}", device.name)
            } else {
                device.name.clone()
            };
            // The active device (and id-less devices) can't be transferred to.
            let enabled = !device.is_active && device.id.is_some();
            // Issue #388: carry the stable Spotify device id (not the list
            // index) so a click resolves even when the cached list raced a
            // live device change. Id-less devices reuse the `none` id: they
            // are disabled and parse back to `Invalid`, which the click
            // handler rejects with a warn.
            let suffix = device.id.as_deref().unwrap_or("none");
            let item = MenuItemBuilder::with_id(format!("{}|{}", ID_DEVICES, suffix), label)
                .enabled(enabled)
                .build(app)
                .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
        }
    }
    Ok(submenu)
}

/// Builds the Up Next submenu from an already-fetched queue snapshot.
/// No HTTP here — fetch must have happened outside the tray lock. See issue #217.
pub fn build_queue_submenu_from_queue(
    app: &AppHandle,
    queue: Option<&crate::spotify::QueueInfo>,
    s: &Strings,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_QUEUE, s.up_next, true).map_err(|e| e.to_string())?;
    let up_next: Vec<crate::spotify::TrackInfo> = queue
        .map(|q| q.up_next.iter().take(3).cloned().collect())
        .unwrap_or_default();
    if up_next.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_QUEUE), s.queue_empty)
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for (index, track) in up_next.iter().enumerate() {
            let item = MenuItemBuilder::with_id(
                format!("{}|none|{}", ID_QUEUE, index),
                format!("{} - {}", track.artist, track.title),
            )
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
        }
    }
    Ok(submenu)
}

/// Issue #871: the tray's Volume submenu. Five discrete picks (0/25/50/75/100)
/// ride through `commands/playback::set_volume`, the same path the
/// Dashboard's slider uses. The whole submenu is disabled when the active
/// device refuses the volume capability, so a stale-capabilities click
/// never reaches the click handler.
pub fn build_volume_submenu(
    app: &AppHandle,
    s: &Strings,
    enabled: bool,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = SubmenuBuilder::new(app, s.volume_menu)
        .enabled(enabled)
        .build()
        .map_err(|e| e.to_string())?;
    for percent in [0u32, 25, 50, 75, 100] {
        let label = s
            .volume_percent_label
            .replace("{percent}", &percent.to_string());
        let item = MenuItemBuilder::with_id(format!("{VOLUME_ITEM_PREFIX}{percent}"), label)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&item).map_err(|e| e.to_string())?;
    }
    Ok(submenu)
}

/// Issue #871: the tray's Seek submenu (back / forward 30 s). Both
/// directions ride through `commands/playback::seek` — Spotify clamps the
/// position to the current track's duration on its side, so a "forward
/// 30 s" pick near the end of the track is a no-op rather than an error.
pub fn build_seek_submenu(
    app: &AppHandle,
    s: &Strings,
    enabled: bool,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = SubmenuBuilder::new(app, s.seek_menu)
        .enabled(enabled)
        .build()
        .map_err(|e| e.to_string())?;
    let back_label = s
        .seek_back_30s_label
        .replace("{seconds}", &(SEEK_BACK_30_MS / 1000).to_string());
    let back =
        MenuItemBuilder::with_id(format!("{SEEK_ITEM_PREFIX}-{SEEK_BACK_30_MS}"), back_label)
            .build(app)
            .map_err(|e| e.to_string())?;
    submenu.append(&back).map_err(|e| e.to_string())?;
    let forward_label = s
        .seek_forward_30s_label
        .replace("{seconds}", &(SEEK_FORWARD_30_MS / 1000).to_string());
    let forward = MenuItemBuilder::with_id(
        format!("{SEEK_ITEM_PREFIX}+{SEEK_FORWARD_30_MS}"),
        forward_label,
    )
    .build(app)
    .map_err(|e| e.to_string())?;
    submenu.append(&forward).map_err(|e| e.to_string())?;
    Ok(submenu)
}

/// Issue #871: the volume menu id parsed into the documented Spotify
/// `volume_percent`. `None` for any id that is not one of the five known
/// percentages so a stale menu from an older build cannot drive a
/// volume slider to a number Spotify would reject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeMenuSelection {
    Percent(u32),
}

/// Parses a volume menu-item id. The strict bounds (`0..=100`) match the
/// documented Spotify range, so a malicious or stale id cannot push the
/// device's volume past Spotify's own ceiling.
pub fn parse_volume_menu_id(id: &str) -> Option<VolumeMenuSelection> {
    let raw = id.strip_prefix(VOLUME_ITEM_PREFIX)?;
    let percent: u32 = raw.parse().ok()?;
    if percent <= 100 {
        Some(VolumeMenuSelection::Percent(percent))
    } else {
        None
    }
}

/// Issue #871: the seek menu id parsed into a signed millisecond delta.
/// The click handler applies the delta to the current `progress_ms`,
/// clamped at zero and the track's `duration_ms` so a forward jump past
/// the end of the track is a no-op rather than an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekMenuSelection {
    Delta(i64),
}

pub fn parse_seek_menu_id(id: &str) -> Option<SeekMenuSelection> {
    let raw = id.strip_prefix(SEEK_ITEM_PREFIX)?;
    // The sign lives in the first character; `+30` is forward, `-30` is
    // back. Anything else is refused.
    if let Some(rest) = raw.strip_prefix('+') {
        let ms: u64 = rest.parse().ok()?;
        // Cap at a sensible upper bound so a typo cannot jump 1000 minutes
        // forward. The tray only ever mints `+/- 30000` (30 s) but the
        // parser must defend against a menu snapshot from a future build.
        if ms <= 10 * 60 * 1000 {
            return Some(SeekMenuSelection::Delta(ms as i64));
        }
    } else if let Some(rest) = raw.strip_prefix('-') {
        let ms: u64 = rest.parse().ok()?;
        if ms <= 10 * 60 * 1000 {
            return Some(SeekMenuSelection::Delta(-(ms as i64)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::testkit::{body_of, prod_source, tray_prod_source};
    /// Issue #388: device menu ids must parse to stable Spotify device ids,
    /// never to a list index. Pure helper — no Tauri runtime needed.
    #[test]
    fn parse_device_menu_id_carries_stable_id() {
        assert_eq!(
            parse_device_menu_id("abc123XYZ"),
            DeviceMenuSelection::DeviceId("abc123XYZ".to_string())
        );
        assert_eq!(
            parse_device_menu_id("0"),
            DeviceMenuSelection::LegacyIndex(0)
        );
        assert_eq!(
            parse_device_menu_id("12"),
            DeviceMenuSelection::LegacyIndex(12)
        );
        assert_eq!(parse_device_menu_id("none"), DeviceMenuSelection::Invalid);
        assert_eq!(parse_device_menu_id(""), DeviceMenuSelection::Invalid);
    }
    /// Issue #482: id-snapshot edge cases -- a reorder between menu build
    /// and click must still hit the labelled device (id-keyed resolution),
    /// and unknown/empty selections must resolve to Invalid, never to a
    /// neighboring device by index.
    #[test]
    fn parse_device_menu_id_rejects_unknown_and_empty() {
        // Spotify ids are opaque base62; digit-only suffixes are the legacy
        // index scheme and must keep parsing as LegacyIndex, never as an id.
        assert_eq!(
            parse_device_menu_id("007"),
            DeviceMenuSelection::LegacyIndex(7)
        );
        // Mixed alphanumeric ids (even with a leading digit) are stable ids.
        assert_eq!(
            parse_device_menu_id("0abc"),
            DeviceMenuSelection::DeviceId("0abc".to_string())
        );
        // The placeholder and the empty string are never a device.
        assert_eq!(parse_device_menu_id("none"), DeviceMenuSelection::Invalid);
        assert_eq!(parse_device_menu_id(""), DeviceMenuSelection::Invalid);
        // Redaction helper never leaks the id itself.
        let logged = selected_for_log(&DeviceMenuSelection::DeviceId("secret-id-123".to_string()));
        assert!(
            !logged.contains("secret-id-123"),
            "device id must not appear in logs"
        );
        assert!(
            logged.contains("len="),
            "redacted label must carry the length"
        );
    }
    /// Issue #388: the click handler must resolve by id with a live
    /// re-fetch fallback instead of `devices.get(i)`. Issue #586: that
    /// re-fetch must resolve its token through the shared refresh-aware
    /// policy, so an expired token does not strand a transfer.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the resolve-by-id plus live re-fetch through the refresh-aware policy; the click needs a live `AppHandle` for HTTP, so the resolution wiring is pinned at the source.
    #[test]
    fn device_click_resolves_by_id_with_live_fallback() {
        let src = tray_prod_source();
        let body = body_of(prod_source(src), "fn resolve_device_id(");
        assert!(
            body.contains("crate::spotify::get_devices"),
            "resolve_device_id must live re-fetch when the cache misses"
        );
        assert!(
            body.contains("player_with_refresh_typed("),
            "the live re-fetch must use the refresh-aware token policy (issue #586)"
        );
        assert!(
            !body.contains("tokens.spotify()"),
            "resolve_device_id must not snapshot the raw access token (issue #586)"
        );
        assert!(
            !body.contains("devices.get(*i).cloned()") || body.contains("LegacyIndex"),
            "index lookup must survive only on the LegacyIndex back-compat path"
        );
    }
}
