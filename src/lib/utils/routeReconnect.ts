/**
 * Pick which provider's reconnect button to surface on the
 * `*-auth-persist-warning` banner in Settings (#932, rework).
 *
 * The `provider` argument is the typed discriminator the backend emits on
 * `teams-auth-persist-warning` / `spotify-auth-persist-warning`. An unknown
 * value falls back to the Spotify reconnect (the Spotify banner is the newer
 * of the two, #932 B1) so a future backend payload cannot crash the banner.
 *
 * Extracted from `Settings.svelte`'s component-local `routeReconnect` so a
 * Vitest spec can assert the routing without re-implementing the ternary in
 * JSX (the commit message on `e833931` claimed such a test exists but
 * `grep -rn "routeReconnect" tests/` returned nothing; this util + the
 * matching test close that nit).
 */

export type ReconnectProvider = 'teams' | 'spotify' | (string & {});

export type ReconnectTarget = 'teams' | 'spotify';

export function pickReconnectProvider(provider: ReconnectProvider): ReconnectTarget {
  return provider === 'teams' ? 'teams' : 'spotify';
}
