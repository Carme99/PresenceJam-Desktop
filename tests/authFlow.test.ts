/**
 * #617 — `authFlow.svelte.ts` had no coverage at all. These pin the four
 * things in it that are user- or security-visible:
 *   - the #410 XSS guard that decides whether the device-code verification
 *     URL may render as a clickable anchor;
 *   - the `m:ss` expiry countdown (#429);
 *   - the `expires_in` → epoch-ms derivation;
 *   - the shared `poll_teams_auth` mutex (#396), whose whole purpose is that
 *     only one long-blocking poll runs at a time, and the per-flow resets
 *     (#421) that must not clear the sibling flow.
 */
import { describe, it, expect, beforeEach } from 'vitest';
import {
  authFlow,
  expiresAtFromResponse,
  formatCountdownMs,
  isSafeHttpUrl,
  releaseTeamsPoll,
  resetSpotifyAuthFlow,
  resetTeamsAuthFlow,
  setTeamsDeviceCode,
  tryAcquireTeamsPoll
} from '$lib/stores/authFlow.svelte';

describe('isSafeHttpUrl (#410)', () => {
  it('accepts an https URL and nothing else', () => {
    expect(isSafeHttpUrl('https://microsoft.com/devicelogin')).toBe(true);
    expect(isSafeHttpUrl('https://login.microsoftonline.com/common/oauth2/deviceauth')).toBe(true);
  });

  it('refuses every non-https scheme — including script-bearing ones', () => {
    expect(isSafeHttpUrl('http://microsoft.com/devicelogin')).toBe(false);
    expect(isSafeHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isSafeHttpUrl('data:text/html,<script>alert(1)</script>')).toBe(false);
    expect(isSafeHttpUrl('presencejam://callback')).toBe(false);
    expect(isSafeHttpUrl('not a url')).toBe(false);
    expect(isSafeHttpUrl('')).toBe(false);
  });
});

describe('formatCountdownMs (#429)', () => {
  it('formats as m:ss, rounding up so the code never shows 0:00 while valid', () => {
    expect(formatCountdownMs(9_400)).toBe('0:10');
    expect(formatCountdownMs(9_000)).toBe('0:09');
    expect(formatCountdownMs(59_400)).toBe('1:00');
    expect(formatCountdownMs(61_000)).toBe('1:01');
    expect(formatCountdownMs(600_000)).toBe('10:00');
  });

  it('clamps an expired or negative span to 0:00', () => {
    expect(formatCountdownMs(0)).toBe('0:00');
    expect(formatCountdownMs(-1)).toBe('0:00');
    expect(formatCountdownMs(-90_000)).toBe('0:00');
  });
});

describe('expiresAtFromResponse (#429)', () => {
  it('derives epoch-ms from expires_in (seconds)', () => {
    const before = Date.now();
    const expiresAt = expiresAtFromResponse({ expires_in: 15 }) ?? 0;
    expect(expiresAt - before).toBeGreaterThanOrEqual(15_000);
    expect(expiresAt - before).toBeLessThan(16_000);
  });

  it('returns null when the backend omitted the expiry', () => {
    expect(expiresAtFromResponse({})).toBeNull();
    expect(expiresAtFromResponse({ expires_in: null })).toBeNull();
  });
});

describe('teams poll mutex (#396)', () => {
  beforeEach(() => {
    releaseTeamsPoll();
  });

  it('admits one poll at a time', () => {
    expect(tryAcquireTeamsPoll()).toBe(true);
    expect(tryAcquireTeamsPoll()).toBe(false);
    releaseTeamsPoll();
    expect(tryAcquireTeamsPoll()).toBe(true);
    releaseTeamsPoll();
  });
});

describe('teams poll mutex under a restart (#933)', () => {
  beforeEach(() => {
    releaseTeamsPoll();
  });

  it('lets the restarted sign-in in, and keeps the abandoned release harmless', () => {
    // The abandoned poll is still awaiting its invoke.
    expect(tryAcquireTeamsPoll()).toBe(true);

    // The user starts over: the flow is reset, and the restarted sign-in must
    // not be skipped behind the abandoned invoke.
    resetTeamsAuthFlow();
    expect(tryAcquireTeamsPoll()).toBe(true);

    // The abandoned poll finally resolves. Its release must not free the mutex
    // the newer flow is still holding.
    releaseTeamsPoll();
    expect(tryAcquireTeamsPoll()).toBe(false);

    // And when the newer flow resolves, the mutex is free again.
    releaseTeamsPoll();
    expect(tryAcquireTeamsPoll()).toBe(true);
    releaseTeamsPoll();
  });
});

describe('per-flow resets (#421)', () => {
  it('clearing the Spotify flow leaves the Teams device code in place', () => {
    setTeamsDeviceCode({
      userCode: 'ABCD-EFGH',
      verificationUrl: 'https://microsoft.com/devicelogin',
      deviceCode: 'device-code',
      interval: 5,
      expiresAt: 1_700_000_000_000
    });

    resetSpotifyAuthFlow();

    expect(authFlow.teams.userCode).toBe('ABCD-EFGH');
    expect(authFlow.teams.verificationUrl).toBe('https://microsoft.com/devicelogin');
    expect(authFlow.teams.deviceCode).toBe('device-code');
    expect(authFlow.teams.expiresAt).toBe(1_700_000_000_000);
  });
});
