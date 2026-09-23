/**
 * #615 — the registration/teardown race that every call site used to
 * hand-roll with its own `destroyed` flag.
 *
 * `listen()` resolves asynchronously, so a component that unmounts while a
 * registration is still in flight must release the subscription the moment
 * it settles. These pin the three guarantees the call sites now rely on:
 *   - a teardown called before `listen()` settles still releases it;
 *   - every subscription is released exactly once, whichever path wins;
 *   - handlers stop firing as soon as the teardown has been called (the
 *     unlisten round-trip is itself async).
 *
 * Fails pre-fix: `useAuthListeners` returned a promise, so there was no
 * teardown to call before the registrations settled at all.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

type Handler = (event: { payload: unknown }) => void;

interface Deferred {
  event: string;
  handler: Handler;
  unlistenCalls: number;
  resolve: () => void;
  reject: (err: unknown) => void;
}

// Referenced by the mock factory at call time (not at hoist time), so the
// arrays may live here — same pattern as tests/dashboard.test.ts.
let deferreds: Deferred[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: (event: string, handler: Handler) => {
    const { promise, resolve, reject } = Promise.withResolvers<() => void>();
    const entry: Deferred = {
      event,
      handler,
      unlistenCalls: 0,
      resolve: () => resolve(() => {
        entry.unlistenCalls++;
      }),
      reject
    };
    deferreds.push(entry);
    return promise;
  }
}));

import { useAuthListeners, useListenerTeardown } from '$lib/utils/useAuthListeners';

const AUTH_EVENTS = [
  'spotify-auth-complete',
  'spotify-auth-failed',
  'teams-auth-complete',
  'teams-auth-failed'
];

function noopListeners() {
  return {
    onSpotifyComplete: vi.fn(),
    onSpotifyFailed: vi.fn(),
    onTeamsComplete: vi.fn(),
    onTeamsFailed: vi.fn()
  };
}

/**
 * Drain pending microtasks. Five turns covers the deepest chain here:
 * settle-handler → `await registration` → `await unlisten()`.
 */
async function drain() {
  for (let turn = 0; turn < 5; turn++) await Promise.resolve();
}

beforeEach(() => {
  deferreds = [];
});

describe('useAuthListeners (#615)', () => {
  it('subscribes the four auth events, then the extras', () => {
    useAuthListeners(noopListeners(), [['spotify-secret-conflict', () => {}]]);
    expect(deferreds.map((d) => d.event)).toEqual([
      ...AUTH_EVENTS,
      'spotify-secret-conflict'
    ]);
  });

  it('releases a subscription that settles after the teardown ran', async () => {
    const teardown = useAuthListeners(noopListeners());
    // Unmount while all four `listen()` calls are still in flight.
    await teardown();
    expect(deferreds.map((d) => d.unlistenCalls)).toEqual([0, 0, 0, 0]);

    for (const d of deferreds) d.resolve();
    await drain();
    expect(deferreds.map((d) => d.unlistenCalls)).toEqual([1, 1, 1, 1]);
  });

  it('releases each subscription exactly once when settle and teardown race', async () => {
    const teardown = useAuthListeners(noopListeners());
    deferreds[0].resolve();
    await Promise.all([teardown(), teardown()]);
    for (const d of deferreds) d.resolve();
    await drain();

    expect(deferreds.map((d) => d.unlistenCalls)).toEqual([1, 1, 1, 1]);
  });

  it('stops calling handlers once torn down', async () => {
    const handlers = noopListeners();
    const teardown = useAuthListeners(handlers);
    for (const d of deferreds) d.resolve();
    await drain();

    deferreds[0].handler({ payload: null });
    deferreds[1].handler({ payload: 'spotify blew up' });
    expect(handlers.onSpotifyComplete).toHaveBeenCalledTimes(1);
    expect(handlers.onSpotifyFailed).toHaveBeenCalledWith('spotify blew up');

    await teardown();
    // The unlisten round-trip is async: an event delivered in that window
    // must not reach a handler whose component is gone.
    deferreds[2].handler({ payload: null });
    deferreds[3].handler({ payload: 'teams blew up' });
    expect(handlers.onTeamsComplete).not.toHaveBeenCalled();
    expect(handlers.onTeamsFailed).not.toHaveBeenCalled();
  });

  it('stops extra handlers after teardown and releases their subscriptions', async () => {
    const extraHandler = vi.fn();
    const teardown = useAuthListeners(noopListeners(), [
      ['spotify-secret-conflict', extraHandler]
    ]);
    for (const d of deferreds) d.resolve();
    await drain();

    deferreds[4].handler({ payload: { hasSpotifySecret: true } });
    expect(extraHandler).toHaveBeenCalledTimes(1);

    await teardown();
    // Keep the captured callback to model an event racing the async unlisten.
    deferreds[4].handler({ payload: { hasSpotifySecret: true } });
    expect(extraHandler).toHaveBeenCalledTimes(1);
    expect(deferreds.map((d) => d.unlistenCalls)).toEqual([1, 1, 1, 1, 1]);
  });

  it('neither rejects nor waits on in-flight siblings when one listen() fails', async () => {
    const teardown = useAuthListeners(noopListeners());
    deferreds[0].reject(new Error('listen refused'));
    await drain();

    // The failed registration has nothing to release, and the three siblings
    // are still in flight — the teardown must not hang waiting for them.
    await expect(teardown()).resolves.toBeUndefined();
    expect(deferreds[0].unlistenCalls).toBe(0);

    for (const d of deferreds.slice(1)) d.resolve();
    await drain();
    expect(deferreds.slice(1).map((d) => d.unlistenCalls)).toEqual([1, 1, 1]);
  });
});

describe('useListenerTeardown (#615)', () => {
  it('covers registrations added after the teardown ran', async () => {
    // Dashboard/+page register after their onMount IPC resolves, so the
    // teardown may already have run by the time `add()` is called.
    const teardown = useListenerTeardown();
    await teardown.dispose();

    let unlistenCalls = 0;
    const { promise, resolve } = Promise.withResolvers<() => void>();
    teardown.add(promise);

    resolve(() => {
      unlistenCalls++;
    });
    await drain();
    expect(unlistenCalls).toBe(1);
  });

  it('releases settled registrations on dispose', async () => {
    const teardown = useListenerTeardown();
    let first = 0;
    let second = 0;
    teardown.add(Promise.resolve(() => {
      first++;
    }));
    teardown.add(Promise.resolve(() => {
      second++;
    }));

    await teardown.dispose();
    expect([first, second]).toEqual([1, 1]);
  });
});
