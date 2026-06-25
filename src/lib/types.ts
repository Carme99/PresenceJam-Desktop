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