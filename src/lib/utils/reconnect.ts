import type { AuthPhase } from '$lib/stores/authFlow.svelte';

/**
 * Should the Reconnect view start a Spotify OAuth flow on mount?
 *
 * #530 routes a returning user whose stored session needs a sign-in to this
 * view. Opening a browser window is only justified when Spotify is the provider
 * that actually needs it:
 *
 * - `credentialsMissing` (Reconnect's `needsSpotify`): the Client ID or keychain
 *   secret is gone, so the OAuth flow cannot complete — the missing-credentials
 *   box sends the user to full setup instead.
 * - `spotifyConnected`: a healthy Spotify session with only Teams dead (the
 *   realistic "idle long enough for the Microsoft refresh token to lapse, not
 *   the Spotify one" case) must not get an unsolicited Spotify login window.
 *
 * A flow already `waiting`/`done` is left alone so a re-mount cannot restart it.
 */
export function shouldAutoStartSpotifyReconnect(input: {
  credentialsMissing: boolean;
  spotifyConnected: boolean;
  phase: AuthPhase;
}): boolean {
  if (input.credentialsMissing) return false;
  if (input.spotifyConnected) return false;
  return input.phase !== 'waiting' && input.phase !== 'done';
}
