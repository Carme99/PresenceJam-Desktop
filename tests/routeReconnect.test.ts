/**
 * #932 (rework): the persist-banner reconnect routing lives in
 * `src/lib/utils/routeReconnect.ts` so a unit test can assert the routing
 * without re-implementing the ternary in JSX. The commit message on
 * `e833931` claimed a test existed for `routeReconnect` but
 * `grep -rn "routeReconnect" tests/` returned nothing — this spec closes
 * that nit by pinning the routing decision the Settings banner depends on.
 *
 * Contract:
 *   - `pickReconnectProvider('teams')` routes to the Teams reconnect.
 *   - `pickReconnectProvider('spotify')` routes to the Spotify reconnect.
 *   - An unknown provider falls back to Spotify (the Spotify banner is
 *     the newer of the two, #932 B1) so a future backend payload cannot
 *     crash the banner.
 */
import { describe, it, expect } from 'vitest';
import { pickReconnectProvider } from '$lib/utils/routeReconnect';

describe('pickReconnectProvider (#932)', () => {
  it('routes the Teams provider to the Teams reconnect', () => {
    expect(pickReconnectProvider('teams')).toBe('teams');
  });

  it('routes the Spotify provider to the Spotify reconnect', () => {
    expect(pickReconnectProvider('spotify')).toBe('spotify');
  });

  it('falls back to Spotify for an unknown provider so a future backend discriminator cannot crash the banner', () => {
    expect(pickReconnectProvider('unknown')).toBe('spotify');
    expect(pickReconnectProvider('')).toBe('spotify');
    expect(pickReconnectProvider('TEAMS')).toBe('spotify');
  });
});
