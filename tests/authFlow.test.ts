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
import { describe, it, expect, beforeEach, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import {
  authFlow,
  expiresAtFromResponse,
  formatCountdownMs,
  isCurrentTeamsPoll,
  isSafeHttpUrl,
  pollTeamsAuth,
  releaseTeamsPoll,
  resetAuthFlow,
  resetSpotifyAuthFlow,
  resetTeamsAuthFlow,
  setTeamsDeviceCode,
  teamsPollMutex,
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

describe('a superseded poll is not a successful sign-in (#933)', () => {
  it('adopts success only while the store still holds the polled device code', () => {
    setTeamsDeviceCode({
      userCode: 'AAAA-BBBB',
      verificationUrl: 'https://microsoft.com/devicelogin',
      deviceCode: 'code-a',
      interval: 5
    });
    expect(isCurrentTeamsPoll('code-a')).toBe(true);

    // A newer sign-in replaced the flow: the older poll's Ok must not be
    // adopted as success even though the backend resolved it.
    setTeamsDeviceCode({
      userCode: 'CCCC-DDDD',
      verificationUrl: 'https://microsoft.com/devicelogin',
      deviceCode: 'code-b',
      interval: 5
    });
    expect(isCurrentTeamsPoll('code-a')).toBe(false);
    expect(isCurrentTeamsPoll('code-b')).toBe(true);

    // Abandoning the flow clears the code outright, and an empty code is never
    // a current poll.
    resetTeamsAuthFlow();
    expect(isCurrentTeamsPoll('code-b')).toBe(false);
    expect(isCurrentTeamsPoll('')).toBe(false);
  });
});

/**
 * #785 — one `pollTeamsAuth` for every view. The four copies this replaces
 * already disagreed about one rule (the layout's was missing the #429 expiry
 * guard), so the tests pin the three rules that used to be per-copy, at the
 * level that now owns them:
 *   - a dead code is never polled;
 *   - while a poll is in flight, a second call is skipped — exactly one
 *     `poll_teams_auth` reaches the backend;
 *   - the mutex is released and the error phase set even when the invoke
 *     rejects, and the caller's callback runs only on real success.
 */
describe('pollTeamsAuth (#785)', () => {
  beforeEach(() => {
    invoke.mockReset();
    resetAuthFlow();
  });

  /** Store a live device code expiring in `expiresInMs` (negative = dead). */
  function storeCode(expiresInMs = 60_000, deviceCode = 'device-code') {
    setTeamsDeviceCode({
      userCode: 'ABCD-EFGH',
      verificationUrl: 'https://microsoft.com/devicelogin',
      deviceCode,
      interval: 5,
      expiresAt: Date.now() + expiresInMs
    });
  }

  it('never polls a code whose expiry has passed', async () => {
    storeCode(-1000);
    const onDone = vi.fn();

    await pollTeamsAuth(onDone);

    expect(invoke).not.toHaveBeenCalled();
    expect(onDone).not.toHaveBeenCalled();
    // The dead code must not have claimed the shared mutex either, or the
    // "Get a new code" flow would be blocked behind it.
    expect(teamsPollMutex.inFlight).toBe(false);
  });

  it('polls nothing when no device code is stored', async () => {
    resetTeamsAuthFlow();

    await pollTeamsAuth();

    expect(invoke).not.toHaveBeenCalled();
  });

  it('lets exactly one poll run while another is in flight', async () => {
    storeCode();
    let settlePoll: () => void = () => {};
    invoke.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          settlePoll = resolve;
        })
    );

    const firstDone = vi.fn();
    const secondDone = vi.fn();
    const first = pollTeamsAuth(firstDone);
    await pollTeamsAuth(secondDone);

    // The second call was skipped by the mutex, not queued behind the first.
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith('poll_teams_auth', {
      deviceCode: 'device-code',
      interval: 5
    });
    expect(secondDone).not.toHaveBeenCalled();

    settlePoll();
    await first;

    expect(firstDone).toHaveBeenCalledTimes(1);
    expect(authFlow.teams.phase).toBe('done');
    expect(teamsPollMutex.inFlight).toBe(false);
  });

  it('releases the mutex and records the error when the poll rejects', async () => {
    storeCode();
    invoke.mockRejectedValueOnce(new Error('no session'));
    const onDone = vi.fn();

    await pollTeamsAuth(onDone);

    expect(authFlow.teams.phase).toBe('error');
    expect(authFlow.teams.error).toContain('no session');
    expect(onDone).not.toHaveBeenCalled();
    // A stuck mutex would make every later Check now a silent no-op.
    expect(teamsPollMutex.inFlight).toBe(false);
  });

  it("ignores a superseded poll's success (#978)", async () => {
    storeCode();
    let settlePoll: () => void = () => {};
    invoke.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          settlePoll = resolve;
        })
    );

    const onDone = vi.fn();
    const pending = pollTeamsAuth(onDone);
    // The user asked for a fresh code while the old poll was in flight: the
    // backend discards the superseded poll's tokens but still resolves Ok.
    storeCode(60_000, 'replacement-device-code');

    settlePoll();
    await pending;

    // That Ok must not claim a connection, and the mutex must still be free.
    expect(onDone).not.toHaveBeenCalled();
    expect(authFlow.teams.phase).toBe('waiting');
    expect(teamsPollMutex.inFlight).toBe(false);
  });
});
