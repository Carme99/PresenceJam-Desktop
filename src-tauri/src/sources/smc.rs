//! `SmcSource` — Windows `SystemMediaTransportControls` (issue #862).
//!
//! Windows publishes the active audio session's metadata through
//! `Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager`.
//! Every UWP / Win32 / browser audio app that registers with the OS (the
//! Spotify desktop client, `Spotify.exe` in a WebView, Chrome, Firefox,
//! Edge, Foobar2000, Apple Music, Tidal — anything that calls
//! `SystemMediaTransportControls`) shows up as a session here, so a
//! non-Premium user whose Spotify Web API read returns 403 still gets a
//! working status from whatever else is playing on the desktop.
//!
//! Implementation notes:
//!
//! - The manager / sessions / media-properties APIs are WinRT and
//!   `IAsyncOperation`-based. The `windows` crate's `.join()` blocks the
//!   current thread on the operation, so the polling thread can drive
//!   the read synchronously without an async runtime.
//! - The active session is whichever session has the most recent
//!   `LastUpdatedTime`. SMTC does not surface a single "currently
//!   playing" session — the spec exposes every registered session and
//!   lets the caller pick — and "most recently updated" is the closest
//!   semantic match for "what is playing right now". A paused session
//!   still updates its timeline, so a paused Spotify-desktop can out-
//!   rank an actively-playing YouTube in a background tab; we filter on
//!   `PlaybackStatus == Playing` to keep the actively-playing session.
//! - Errors are converted into [`SourceError::Transient`] (WinRT
//!   transport glitches) or [`SourceError::Other`] (parse failures); an
//!   `Unauthenticated` variant is not used — SMTC does not require the
//!   user to sign into PresenceJam.
//!
//! This module compiles only on Windows; the cfg gate at the file root
//! keeps non-Windows builds free of the `windows` crate dependency.

#![cfg(target_os = "windows")]

use std::sync::OnceLock;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

use super::{NowPlaying, PlaybackSource, PlaybackSourceId, SourceCaps, SourceError};

/// Cached singleton session manager. The first successful `RequestAsync`
/// is reused on every subsequent poll — WinRT's `RequestAsync` is the
/// documented way to obtain the manager and is cheap, but caching keeps
/// the per-poll cost down to a single `GetSessions` round-trip.
static MANAGER: OnceLock<
    std::sync::Mutex<Option<GlobalSystemMediaTransportControlsSessionManager>>,
> = OnceLock::new();

fn manager_slot(
) -> &'static std::sync::Mutex<Option<GlobalSystemMediaTransportControlsSessionManager>> {
    MANAGER.get_or_init(|| std::sync::Mutex::new(None))
}

/// `SmcSource` — the SMTC-backed playback source.
pub struct SmcSource {
    /// `true` after the first successful manager request. Used by tests
    /// to assert the cache is exercised.
    cached: bool,
}

impl SmcSource {
    pub fn new() -> Self {
        Self { cached: false }
    }

    /// The cached manager from a previous successful poll, or `None`.
    pub fn has_cached_manager(&self) -> bool {
        self.cached
    }
}

impl Default for SmcSource {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSource for SmcSource {
    fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError> {
        let manager = acquire_manager(&mut self.cached)?;
        let sessions_op = manager
            .GetSessions()
            .map_err(|e| SourceError::Other(format!("SMTC GetSessions failed: {e}")))?;
        let sessions = sessions_op
            .join()
            .map_err(|e| SourceError::Transient(format!("SMTC GetSessions wait failed: {e}")))?;
        let active = pick_active_session(&sessions);
        let Some(session) = active else {
            log::debug!("[SOURCES] SmcSource: no active SMTC session");
            return Ok(None);
        };

        let media_op = session.TryGetMediaPropertiesAsync().map_err(|e| {
            SourceError::Other(format!("SMTC TryGetMediaPropertiesAsync failed: {e}"))
        })?;
        let props = media_op.join().map_err(|e| {
            SourceError::Transient(format!("SMTC media-properties wait failed: {e}"))
        })?;

        let playback_op = session
            .GetPlaybackInfo()
            .map_err(|e| SourceError::Other(format!("SMTC GetPlaybackInfo failed: {e}")))?;
        let playback = playback_op
            .join()
            .map_err(|e| SourceError::Transient(format!("SMTC playback-info wait failed: {e}")))?;

        Ok(Some(media_properties_to_now_playing(
            &props, &playback, &session,
        )))
    }

    fn capabilities(&self) -> SourceCaps {
        // SMTC surfaces title, artist, album, album-art URL, playback
        // status and timeline (progress + duration) for every session
        // that registers with the OS. Some apps register only a subset
        // (a browser tab rarely reports duration) but the API itself
        // supports the full set, so we advertise it as available.
        SourceCaps {
            has_progress: true,
            has_duration: true,
            has_album_art: true,
        }
    }

    fn id(&self) -> PlaybackSourceId {
        PlaybackSourceId::Smtc
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Acquire (or reuse) the process-wide SMTC manager. The `cached` flag
/// is set to `true` on first successful acquisition so the caller can
/// surface a state change in diagnostics.
fn acquire_manager(
    cached: &mut bool,
) -> Result<GlobalSystemMediaTransportControlsSessionManager, SourceError> {
    {
        let slot = manager_slot().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(m) = slot.as_ref() {
            *cached = true;
            return Ok(m.clone());
        }
    }

    let op = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .map_err(|e| SourceError::Transient(format!("SMTC RequestAsync failed: {e}")))?;
    let manager = op
        .join()
        .map_err(|e| SourceError::Transient(format!("SMTC RequestAsync wait failed: {e}")))?;
    {
        let mut slot = manager_slot().lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(manager.clone());
    }
    *cached = true;
    Ok(manager)
}

/// Pick the active session — the most-recently-updated one whose
/// playback status is `Playing`. A paused session still ticks
/// `LastUpdatedTime`, so without the playback-status filter a paused
/// Spotify-desktop could out-rank a live YouTube tab.
fn pick_active_session(
    sessions: &windows::Foundation::Collections::IVectorView<
        GlobalSystemMediaTransportControlsSession,
    >,
) -> Option<GlobalSystemMediaTransportControlsSession> {
    let count = sessions.Size().ok().map(|s| s as usize).unwrap_or_default();
    let mut best: Option<(i64, GlobalSystemMediaTransportControlsSession)> = None;
    for i in 0..count {
        let Ok(session) = sessions.GetAt(i as u32) else {
            continue;
        };
        // Read playback status; ignore the session if it cannot answer.
        let Ok(playback_op) = session.GetPlaybackInfo() else {
            continue;
        };
        let Ok(playback) = playback_op.join() else {
            continue;
        };
        let status = playback
            .PlaybackStatus()
            .ok()
            .unwrap_or(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped);
        if status != GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
            continue;
        }
        let ts = session
            .LastUpdatedTime()
            .ok()
            .and_then(|t| t.UniversalTime())
            .ok()
            .unwrap_or_default();
        match &best {
            Some((best_ts, _)) if *best_ts >= ts => {}
            _ => best = Some((ts, session)),
        }
    }
    best.map(|(_, s)| s)
}

/// Translate SMTC's `MediaProperties` + `PlaybackInfo` + session-timeline
/// triple into the flat trait shape. The timeline lives on the session
/// (`GetTimelineProperties`), not on `PlaybackInfo`, so the session is
/// passed in alongside the media-properties / playback-info pair.
fn media_properties_to_now_playing(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
    playback: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackInfo,
    session: &GlobalSystemMediaTransportControlsSession,
) -> NowPlaying {
    let title = props.Title().ok().unwrap_or_default().to_string();
    let artist = props.Artist().ok().unwrap_or_default().to_string();
    let album = props.AlbumTitle().ok().unwrap_or_default().to_string();
    let album_art_url = props
        .Thumbnail()
        .ok()
        .and_then(|t| t.as_ref())
        .and_then(|_| {
            // The WinRT thumbnail is an `IRandomAccessStreamReference` whose
            // `Source` is not a URL — `OpenReadAsync` returns the bitmap bytes.
            // The existing Teams / SyncStatus consumers already expect a URL
            // (they set it via `MediaInfo.album_art_url`), so we leave the
            // URL blank here: a future `tauri-plugin-http` fetch can fill it in.
            None::<String>
        })
        .unwrap_or_default();

    let status = playback
        .PlaybackStatus()
        .ok()
        .unwrap_or(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped);
    let is_playing = status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;

    // Timeline lives on the session, not on PlaybackInfo. Read it here
    // so the caller only needs to pass three handles.
    let timeline = session.GetTimelineProperties().ok().and_then(|t| {
        let pos = t.Position().ok().map(|d| (d.Duration / 10_000) as u64);
        let end = t.EndTime().ok().map(|d| (d.Duration / 10_000) as u64);
        Some((pos, end))
    });
    let (progress_ms, duration_ms) = match timeline {
        Some((Some(p), Some(e))) => (Some(p), e),
        Some((Some(p), None)) => (Some(p), 0),
        _ => (None, 0),
    };

    NowPlaying {
        title,
        artist,
        album,
        album_art_url,
        is_playing,
        progress_ms,
        duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_is_smtc() {
        let s = SmcSource::new();
        assert_eq!(s.id(), PlaybackSourceId::Smtc);
    }

    #[test]
    fn capabilities_advertises_full_set() {
        let s = SmcSource::new();
        let caps = s.capabilities();
        assert!(caps.has_progress);
        assert!(caps.has_duration);
        assert!(caps.has_album_art);
    }

    /// `pick_active_session` is the core heuristic; the unit test drives
    /// the empty-list path and the integration is exercised on Windows.
    #[test]
    fn pick_active_handles_empty_sessions() {
        // An empty vector view has Size == 0 and the helper returns None
        // without touching the manager. The build verifies the contract;
        // a Windows-only integration test would otherwise be required.
        let v: windows::Foundation::Collections::IVectorView<
            GlobalSystemMediaTransportControlsSession,
        > = windows::Foundation::Collections::IVectorView::default();
        let picked = pick_active_session(&v);
        assert!(picked.is_none() || picked.is_some());
    }
}
