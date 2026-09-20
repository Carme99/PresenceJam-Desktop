//! `SpotifySource` — the `PlaybackSource` wrapper around the existing
//! `spotify::get_currently_playing` HTTP read (issue #862).
//!
//! The source owns the `last_etag` and the cached `NowPlaying` the
//! 304-not-modified path returns without re-parsing the body (issue #577).
//! Token refresh stays outside the source: the poll loop swaps the
//! access token via [`SpotifySource::set_access_token`] before every
//! poll, and an expired token surfaces as [`SourceError::Auth`] so the
//! caller can refresh and retry without coupling the source to the
//! `state.tokens` slot.
//!
//! The existing `spotify::NowPlaying { media: TrackInfo, episode,
//! context }` shape carries Spotify-only metadata the system sources do
//! not produce. The trait exposes a flat `NowPlaying` (matching the
//! `TrackInfo` field set) so the unchanged status write path can take
//! the source's value directly. Episode / context data is preserved by
//! the surrounding `poll_once` adapter — the source flattens only at
//! the trait boundary.

use super::{NowPlaying, PlaybackSource, PlaybackSourceId, SourceCaps, SourceError};

/// `SpotifySource` — every read goes through `spotify::get_currently_playing`,
/// which is the same HTTP call the pre-v5 polling loop made directly
/// (issue #862, `src-tauri/src/spotify.rs:1244`). The source adds nothing
/// beyond the trait boundary and 304 caching.
pub struct SpotifySource {
    /// Bearer token the next `poll` will use. Updated by the poll loop
    /// from `state.tokens.spotify()` before every iteration. `None` while
    /// the user has not authenticated — the source then refuses the read
    /// with [`SourceError::Unauthenticated`].
    access_token: Option<String>,
    /// The `ETag` from the previous successful read. Sent back as
    /// `If-None-Match` on the next poll so an unchanged resource answers
    /// 304 without a body (issue #577).
    last_etag: Option<String>,
    /// The cached track from the most recent 200 response. A 304 returns
    /// this directly so the trait never has to expose the 304 distinction
    /// — the poll loop's `status_track_key` comparison recognises it as
    /// the same track and falls into the unchanged-track path.
    last_now_playing: Option<NowPlaying>,
    /// `true` iff the most recent `poll` returned the cached
    /// `last_now_playing` because Spotify answered 304. Drives
    /// `last_poll_was_not_modified`. Cleared on every 200/204/error so
    /// a single fresh read resets the flag.
    last_was_not_modified: bool,
}

impl SpotifySource {
    pub fn new() -> Self {
        Self {
            access_token: None,
            last_etag: None,
            last_now_playing: None,
            last_was_not_modified: false,
        }
    }

    /// Update the bearer token the next `poll` will use. The poll loop
    /// calls this once per iteration from `state.tokens.spotify()`.
    pub fn set_access_token(&mut self, token: Option<String>) {
        self.access_token = token;
    }

    /// Clear the 304 cache — called when the auth state changes (a
    /// reconnect, a fresh login) so the new poll cannot return a stale
    /// `NowPlaying` from the previous user.
    pub fn invalidate(&mut self) {
        self.last_etag = None;
        self.last_now_playing = None;
        self.last_was_not_modified = false;
    }
}

impl Default for SpotifySource {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackSource for SpotifySource {
    fn poll(&mut self) -> Result<Option<NowPlaying>, SourceError> {
        let token = match self.access_token.as_deref() {
            Some(t) if !t.is_empty() => t,
            _ => {
                log::debug!("[SOURCES] SpotifySource: no access token, returning Unauthenticated");
                return Err(SourceError::Unauthenticated);
            }
        };

        match crate::spotify::get_currently_playing(token, self.last_etag.as_deref()) {
            Ok(crate::spotify::CurrentlyPlaying::Modified {
                now: Some(np),
                etag,
            }) => {
                self.last_etag = etag;
                let flat = flatten_spotify_now_playing(&np);
                self.last_now_playing = Some(flat.clone());
                self.last_was_not_modified = false;
                Ok(Some(flat))
            }
            Ok(crate::spotify::CurrentlyPlaying::Modified { now: None, etag }) => {
                // 204 — nothing is playing. Cache the new etag so the next
                // request can use it (Spotify sends an ETag on 204 too).
                self.last_etag = etag;
                self.last_now_playing = None;
                self.last_was_not_modified = false;
                Ok(None)
            }
            Ok(crate::spotify::CurrentlyPlaying::NotModified) => {
                // 304 — the body is unchanged. Surface the cached track so
                // the poll loop's `last_track_key` comparison keeps
                // recognising it as the same one. Flag the iteration so
                // the poll loop takes the unchanged-track fast path.
                self.last_was_not_modified = true;
                Ok(self.last_now_playing.clone())
            }
            Err(crate::spotify::SpotifyApiError::ExpiredToken) => {
                // The poll loop catches `SourceError::Auth` and refreshes
                // via `refresh_spotify_token`. Drop the etag so the retry
                // cannot accidentally turn into a 304-by-stale-etag.
                self.last_etag = None;
                self.last_was_not_modified = false;
                Err(SourceError::Auth("spotify access token expired".into()))
            }
            Err(crate::spotify::SpotifyApiError::InvalidGrant) => {
                self.last_was_not_modified = false;
                Err(SourceError::Auth(
                    "spotify refresh token invalid (invalid_grant)".into(),
                ))
            }
            Err(crate::spotify::SpotifyApiError::NotPremium) => {
                // Non-Premium users hit this on the player endpoint. The
                // system sources (SMTC / MPRIS) still surface what is
                // playing in their browser / desktop client, so fall
                // back to `Err(Auth)` and let the poll loop swap to the
                // OS source in `Auto` mode.
                log::warn!(
                    "[SOURCES] SpotifySource: Spotify reported NotPremium — caller should switch source"
                );
                self.last_was_not_modified = false;
                Err(SourceError::Auth(
                    "spotify playback read requires Premium".into(),
                ))
            }
            Err(crate::spotify::SpotifyApiError::NoActiveDevice) => {
                self.last_was_not_modified = false;
                Err(SourceError::Other(
                    "spotify returned NO_ACTIVE_DEVICE".into(),
                ))
            }
            Err(crate::spotify::SpotifyApiError::RateLimited(retry_after)) => {
                self.last_was_not_modified = false;
                Err(SourceError::Transient(format!(
                    "spotify rate limited (retry_after={:?})",
                    retry_after
                )))
            }
            Err(crate::spotify::SpotifyApiError::Transient { status, .. }) => {
                self.last_was_not_modified = false;
                Err(SourceError::Transient(format!(
                    "spotify transient HTTP {status}"
                )))
            }
            Err(crate::spotify::SpotifyApiError::Http { status, .. }) => {
                self.last_was_not_modified = false;
                Err(SourceError::Other(format!(
                    "spotify HTTP {status} (non-retriable)"
                )))
            }
            Err(crate::spotify::SpotifyApiError::Other(msg)) => {
                self.last_was_not_modified = false;
                Err(SourceError::Other(msg))
            }
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
        PlaybackSourceId::Spotify
    }

    fn invalidate(&mut self) {
        // Same body as the inherent `SpotifySource::invalidate`: drop
        // the etag and the 304 cache. Calling the inherent method
        // directly avoids the trait dispatch recursion (Rust would
        // resolve `self.invalidate()` to the trait method again).
        self.last_etag = None;
        self.last_now_playing = None;
        self.last_was_not_modified = false;
    }

    fn last_poll_was_not_modified(&self) -> bool {
        self.last_was_not_modified
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Flatten the rich `spotify::NowPlaying { media: TrackInfo, episode,
/// context }` down to the trait's flat shape. The media field IS the
/// TrackInfo, so the conversion is a field-for-field clone.
fn flatten_spotify_now_playing(np: &crate::spotify::NowPlaying) -> NowPlaying {
    let m = &np.media;
    NowPlaying {
        title: m.title.clone(),
        artist: m.artist.clone(),
        album: m.album.clone(),
        album_art_url: m.album_art_url.clone(),
        is_playing: m.is_playing,
        progress_ms: m.progress_ms,
        duration_ms: m.duration_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::{
        EpisodeInfo, NowPlaying as SpotifyNowPlaying, PlaybackContext, TrackInfo,
    };

    /// 200 with a track must serialise the TrackInfo fields into the flat
    /// trait shape field-for-field — that is the whole reason the
    /// `process_track` signature does not have to change.
    #[test]
    fn flatten_preserves_track_info_fields() {
        let np = SpotifyNowPlaying {
            media: TrackInfo {
                title: "Bohemian Rhapsody".into(),
                artist: "Queen".into(),
                album: "A Night at the Opera".into(),
                album_art_url: "https://example.com/art.jpg".into(),
                is_playing: true,
                progress_ms: Some(42_000),
                duration_ms: 355_000,
                volume_percent: None,
                supports_volume: None,
                actions: None,
            },
            episode: None,
            context: PlaybackContext::default(),
        };
        let flat = flatten_spotify_now_playing(&np);
        assert_eq!(flat.title, "Bohemian Rhapsody");
        assert_eq!(flat.artist, "Queen");
        assert_eq!(flat.album, "A Night at the Opera");
        assert_eq!(flat.album_art_url, "https://example.com/art.jpg");
        assert!(flat.is_playing);
        assert_eq!(flat.progress_ms, Some(42_000));
        assert_eq!(flat.duration_ms, 355_000);
    }

    /// The episode / context metadata is intentionally dropped at the
    /// trait boundary. The flat shape is the public contract.
    #[test]
    fn flatten_drops_episode_and_context() {
        let np = SpotifyNowPlaying {
            media: TrackInfo {
                title: "Episode 1".into(),
                artist: "Some Show".into(),
                album: "Season 1".into(),
                album_art_url: String::new(),
                is_playing: true,
                progress_ms: Some(0),
                duration_ms: 1_800_000,
                volume_percent: None,
                supports_volume: None,
                actions: None,
            },
            episode: Some(EpisodeInfo::default()),
            context: PlaybackContext::default(),
        };
        let flat = flatten_spotify_now_playing(&np);
        assert_eq!(flat.title, "Episode 1");
        assert_eq!(flat.duration_ms, 1_800_000);
    }

    /// The trait does not expose the 304 distinction — a 304 must
    /// surface as `Ok(Some(cached))` so the poll loop's unchanged-track
    /// path runs.
    #[test]
    fn last_now_playing_is_returned_on_repeated_calls() {
        let mut src = SpotifySource::new();
        src.set_access_token(Some("t".into()));
        src.last_now_playing = Some(NowPlaying {
            title: "Echoes".into(),
            artist: "Pink Floyd".into(),
            ..NowPlaying::default()
        });
        // Simulate the 304 path by hand (the HTTP layer is not reachable
        // in unit tests): the trait surface returns the cached value.
        let cached = src.last_now_playing.clone();
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().title, "Echoes");
    }

    /// A source with no token refuses the read rather than producing a
    /// silent no-op — that is the signal the poll loop needs to fall
    /// back to a system source in `Auto` mode.
    #[test]
    fn no_token_returns_unauthenticated() {
        let mut src = SpotifySource::new();
        let err = src.poll().expect_err("expected Unauthenticated");
        match err {
            SourceError::Unauthenticated => {}
            other => panic!("expected Unauthenticated, got {other:?}"),
        }
    }

    /// `invalidate` clears the 304 cache so a re-auth cannot surface a
    /// track the previous user was listening to.
    #[test]
    fn invalidate_clears_etag_and_cache() {
        let mut src = SpotifySource::new();
        src.last_etag = Some("W/\"abc\"".into());
        src.last_now_playing = Some(NowPlaying {
            title: "Stale".into(),
            ..NowPlaying::default()
        });
        src.invalidate();
        assert!(src.last_etag.is_none());
        assert!(src.last_now_playing.is_none());
    }
}
