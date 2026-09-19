//! Spotify playback-control helpers: the shared player-refresh policy plus
//! the one command that still needs an IPC surface.
//!
//! See issue #3.0-P3. The tray menu and the global-shortcut handlers dispatch
//! player actions from Rust through [`player_with_refresh_typed`] /
//! [`player_with_refresh`] with no frontend roundtrip;
//! `get_spotify_granted_scopes` powers the Settings one-time-reconnect banner
//! for the `user-modify-playback-state` scope.
//!
//! Issue #770: the seven callerless `#[tauri::command]` wrappers —
//! `playback_play`, `playback_pause`, `playback_next`, `playback_previous`,
//! `playback_transfer`, `get_playback_devices`, `get_playback_queue` — were
//! deleted. Nothing in `src/`, `tests/` or Rust invoked them (`generate_handler!`
//! registrations aside), so they were reachable IPC surface with no caller.
//! The refresh policy they wrapped stays exactly as it was: only the
//! unreachable commands are gone.

use crate::spotify::{
    decode_spotify_granted_scopes, is_token_expired, refresh_spotify_token, SpotifyApiError,
    SpotifyTokens,
};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.PLAYBACK]";

/// Never-re-auth token getter (issues #375/#428): return the stored Spotify
/// access token when it is still fresh; otherwise attempt one
/// `refresh_spotify_token` + CAS-commit + persist and return the fresh
/// token. Only a dead refresh token (`InvalidGrant`) surfaces the reconnect
/// wording — any other refresh failure falls back to the stale token so the
/// caller can attempt the call and classify the wire error itself.
fn refreshed_access_token(state: &Arc<crate::AppState>, app: &AppHandle) -> Result<String, String> {
    let current = state
        .tokens
        .spotify()
        .clone()
        .ok_or_else(|| "Spotify is not connected".to_string())?;
    if !is_token_expired(&current) {
        return Ok(current.access_token);
    }
    log::info!("{CMD} refreshed_access_token: Spotify token expired, refreshing...");
    match try_refresh_spotify_token(state, app, &current) {
        Ok(token) => Ok(token),
        Err(SpotifyApiError::InvalidGrant) => {
            Err("Spotify session invalid - reconnect from Settings".to_string())
        }
        Err(_) => Ok(current.access_token),
    }
}

/// One Spotify refresh + CAS-commit + persist. Returns the committed (or
/// concurrently-winning) access token; the typed `SpotifyApiError` lets the
/// caller keep reconnect wording for dead credentials only.
fn try_refresh_spotify_token(
    state: &Arc<crate::AppState>,
    app: &AppHandle,
    current: &SpotifyTokens,
) -> Result<String, SpotifyApiError> {
    let client_id = state
        .config
        .get()
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::peek_spotify_client_secret().unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(SpotifyApiError::Other(
            "Spotify credentials unavailable".to_string(),
        ));
    }
    let pre_refresh_access_token = current.access_token.clone();
    let new_tokens = refresh_spotify_token(current, &client_id, &client_secret)?;
    // CAS: only commit if state still holds the token we refreshed from.
    let committed = {
        let mut guard = state.tokens.spotify_mut();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!(
                "{CMD} try_refresh_spotify_token: state changed during refresh, keeping current tokens"
            );
            false
        }
    };
    if committed {
        if let Err(e) = crate::token_io::persist_tokens(state, app) {
            log::warn!(
                "{CMD} try_refresh_spotify_token: failed to persist refreshed spotify tokens: {}",
                e
            );
        }
        Ok(new_tokens.access_token)
    } else {
        Ok(state
            .tokens
            .spotify()
            .clone()
            .map(|t| t.access_token)
            .unwrap_or(pre_refresh_access_token))
    }
}

/// Issue #464: pure predicate for the `ExpiredToken` retry path — true when
/// a concurrent refresh already replaced the attempted token, so the retry
/// can use the current token directly instead of refreshing again.
fn concurrent_refresh_won(current_access_token: &str, attempted_token: &str) -> bool {
    current_access_token != attempted_token
}

/// Runs a Spotify player call with never-re-auth semantics (issues #375,
/// #428): proactive `refreshed_access_token`, then on `ExpiredToken` one
/// refresh + single retry before surfacing the *typed* failure. All player
/// commands — and the tray's click path — route through here, so the
/// refresh policy lives in one place.
///
/// Typed counterpart of [`player_with_refresh`]: the tray dispatches player
/// actions directly from Rust and matches on the `SpotifyApiError` to keep
/// its distinct no-active-device wording (issue #586), so the
/// classification has to survive the policy.
pub(crate) fn player_with_refresh_typed<T>(
    state: &Arc<crate::AppState>,
    app: &AppHandle,
    label: &str,
    call: impl Fn(&str) -> Result<T, SpotifyApiError>,
) -> Result<T, SpotifyApiError> {
    let token = refreshed_access_token(state, app).map_err(SpotifyApiError::Other)?;
    match call(&token) {
        Err(SpotifyApiError::ExpiredToken) => {
            log::info!("{CMD} {label}: ExpiredToken, attempting one refresh + retry");
            let current = state.tokens.spotify().clone();
            match current {
                Some(current_tokens)
                    if concurrent_refresh_won(&current_tokens.access_token, &token) =>
                {
                    // A concurrent refresh already won; retry once with it.
                    call(&current_tokens.access_token)
                }
                Some(current_tokens) => try_refresh_spotify_token(state, app, &current_tokens)
                    .and_then(|retry_token| call(&retry_token)),
                None => Err(SpotifyApiError::Other(
                    "Spotify is not connected".to_string(),
                )),
            }
        }
        result => result,
    }
}

/// [`player_with_refresh_typed`] with the user-facing wording applied — the
/// shape every command handler returns, and (issue #676) the entry point the
/// global-shortcut handler shares with them, so a shortcut cannot take a
/// second, divergent path to Spotify (issues #375, #428, #464).
pub(crate) fn player_with_refresh<T>(
    state: &Arc<crate::AppState>,
    app: &AppHandle,
    label: &str,
    call: impl Fn(&str) -> Result<T, SpotifyApiError>,
) -> Result<T, String> {
    player_with_refresh_typed(state, app, label, call).map_err(friendly_playback_error)
}

/// Maps a `SpotifyApiError` from a player call to a user-facing message,
/// with dedicated wording for the no-active-device and non-Premium cases.
fn friendly_playback_error(err: SpotifyApiError) -> String {
    match err {
        SpotifyApiError::NoActiveDevice => {
            "No active playback device - pick one from the tray Devices menu".to_string()
        }
        SpotifyApiError::NotPremium => "Playback control requires Spotify Premium".to_string(),
        SpotifyApiError::ExpiredToken => {
            "Spotify session expired - reconnect from Settings".to_string()
        }
        SpotifyApiError::InvalidGrant => {
            "Spotify session invalid - reconnect from Settings".to_string()
        }
        SpotifyApiError::RateLimited(retry_after) => match retry_after {
            Some(secs) => format!("Spotify is rate limiting requests (retry after {}s)", secs),
            None => "Spotify is rate limiting requests".to_string(),
        },
        // Issue #749: `Other` (no HTTP status), `Transient` (5xx) and `Http`
        // (an unclassified 4xx) each render their own wording through
        // `Display`, which already names the status without the response body.
        other => other.to_string(),
    }
}

/// Returns the scopes granted on the stored Spotify access token by
/// base64url-decoding its JWT payload (informational only — no signature
/// verification).
///
/// `None` means "unknown": the token is not a decodable JWT carrying a `scope`
/// claim, so a missing permission cannot be concluded from it. `Some(vec![])`
/// means the scope claim was present and empty. The Settings page uses this to
/// detect a missing `user-modify-playback-state` and show the one-time
/// reconnect banner, and must keep "unknown" apart from "absent" — reconnecting
/// cannot fix a decode failure (issue #973). See also issue #3.0-P3.
#[tauri::command]
pub fn get_spotify_granted_scopes(state: State<'_, Arc<crate::AppState>>) -> Option<Vec<String>> {
    log::debug!("{CMD} get_spotify_granted_scopes: ENTRY");
    state
        .tokens
        .spotify()
        .as_ref()
        .and_then(|tokens| decode_spotify_granted_scopes(&tokens.access_token))
}

#[cfg(test)]
mod tests {
    use super::{concurrent_refresh_won, friendly_playback_error};
    use crate::spotify::SpotifyApiError;

    /// Brace-counted body isolation (house style — never boundary anchors,
    /// which drift). Returns the byte range of the fn body starting at its
    /// opening `{`.
    fn fn_body<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("playback.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }

    /// Issue #770: the seven callerless playback wrappers must stay deleted —
    /// no definition left here and no `generate_handler!` registration left in
    /// `lib.rs`, or they become reachable IPC surface with no caller again.
    ///
    /// This replaces the issue #464 sweep that listed those seven commands:
    /// their refresh-policy coverage now lives in the policy tests below and
    /// in the tray/shortcut callers, which invoke `player_with_refresh_typed` /
    /// `player_with_refresh` directly with no IPC hop.
    #[test]
    fn test_callerless_playback_commands_stay_deleted() {
        let source = include_str!("playback.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("playback.rs has no #[cfg(test)] mod tests block");
        let registered = include_str!("../lib.rs");

        for name in [
            "playback_play",
            "playback_pause",
            "playback_next",
            "playback_previous",
            "playback_transfer",
            "get_playback_devices",
            "get_playback_queue",
        ] {
            assert!(
                !prod_source.contains(&format!("fn {name}(")),
                "{name} has no caller in src/, tests/ or Rust — it must stay \
                 deleted (issue #770)"
            );
            assert!(
                !registered.contains(&format!("commands::playback::{name}")),
                "{name} was deleted, so lib.rs must not register it (issue #770)"
            );
        }

        // The scopes reader stays, and stays out of the refresh policy: it
        // base64url-decodes the stored JWT and makes no API call.
        let scopes_body = fn_body(prod_source, "pub fn get_spotify_granted_scopes(");
        assert!(
            !scopes_body.contains("player_with_refresh("),
            "get_spotify_granted_scopes makes no API call and must not route \
             through player_with_refresh (issue #464)"
        );
        assert!(
            registered.contains("commands::playback::get_spotify_granted_scopes"),
            "the one playback command with a live caller must stay registered \
             (issue #770)"
        );
    }

    /// Issue #464: the shared policy itself must proactively refresh via
    /// `refreshed_access_token` and reactively retry via
    /// `try_refresh_spotify_token`, so the policy lives in one place.
    /// Issue #586: the policy is the typed core the tray also calls —
    /// `player_with_refresh` is only its friendly-message wrapper.
    ///
    /// Source-level by necessity: `player_with_refresh_typed` starts by reading
    /// a stored token and can pay for a network refresh, so a unit test cannot
    /// reach it — the injected-closure seam would change the signature the tray
    /// depends on. The scan pins that both refresh paths and the
    /// concurrent-refresh check stay inside the one policy.
    #[test]
    fn test_player_with_refresh_owns_both_refresh_paths() {
        let source = include_str!("playback.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("playback.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "fn player_with_refresh_typed<T>(");
        assert!(
            body.contains("refreshed_access_token("),
            "player_with_refresh must proactively refresh via refreshed_access_token"
        );
        assert!(
            body.contains("try_refresh_spotify_token("),
            "player_with_refresh must reactively refresh via try_refresh_spotify_token on ExpiredToken"
        );
        assert!(
            body.contains("concurrent_refresh_won("),
            "player_with_refresh must consult concurrent_refresh_won before refreshing"
        );
    }

    /// Unit tests for the extracted `concurrent_refresh_won` predicate.
    #[test]
    fn test_concurrent_refresh_won_compares_tokens() {
        assert!(
            concurrent_refresh_won("fresh-token", "stale-token"),
            "a replaced token means a concurrent refresh already won"
        );
        assert!(
            !concurrent_refresh_won("same-token", "same-token"),
            "an unchanged token means this call owns the refresh"
        );
        assert!(
            !concurrent_refresh_won("", ""),
            "two empty tokens are equal — no concurrent refresh happened"
        );
        assert!(
            concurrent_refresh_won("a", ""),
            "any difference counts as a concurrent refresh win"
        );
    }

    /// Issue #761: every player command returns this wording, so the two tiers
    /// a user can act on have to keep their instruction — "pick a device" is
    /// the fix for no-active-device, and the Premium line is the only place the
    /// account limitation is named. Executed rather than scanned: the scan
    /// above pins the wiring, this pins the strings.
    #[test]
    fn friendly_playback_error_names_the_action_for_every_tier() {
        assert_eq!(
            friendly_playback_error(SpotifyApiError::NoActiveDevice),
            "No active playback device - pick one from the tray Devices menu"
        );
        assert_eq!(
            friendly_playback_error(SpotifyApiError::NotPremium),
            "Playback control requires Spotify Premium"
        );
        // An expired token and an invalid grant both need a re-auth, but they
        // are not the same failure: one is recoverable by refreshing, the other
        // means the stored grant is dead.
        assert_eq!(
            friendly_playback_error(SpotifyApiError::ExpiredToken),
            "Spotify session expired - reconnect from Settings"
        );
        assert_eq!(
            friendly_playback_error(SpotifyApiError::InvalidGrant),
            "Spotify session invalid - reconnect from Settings"
        );
        // A 429 keeps the server's hint when it sent one.
        assert_eq!(
            friendly_playback_error(SpotifyApiError::RateLimited(Some(12))),
            "Spotify is rate limiting requests (retry after 12s)"
        );
        assert_eq!(
            friendly_playback_error(SpotifyApiError::RateLimited(None)),
            "Spotify is rate limiting requests"
        );
        // Anything else is surfaced verbatim rather than swallowed.
        assert_eq!(
            friendly_playback_error(SpotifyApiError::Other("player said no".to_string())),
            "player said no"
        );
    }
}
