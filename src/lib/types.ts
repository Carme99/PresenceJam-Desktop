/**
 * Shared frontend type definitions.
 *
 * These types mirror the Rust-side structs in `src-tauri/src/spotify.rs`
 * and `src-tauri/src/teams.rs`. They live here (not in `src/lib/stores/`)
 * because they're pure data shapes, not reactive state.
 */

export interface SpotifyTokens {
  access_token: string;
  refresh_token: string;
  expires_at: string;
}

export interface TrackInfo {
  title: string;
  artist: string;
  album: string;
  album_art_url: string;
  is_playing: boolean;
  progress_ms: number;
  duration_ms: number;
}

export interface TeamsTokens {
  access_token: string;
  // Spotify's refresh token is non-optional (Spotify always returns one on
  // the auth-code exchange), so it's `string` in both directions. Teams'
  // refresh token is `Option<String>` in Rust (`serde(default)` keeps the
  // field present, defaulting to `null` when the Microsoft endpoint doesn't
  // return one) — so on the wire it's `string | null`, not `string`.
  refresh_token: string | null;
  expires_at: string;
}

/**
 * Returned by the `get_sync_status` Tauri command. Mirrors
 * `commands::SyncStatus` in `src-tauri/src/commands.rs`.
 */
export interface SyncStatus {
  is_syncing: boolean;
  current_track: TrackInfo | null;
  spotify_connected: boolean;
  teams_connected: boolean;
}

/**
 * Returned by the `start_teams_auth_device_code` Tauri command. Mirrors
 * `teams::DeviceCodeResponse` in `src-tauri/src/teams.rs`.
 */
export interface DeviceCodeResponse {
  user_code: string;
  verification_url: string;
  device_code: string;
  interval: number;
  expires_in: number;
}

/**
 * Payload of the `log://log` event emitted by `tauri-plugin-log`. The
 * plugin's actual payload shape includes a `target` string and several
 * timestamp fields; we narrow defensively in the listener to the
 * fields the app actually uses. The full plugin type lives in
 * `@tauri-apps/plugin-log` if a strict import is preferred later.
 */
export interface LogPayload {
  level: number;
  message: string;
}