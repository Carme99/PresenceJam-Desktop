import type { View } from '$lib/stores/app';

/**
 * Which view the boot gate lands the user on (issue #530).
 *
 * An `is_onboarding_complete` verdict of `false` has two very different
 * meanings, and treating them alike is what made a returning user re-enter
 * their Spotify Client ID and Secret:
 *
 * - nothing is configured yet → the setup wizard (`onboarding`);
 * - a stored session needs a sign-in while the credentials are still in
 *   `config.json` + the OS keychain → `reconnect`, which re-runs the OAuth flow
 *   without asking for credentials the app already has.
 *
 * `hasSpotifyCredentials` mirrors `Reconnect.svelte`'s `needsSpotify`
 * derivation (client_id present AND keychain secret present).
 */
export function bootView(
  complete: boolean,
  hasSpotifyCredentials: boolean
): Extract<View, 'dashboard' | 'reconnect' | 'onboarding'> {
  if (complete) return 'dashboard';
  return hasSpotifyCredentials ? 'reconnect' : 'onboarding';
}
