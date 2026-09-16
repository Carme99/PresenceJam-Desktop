/**
 * #530 follow-up — Reconnect must not open a Spotify OAuth window for a
 * provider that is already connected. The boot route added by #530 lands
 * returning users here, so the auto-start decision needs to key off the real
 * Spotify session state (`SyncStatus.spotify_connected`), not just the presence
 * of stored credentials.
 */
import { describe, it, expect } from 'vitest';
import { shouldAutoStartSpotifyReconnect } from '$lib/utils/reconnect';

describe('shouldAutoStartSpotifyReconnect', () => {
  it('starts the flow when Spotify is the dead provider', () => {
    expect(
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: false,
        spotifyConnected: false,
        phase: 'idle'
      })
    ).toBe(true);
  });

  it('stays silent when only Teams needs a sign-in', () => {
    expect(
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: false,
        spotifyConnected: true,
        phase: 'idle'
      })
    ).toBe(false);
  });

  it('never starts a flow it cannot complete without credentials', () => {
    expect(
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: true,
        spotifyConnected: false,
        phase: 'idle'
      })
    ).toBe(false);
  });

  it('does not restart a flow that is already waiting or done', () => {
    expect(
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: false,
        spotifyConnected: false,
        phase: 'waiting'
      })
    ).toBe(false);
    expect(
      shouldAutoStartSpotifyReconnect({
        credentialsMissing: false,
        spotifyConnected: false,
        phase: 'done'
      })
    ).toBe(false);
  });
});
