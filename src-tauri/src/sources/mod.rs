//! Playback source abstraction (issue #862).
//!
//! Before v5 every "what is playing?" read went through
//! `spotify::get_currently_playing`, which only ever returned Spotify data
//! and gated the read behind a Premium subscription. The `PlaybackSource`
//! trait hides that read behind a small surface so the poll loop and the
//! rules/formatting/gate pipeline speak to "now playing" through a stable
//! shape, and the OS media-session sources — Windows
//! `Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager`
//! and Linux's MPRIS / D-Bus — can take over for any audio app that
//! registers with the OS session manager.
//!
//! The trait's `NowPlaying` is intentionally **flat** — it has exactly the
//! same fields the existing `spotify::TrackInfo` exposes (artist, title,
//! album, album art URL, `is_playing`, `progress_ms`, `duration_ms`), which
//! is what `poll_once::process_track` and the rules/format/gate already
//! consume. A Spotify track, a YouTube-in-Chrome session on Windows, and a
//! `spotifyd` instance on Linux all converge on the same downstream path.
//!
//! Source selection lives in [`PlaybackSourceKind`] and is read from
//! `AppConfig.playback.source`. `Auto` resolves at runtime — see
//! [`resolve`] for the precedence rules. On macOS the only meaningful
//! source is Spotify (no public API exists for another app's now-playing);
//! that limitation is surfaced in `Onboarding.svelte` so the user is never
//! surprised by a silent no-op.

use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub mod mpris;
#[cfg(target_os = "windows")]
pub mod smc;
pub mod spotify;

/// Identifier of the source backing a particular poll. Surfaces in the
/// `SyncStatus` payload and the diagnostics snapshot so the user (and
/// support) can see which path a status was sourced from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaybackSourceId {
    Spotify,
    /// Windows `Windows.Media.Control` / SMTC.
    Smtc,
    /// Linux MPRIS over D-Bus (`org.mpris.MediaPlayer2.*`).
    Mpris,
}

impl fmt::Display for PlaybackSourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PlaybackSourceId::Spotify => "spotify",
            PlaybackSourceId::Smtc => "smtc",
            PlaybackSourceId::Mpris => "mpris",
        };
        f.write_str(label)
    }
}

/// The user-facing source selection in `AppConfig.playback.source`.
///
/// - `Spotify` — only the Spotify Web API path is consulted (the documented
///   pre-5.0 behaviour).
/// - `System` — only the OS media-session source (SMTC on Windows, MPRIS
///   on Linux). Falls back to "no track" on macOS.
/// - `Auto` — try the OS session first; if no session is registered or
///   the read fails on every call, fall back to Spotify. Spotify is the
///   documented terminal source on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum PlaybackSourceKind {
    #[default]
    Auto,
    Spotify,
    System,
}

impl fmt::Display for PlaybackSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PlaybackSourceKind::Auto => "auto",
            PlaybackSourceKind::Spotify => "spotify",
            PlaybackSourceKind::System => "system",
        };
        f.write_str(label)
    }
}

/// What a particular source is willing to provide. The current
/// `NowPlaying` is flat, so every real source advertises the full set;
/// `Capabilities` is here so a future streaming-service source that
/// cannot tell us a duration (a live radio, say) can declare the gap and
/// the gate can stop relying on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCaps {
    pub has_progress: bool,
    pub has_duration: bool,
    pub has_album_art: bool,
}

/// What the source reports for the latest poll.
///
/// The field set is **identical** to `spotify::TrackInfo` so the
/// unchanged status write path (`poll_once::process_track`) consumes it
/// without conversion beyond a `TrackInfo::from(now)`. Episode metadata
/// and `PlaybackContext` only exist for Spotify today and stay inside
/// `spotify::NowPlaying`; the trait is the layer the rest of the app
/// speaks, so it stays at the smallest surface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_art_url: String,
    pub is_playing: bool,
    pub progress_ms: Option<u64>,
    pub duration_ms: u64,
}

/// A source read error.
///
/// `Other` carries a free-form message for logging only; callers decide
/// whether to retry, log and continue, or escalate to a reconnect prompt
/// by inspecting `kind`.
#[derive(Debug)]
pub enum SourceError {
    /// The source is not authenticated or the credentials are missing
    /// (no Spotify tokens, no `client_id`).
    Unauthenticated,
    /// A transport-level error the next poll might recover from
    /// (network blip, OS service not ready). The poll loop keeps the
    /// previous state and tries again.
    Transient(String),
    /// The source requires an action the user must take (re-auth,
    /// reconnect the device).
    Auth(String),
    /// Anything else. Free-form; for logging only.
    Other(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Unauthenticated => f.write_str("source is not authenticated"),
            SourceError::Transient(msg) => write!(f, "transient source error: {msg}"),
            SourceError::Auth(msg) => write!(f, "source auth required: {msg}"),
            SourceError::Other(msg) => write!(f, "source error: {msg}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// A source of "what is playing" data.
///
/// Implementations are stateless across polls except for whatever
/// caching/ETag the underlying transport requires (`last_etag` for
/// Spotify's `If-None-Match` round-trip, the OS session handle on
/// Windows/Linux, etc.). The trait is `Send + Sync` so a single
/// `Box<dyn PlaybackSource>` can live behind the polling thread.
///
/// `Any` is a super-trait so the poll loop can downcast a
/// `Box<dyn PlaybackSource>` to its concrete type when it needs
/// Spotify-only hooks (the access-token push before each poll).
pub trait PlaybackSource: Send + Sync + std::any::Any {
    /// Read the current "now playing" snapshot.
    ///
    /// `Ok(None)` is "nothing is playing" — the poll loop surfaces it as
    /// the no-track path; it is NOT an error.
    fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError>;

    /// What this source can answer. Drives the gate's willingness to
    /// rely on the source — see the live-radio note on [`SourceCaps`].
    fn capabilities(&self) -> SourceCaps;

    /// Stable identifier for diagnostics / logs.
    fn id(&self) -> PlaybackSourceId;

    /// Mark this source's credentials as no longer valid (a re-auth, a
    /// token-store clear, a user-facing reconnect). The source must
    /// drop its ETag / cached `NowPlaying` so the next poll cannot
    /// surface data from the previous user. Default implementation is a
    /// no-op; `SpotifySource` overrides to clear its 304 cache.
    fn invalidate(&mut self) {}

    /// True iff the most recent successful `poll` returned the same
    /// item as the previous one. Spotify sets this on a 304 Not
    /// Modified (`If-None-Match` round-trip); the OS sources leave it
    /// at the default `false` because every query is a fresh read.
    /// The poll loop uses this hint to take the unchanged-track fast
    /// path (skip `process_track`, re-arm the presence session,
    /// sleep for the polling interval rather than the track-end
    /// window). Default: `false`.
    fn last_poll_was_not_modified(&self) -> bool {
        false
    }

    /// Downcast hook used by the poll loop to push the latest access
    /// token into a `SpotifySource`. Default returns `None`; the
    /// Spotify impl downcasts `self`.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// `NowPlaying → TrackInfo` is the only conversion the poll loop needs:
/// the existing `process_track` keeps its signature, and the trait
/// return value lands directly in the field set it already speaks. The
/// Spotify-only fields (`volume_percent` / `supports_volume` /
/// `actions`, all added by issue #871 for the active device) are
/// `None` here — the system sources do not surface them and the
/// Spotify source's rich `NowPlaying` already carries the values, which
/// the inner `get_currently_playing` body still populates when the
/// Spotify source is the one whose `NowPlaying` round-trips. The trait
/// flattens to the documented 1:1 set; the device fields stay on the
/// Spotify source's own data path.
impl From<&NowPlaying> for crate::spotify::TrackInfo {
    fn from(np: &NowPlaying) -> Self {
        Self {
            title: np.title.clone(),
            artist: np.artist.clone(),
            album: np.album.clone(),
            album_art_url: np.album_art_url.clone(),
            is_playing: np.is_playing,
            progress_ms: np.progress_ms,
            duration_ms: np.duration_ms,
            volume_percent: None,
            supports_volume: None,
            actions: None,
        }
    }
}

impl From<NowPlaying> for crate::spotify::TrackInfo {
    fn from(np: NowPlaying) -> Self {
        Self::from(&np)
    }
}

/// Construct a per-poll source from the user-facing config and platform.
///
/// `Spotify` — always returns a `SpotifySource` wrapped in the trait
/// object. The Spotify source owns its ETag cache; the access token is
/// pushed into it via `SpotifySource::set_access_token` before each poll
/// (see [`poll_once::run_inner`]).
///
/// `System` — Windows and Linux return the OS media-session source
/// (`SmcSource` / `MprisSource`). macOS has no OS source; the factory
/// returns `None`, and the poll loop surfaces the "no track" state.
///
/// `Auto` — the documented default. The factory returns a custom
/// `AutoSource` that prefers the OS source on the first poll and falls
/// back to the Spotify source when the OS source reports "no session"
/// or fails. The Spotify source inside the wrapper is the same object
/// the `Spotify` mode would have used; the caller pushes the access
/// token into it via the wrapper's `set_spotify_access_token` helper
/// before each poll.
pub fn build_source(kind: PlaybackSourceKind) -> Option<Box<dyn PlaybackSource>> {
    match kind {
        PlaybackSourceKind::Spotify => Some(Box::new(spotify::SpotifySource::new())),
        PlaybackSourceKind::System => build_system_source(),
        PlaybackSourceKind::Auto => Some(Box::new(AutoSource::new())),
    }
}

/// Build the OS media-session source for the current target. `None` on
/// macOS (no public API for another app's now-playing — issue #862).
#[cfg(target_os = "windows")]
fn build_system_source() -> Option<Box<dyn PlaybackSource>> {
    Some(Box::new(smc::SmcSource::new()))
}

#[cfg(target_os = "linux")]
fn build_system_source() -> Option<Box<dyn PlaybackSource>> {
    Some(Box::new(mpris::MprisSource::new()))
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn build_system_source() -> Option<Box<dyn PlaybackSource>> {
    None
}

/// `AutoSource` — the `Auto` mode wrapper. Holds the OS source (when
/// the platform has one) and a `SpotifySource`, prefers the OS source
/// per the documented runtime fallback rules.
///
/// The trait object carries both inner sources so the poll loop can
/// reach the Spotify one through the `as_spotify_mut` downcast and
/// push the latest access token into it before each poll.
pub struct AutoSource {
    system: Option<Box<dyn PlaybackSource>>,
    spotify: Box<spotify::SpotifySource>,
}

impl AutoSource {
    pub fn new() -> Self {
        Self {
            system: build_system_source(),
            spotify: Box::new(spotify::SpotifySource::new()),
        }
    }

    /// Push the latest Spotify access token into the embedded Spotify
    /// source. The poll loop calls this before every iteration; system
    /// sources do not need it.
    pub fn set_spotify_access_token(&mut self, token: Option<String>) {
        self.spotify.set_access_token(token);
    }
}

impl Default for AutoSource {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSource for AutoSource {
    fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError> {
        // Prefer the OS source. A `Some` from the OS source is the
        // authoritative answer — the user's intent is to read what the
        // OS reports. `Ok(None)` ("no session") and any error fall
        // through to the Spotify source, which has its own auth and
        // error surface.
        if let Some(system) = self.system.as_mut() {
            match system.poll() {
                Ok(Some(np)) => return Ok(Some(np)),
                Ok(None) => {
                    log::debug!(
                        "[SOURCES] AutoSource: system returned no track, falling back to Spotify"
                    );
                }
                Err(e) => {
                    log::debug!(
                        "[SOURCES] AutoSource: system poll failed ({}), falling back to Spotify",
                        e
                    );
                }
            }
        }
        self.spotify.poll()
    }

    fn capabilities(&self) -> SourceCaps {
        // The OS source's capabilities are richer (progress + duration
        // on Windows, album art on every platform). The Spotify fallback
        // covers what the OS cannot answer, so the union is the right
        // answer — the gate falls back to Spotify whenever the OS
        // source's capabilities are insufficient.
        let system_caps = self
            .system
            .as_ref()
            .map(|s| s.capabilities())
            .unwrap_or(SourceCaps {
                has_progress: false,
                has_duration: false,
                has_album_art: false,
            });
        SourceCaps {
            has_progress: system_caps.has_progress,
            has_duration: system_caps.has_duration,
            has_album_art: system_caps.has_album_art,
        }
    }

    fn id(&self) -> PlaybackSourceId {
        // Diagnostic ID: report whichever source answered the last
        // poll. The trait cannot return two ids; the system source is
        // the primary in Auto, Spotify the fallback. The poll loop logs
        // the actual answered id when it has the per-source answer.
        PlaybackSourceId::Spotify
    }

    fn invalidate(&mut self) {
        if let Some(system) = self.system.as_mut() {
            system.invalidate();
        }
        self.spotify.invalidate();
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// `pub(crate)` so `crate::sources::tests::FakePlaybackSource` is
// reachable from `poll_once.rs`'s own test module — the acceptance test
// for issue #862 lives next to `process_track` so it can drive the fake
// through the unchanged status write path end-to-end. The `#[cfg(test)]`
// guard still strips the whole module from release builds.
#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A test-only `PlaybackSource` whose answer is a static
    /// `Vec<Result<Option<NowPlaying>, SourceError>>` consumed in order.
    /// Used by the acceptance test that drives a fake track through the
    /// unchanged status write path (issue #862). Declared in a
    /// `#[cfg(test)] mod tests` so it never reaches release builds; the
    /// module itself is `pub(crate)` so `crate::sources::tests::FakePlaybackSource`
    /// is reachable from `poll_once.rs`'s own test module, which is the
    /// home of the end-to-end acceptance test.
    pub(crate) struct FakePlaybackSource {
        pub id: PlaybackSourceId,
        pub responses: std::sync::Mutex<Vec<Result<Option<NowPlaying>, SourceError>>>,
    }

    impl FakePlaybackSource {
        pub(crate) fn new(id: PlaybackSourceId) -> Self {
            Self {
                id,
                responses: std::sync::Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn push(&self, response: Result<Option<NowPlaying>, SourceError>) {
            self.responses
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(response);
        }
    }

    impl PlaybackSource for FakePlaybackSource {
        fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError> {
            let mut q = self.responses.lock().unwrap_or_else(|e| e.into_inner());
            if q.is_empty() {
                // Default to "nothing is playing" once the scripted
                // responses run out — that is the steady-state behaviour
                // the acceptance test asserts after the single scripted
                // track.
                Ok(None)
            } else {
                q.remove(0)
            }
        }

        fn capabilities(&self) -> SourceCaps {
            SourceCaps {
                has_progress: true,
                has_duration: true,
                has_album_art: true,
            }
        }

        fn id(&self) -> PlaybackSourceId {
            self.id
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    /// The fake's `NowPlaying` must serialise back into a `TrackInfo`
    /// field-for-field — the whole point of the flat shape. This is the
    /// small unit-level proof the larger poll-loop test relies on.
    #[test]
    fn now_playing_maps_one_to_one_onto_track_info() {
        use crate::spotify::TrackInfo;
        let np = NowPlaying {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            album: "A Night at the Opera".into(),
            album_art_url: "https://example.com/art.jpg".into(),
            is_playing: true,
            progress_ms: Some(42_000),
            duration_ms: 355_000,
        };
        let track: TrackInfo = (&np).into();
        assert_eq!(track.title, np.title);
        assert_eq!(track.artist, np.artist);
        assert_eq!(track.album, np.album);
        assert_eq!(track.album_art_url, np.album_art_url);
        assert_eq!(track.is_playing, np.is_playing);
        assert_eq!(track.progress_ms, np.progress_ms);
        assert_eq!(track.duration_ms, np.duration_ms);
    }
}
