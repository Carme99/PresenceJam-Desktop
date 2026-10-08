//! tray/dedup.rs — split from tray/mod.rs (#756).

#![allow(unused_imports)]

use super::*;
use crate::i18n::Strings;
use crate::spotify::RepeatState;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Snapshot of the last tray-menu state, used for the dedup guard in
/// `update_tray_menu` (issue #71). The polling thread calls
/// `update_tray_menu` on every successful poll; the menu only needs to
/// change when `is_syncing`, window visibility (the Show/Hide label), the
/// current track's title/is_playing, or one of the two playback modes
/// changes.
///
/// The two playback modes are part of the key (issue #691): their atoms
/// drive the Shuffle/Repeat check marks, so an external mode change has to
/// force a rebuild — otherwise the marks stayed on the previous mode. The
/// click target is unaffected: `shuffle_toggle_target` reads the live atom.
///
/// The snooze is part of the key too (4.7.0, S9 / issue #677): it drives the
/// status line, the presence of the "Resume sync now" entry AND the fetch mode
/// of the Devices/Up Next submenus, so a snooze that starts, ends or ticks over
/// to the next minute must repaint rather than early-return. The tick is why
/// the key carries [`snooze_dedup_key`]'s minute bucket and not just the
/// deadline — a key of the deadline alone would freeze the countdown at the
/// minute it was set.
#[derive(Clone, PartialEq, Eq)]
pub struct TrayStateSnapshot {
    pub(crate) is_syncing: bool,
    is_window_visible: bool,
    /// `artist|title|is_playing` — is_playing is in the key so a same-track
    /// pause repaints the Play/Pause mark (issue #229).
    pub(crate) track_key: Option<String>,
    shuffle: bool,
    repeat: RepeatState,
    /// Throttle bucket of the devices cache when this key was built (issue
    /// #805): the number of full [`TRAY_SPOTIFY_FETCH_THROTTLE`] windows since
    /// it was filled, `0` while it is empty.
    ///
    /// The key carries the bucket and not the timestamp because the bucket is
    /// what the fetchers act on: a session where nothing else moves now
    /// repaints — and therefore re-fetches — once per window, while a rebuild
    /// inside the window still dedupes to a no-op. Without it the early return
    /// below skipped the fetches themselves, and the Devices/Up Next submenus
    /// kept whatever the last rebuild happened to render, indefinitely.
    devices_bucket: u64,
    /// Same, for the Up Next queue cache (issue #805).
    queue_bucket: u64,
    snooze_key: Option<String>,
    /// Issue #869: the active presence profile id, fed into the dedup key
    /// so a profile switch (tray, CLI, Settings card) repaints the
    /// "Active profile" submenu's check mark. The id is `None` when the
    /// base configuration is in use — the documented pre-5.0 default and
    /// the post-switch state of `--profile base`.
    active_profile_key: Option<String>,
}

/// True when `next` differs from the last committed snapshot, i.e. when the
/// tray menu must be rebuilt. Pure, so the dedup contract is unit-testable
/// without a Tauri runtime.
pub fn tray_state_changed(prev: Option<&TrayStateSnapshot>, next: &TrayStateSnapshot) -> bool {
    prev != Some(next)
}

/// Builds the dedup key from the same inputs the rebuild renders from: the
/// caller's sync flag and precomputed window visibility, the track's
/// artist/title/is_playing, the two playback-mode atoms the polling loop feeds
/// (`note_playback_modes`), the snooze the rebuild will render (4.7.0, S9) and
/// the throttle bucket of both caches (issue #805).
/// Single construction site so the key can never be built from a subset of what
/// the menu shows (issue #691).
// Nine inputs by design (issue #691): the key is built at one construction
// site from everything the menu renders, so it stays a pure function that
// can never be built from a subset.
#[allow(clippy::too_many_arguments)]
pub fn tray_snapshot_for(
    is_syncing: bool,
    is_window_visible: bool,
    current_track: Option<&crate::spotify::TrackInfo>,
    snooze_key: Option<String>,
    // The two cache throttle buckets (issue #805). Read by the caller rather
    // than here so the key stays a pure function of its inputs.
    devices_bucket: u64,
    queue_bucket: u64,
    // Issue #869: the active profile id, fed in by the caller for the same
    // dedup reason as the snooze — a switch changes the check mark, so the
    // key has to include the id or the rebuild early-returns.
    active_profile_key: Option<String>,
    // The two playback modes (issue #691): fed in by the caller from
    // `AppCaches` for the same purity reason as the buckets above.
    shuffle: bool,
    repeat: RepeatState,
) -> TrayStateSnapshot {
    TrayStateSnapshot {
        is_syncing,
        is_window_visible,
        track_key: current_track.map(|t| format!("{}|{}|{}", t.artist, t.title, t.is_playing)),
        shuffle,
        repeat,
        snooze_key,
        devices_bucket,
        queue_bucket,
        active_profile_key,
    }
}

/// Records the main window's visibility (issue #886). Every path that shows or
/// hides the window should report it — the tray's own Show/Hide and Open
/// Settings arms do, and the window commands / close-to-tray paths are expected
/// to; the poll loop then never has to ask the event loop for it.
pub fn note_window_visibility(caches: &crate::state::AppCaches, visible: bool) {
    caches
        .window_visible_flag()
        .store(visible, Ordering::Release);
}

/// The visibility the dedup key is built from (issue #886) — the mirror, so the
/// discarded path performs no event-loop hop.
pub fn window_visible(caches: &crate::state::AppCaches) -> bool {
    caches.window_visible_flag().load(Ordering::Acquire)
}

/// The real main-window visibility, queried once per paint (issue #886). Only
/// the paths that are already off the hot path may call this: the startup paint
/// (on the main thread) and the rebuild that passed the dedup guard.
pub fn live_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// The shuffle state a click switches to: the inverse of the last observed
/// one. Pure so the toggle contract is unit-testable.
pub fn shuffle_toggle_target(current: bool) -> bool {
    !current
}

/// Records the playing state the Play/Pause mark and the status line render.
/// Every path that learns the truth writes it here, so the mark never infers
/// playback from a candidate that may already be stale. Issue #3.0-P3.
pub fn note_playing_state(caches: &crate::state::AppCaches, is_playing: bool) {
    caches.playing_flag().store(is_playing, Ordering::Release);
}

/// Consumes a `playback-state-changed` payload (issue #689, tray half) via
/// the shared [`crate::events::PlaybackStateChanged`] contract, so a Rust-side
/// field rename fails here too — not just at the emit site. Only `is_playing`
/// is consumed: the event's `track_key` is part of the shared contract, but
/// the tray's dedup key already carries the track identity. The poll body's
/// playing state is authoritative for a same-track change, and the poller has
/// already re-stored it, so recording it here keeps the Play/Pause mark and
/// the status line truthful without waiting for the next track. An unparsable
/// payload keeps the last known state rather than inventing one.
pub fn consume_playback_state_changed(caches: &crate::state::AppCaches, payload: &str) {
    match serde_json::from_str::<PlaybackStateChanged>(payload) {
        Ok(state) => note_playing_state(caches, state.is_playing),
        Err(e) => log::warn!(
            "[TRAY] playback-state-changed: unparsable payload, keeping the last playing state: {}",
            e
        ),
    }
}

/// Records the playback modes a poll body reported. Called by the polling
/// loop for every observed item (playing or paused) — the poll response is
/// the source of truth for both toggles, so no extra Spotify request is
/// needed to render them. See issue #582.
pub fn note_playback_modes(caches: &crate::state::AppCaches, shuffle: bool, repeat: RepeatState) {
    caches.shuffle_flag().store(shuffle, Ordering::Release);
    caches.repeat_flag().store(repeat as u8, Ordering::Release);
}

/// The tray's view of the current repeat mode. An out-of-range byte (only
/// possible if the encoder above is changed without this decoder) degrades
/// to `Off` rather than panicking in a menu build.
pub fn last_repeat_state(caches: &crate::state::AppCaches) -> RepeatState {
    match caches.repeat_flag().load(Ordering::Acquire) {
        1 => RepeatState::Context,
        2 => RepeatState::Track,
        _ => RepeatState::Off,
    }
}

/// Menu label for the Repeat item: the mode is spelled out because the
/// documented state space has three values and a check mark only carries
/// on/off. Pure, and localized from the table it is handed (issue #674), so
/// the label contract is unit-testable in every locale.
pub fn repeat_menu_label(strings: &Strings, state: RepeatState) -> &'static str {
    match state {
        RepeatState::Off => strings.repeat_off,
        RepeatState::Context => strings.repeat_context,
        RepeatState::Track => strings.repeat_track,
    }
}

/// One-line sync/status summary for the tray's status item and tooltip
/// (issue #591). The Pause/Resume verb on its own left the sync state
/// unstated, and the presence-gated dock badge is macOS-only. `is_playing`
/// is `LAST_PLAYING_STATE` — the same source as the Play/Pause checkmark —
/// not the polling loop's copy, which goes stale on a same-track pause.
///
/// Localized from the table it is handed (issue #674); the artist/title and
/// the em-dash separators are locale-neutral, so only the status word and the
/// track-less line come from the table.
pub fn sync_status_line(
    strings: &Strings,
    is_syncing: bool,
    is_playing: bool,
    track: Option<&crate::spotify::TrackInfo>,
) -> String {
    if !is_syncing {
        return strings.status_not_syncing.to_string();
    }
    match track {
        None => strings.status_syncing_no_track.to_string(),
        Some(t) if is_playing => format!("{} — {} — {}", strings.status_syncing, t.artist, t.title),
        Some(t) => format!("{} — {} — {}", strings.status_paused, t.artist, t.title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{DE, EN, FR};
    use crate::tray::testkit::MODE_ATOM_LOCK;
    /// Issue #582: the two toggles render the state the poll body reported
    /// (`note_playback_modes` → the atoms the menu build reads), and the
    /// click target is the documented cycle, so a successful toggle leaves
    /// the item showing what the API was just told to adopt.
    #[test]
    fn playback_modes_feed_both_toggle_items() {
        // Per-test caches (issue #758 slice 2): no global lock needed.
        let caches = crate::state::AppCaches::new();
        note_playback_modes(&caches, true, RepeatState::Track);
        assert!(caches.shuffle_flag().load(Ordering::Acquire));
        assert_eq!(last_repeat_state(&caches), RepeatState::Track);
        assert_eq!(
            repeat_menu_label(&EN, last_repeat_state(&caches)),
            "Repeat: Track"
        );
        // The click targets: Repeat advances along the documented cycle,
        // Shuffle flips whatever was last observed.
        assert_eq!(last_repeat_state(&caches).next(), RepeatState::Off);
        assert!(!shuffle_toggle_target(
            caches.shuffle_flag().load(Ordering::Acquire)
        ));

        // A poll that reports everything off must clear both items.
        note_playback_modes(&caches, false, RepeatState::Off);
        assert!(!caches.shuffle_flag().load(Ordering::Acquire));
        assert_eq!(last_repeat_state(&caches), RepeatState::Off);
        assert!(!last_repeat_state(&caches).is_on());
    }
    /// Issue #691: the dedup key is built by the same function the rebuild
    /// path uses, and it carries both playback modes — so a shuffle/repeat
    /// change made in another Spotify client forces the repaint that updates
    /// the check marks (and the next click toggles from the fresh belief),
    /// while an unchanged poll still dedupes to a no-op.
    #[test]
    fn mode_change_forces_a_tray_rebuild() {
        let caches = crate::state::AppCaches::new();
        let track = |is_playing: bool| crate::spotify::TrackInfo {
            title: "Title".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing,
            progress_ms: None,
            duration_ms: 0,
            volume_percent: None,
            supports_volume: None,
            actions: None,
        };
        let key = |caches: &crate::state::AppCaches, sync: bool, visible: bool| {
            tray_snapshot_for(
                sync,
                visible,
                Some(&track(true)),
                None,
                0,
                0,
                None,
                caches.shuffle_flag().load(Ordering::Acquire),
                last_repeat_state(caches),
            )
        };

        note_playback_modes(&caches, false, RepeatState::Off);
        let base = key(&caches, true, true);

        // The poll body reports a shuffle the user toggled in the Spotify app.
        note_playback_modes(&caches, true, RepeatState::Off);
        let shuffled = key(&caches, true, true);
        assert!(
            tray_state_changed(Some(&base), &shuffled),
            "an external shuffle change must repaint the tray (issue #691)"
        );

        // …and a repeat-mode change.
        note_playback_modes(&caches, true, RepeatState::Context);
        let repeated = key(&caches, true, true);
        assert!(
            tray_state_changed(Some(&shuffled), &repeated),
            "an external repeat change must repaint the tray (issue #691)"
        );

        // An unchanged poll is still a no-op.
        assert!(
            !tray_state_changed(Some(&repeated), &key(&caches, true, true)),
            "an unchanged poll must not rebuild the menu"
        );
        assert!(
            tray_state_changed(None, &repeated),
            "the first call has no snapshot and must always rebuild"
        );

        // The pre-existing parts of the key still repaint.
        assert!(
            tray_state_changed(Some(&repeated), &key(&caches, false, true)),
            "a sync toggle must still repaint (issue #71)"
        );
        assert!(
            tray_state_changed(Some(&repeated), &key(&caches, true, false)),
            "a Show/Hide click must still repaint (issue #71)"
        );

        // A same-track pause lives in the track half of the key (issue #229).
        let paused = tray_snapshot_for(
            true,
            true,
            Some(&track(false)),
            None,
            0,
            0,
            None,
            caches.shuffle_flag().load(Ordering::Acquire),
            last_repeat_state(&caches),
        );
        assert!(
            tray_state_changed(Some(&repeated), &paused),
            "a same-track pause must still repaint the Play/Pause mark (#229)"
        );

        // Leave the shared atoms as a fresh poll would find them.
        note_playback_modes(&caches, false, RepeatState::Off);
    }
    /// Issue #689 (D6, tray half): the poller emits `playback-state-changed`
    /// when a track's playing state changes without the track itself
    /// changing. Consuming it moves the Play/Pause mark and the status line
    /// off "playing" — the tray used to keep claiming the stale state until
    /// the next track emerged.
    #[test]
    fn playback_state_event_moves_the_play_pause_mark() {
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
        let mark = |playing: bool| sync_status_line(&EN, true, playing, Some(&track));

        let caches = crate::state::AppCaches::new();
        note_playing_state(&caches, true);
        assert_eq!(
            mark(caches.playing_flag().load(Ordering::Acquire)),
            "Syncing — Artist — Track"
        );

        // The exact payload shape the poller emits for a same-track pause.
        consume_playback_state_changed(
            &caches,
            r#"{"is_playing":false,"track_key":"Artist|Title"}"#,
        );
        assert!(
            !caches.playing_flag().load(Ordering::Acquire),
            "the tray must believe a same-track pause (issue #689)"
        );
        assert_eq!(
            mark(caches.playing_flag().load(Ordering::Acquire)),
            "Paused — Artist — Track"
        );

        consume_playback_state_changed(
            &caches,
            r#"{"is_playing":true,"track_key":"Artist|Title"}"#,
        );
        assert!(caches.playing_flag().load(Ordering::Acquire));
        assert_eq!(
            mark(caches.playing_flag().load(Ordering::Acquire)),
            "Syncing — Artist — Track"
        );

        // A payload the tray cannot parse must keep the last known state
        // rather than invent one.
        consume_playback_state_changed(&caches, "not json");
        assert!(
            caches.playing_flag().load(Ordering::Acquire),
            "an unparsable payload must not clobber the last playing state"
        );
    }
}
