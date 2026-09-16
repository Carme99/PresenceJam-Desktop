/**
 * #547 / #549 / #551 — Dashboard presence state.
 *
 * `+page.svelte` destroys Dashboard on every view switch, so these pin the
 * three behaviors that used to be lost or wrong across a remount:
 *   - the Teams status preview and the presence-gate chip hydrate from the
 *     shared presence store instead of resetting to "Not configured" (#547);
 *   - the notification opt-in is read live, so a flip made elsewhere (the
 *     detached Settings window) reaches the already-mounted Dashboard (#549);
 *   - the availability chip renders localized copy from the payload's
 *     structured flag and dismisses its "cleared" announcement (#551).
 *
 * Fails pre-fix: the preview is "Not configured" after the remount, the
 * track-change notification never fires, and the chip renders the raw
 * backend label forever.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { Mock } from 'vitest';

type Listener = { event: string; fn: (e: { payload: unknown }) => void };
const listeners: Listener[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, fn: (e: unknown) => void) => {
    const entry = { event, fn: fn as Listener['fn'] };
    listeners.push(entry);
    return () => {
      const i = listeners.indexOf(entry);
      if (i >= 0) listeners.splice(i, 1);
    };
  })
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn()
}));
// The detach store reaches for WebviewWindow at import time; Dashboard only
// reads the pane flags. `svelte/store` is imported inside the factory because
// a hoisted `vi.mock` factory cannot use static imports.
vi.mock('$lib/stores/detach', async () => {
  const { writable } = await import('svelte/store');
  return {
    detachedPanes: writable({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {})
  };
});

import { invoke } from '@tauri-apps/api/core';
import { sendNotification } from '@tauri-apps/plugin-notification';
import Dashboard from '$lib/components/Dashboard.svelte';
import { presence } from '$lib/stores/presence';
import { setNotificationsEnabled } from '$lib/stores/notifications';
import { i18n, t } from '$lib/i18n';

const invokeMock = invoke as unknown as Mock;
const sendNotificationMock = sendNotification as unknown as Mock;

const TRACK = {
  id: 'track-1',
  title: 'A Track',
  artist: 'An Artist',
  album: 'An Album',
  album_art_url: null,
  duration_ms: 200000,
  progress_ms: 1000,
  is_playing: true
};

/**
 * Deliver a Tauri event to the mounted Dashboard and let Svelte flush, so an
 * assertion right after sees the rendered result. The copy matters: a handler
 * may unregister mid-iteration.
 */
async function emit(event: string, payload: unknown) {
  for (const l of [...listeners]) {
    if (l.event === event) l.fn({ payload });
  }
  await tick();
}

/** Wait until Dashboard has registered its listener for `event`. */
async function listenerReady(event: string, count = 1) {
  await waitFor(() => {
    expect(listeners.filter((l) => l.event === event).length).toBe(count);
  });
}

beforeEach(() => {
  listeners.length = 0;
  presence.set({ postedStatus: null, gated: false, availabilityListening: false });
  setNotificationsEnabled(false);
  i18n.set('en');
  sendNotificationMock.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_sync_status') {
      return {
        is_syncing: true,
        current_track: TRACK,
        spotify_connected: true,
        teams_connected: true
      };
    }
    return undefined;
  });
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('Dashboard presence hydration (#547)', () => {
  it('restores the status preview, the gate chip and the availability chip on remount', async () => {
    const first = render(Dashboard);
    await listenerReady('presence-updated');
    await listenerReady('presence-availability-updated');

    const preview = () => first.container.querySelector('.status-text')?.textContent;
    expect(preview()).toBe(t('dashboard.statusNotConfigured'));

    await emit('presence-updated', { status: '🎵 An Artist - A Track 🎧' });
    await emit('presence-gated', { reason: 'in a call' });
    await emit('presence-availability-updated', { available: true, label: 'Listening (Available)' });
    expect(preview()).toBe('🎵 An Artist - A Track 🎧');
    expect(first.container.querySelector('.presence-chip')).not.toBeNull();
    expect(first.container.querySelector('.availability-chip')).not.toBeNull();

    // A view switch: Dashboard is destroyed and rebuilt from the store.
    first.unmount();
    const second = render(Dashboard);
    await listenerReady('presence-updated');

    expect(second.container.querySelector('.status-text')?.textContent).toBe(
      '🎵 An Artist - A Track 🎧'
    );
    expect(second.container.querySelector('.presence-chip')).not.toBeNull();
    expect(second.container.querySelector('.availability-chip')).not.toBeNull();
  });

  it('lets the events stay authoritative after the hydration', async () => {
    const first = render(Dashboard);
    await listenerReady('presence-updated');
    await emit('presence-updated', { status: 'first' });
    first.unmount();

    const second = render(Dashboard);
    await listenerReady('presence-updated');
    await emit('presence-updated', { status: 'second' });
    expect(second.container.querySelector('.status-text')?.textContent).toBe('second');

    await emit('presence-gated', { reason: 'presenting' });
    expect(second.container.querySelector('.presence-chip')).not.toBeNull();
    // A real write means the gate stopped suppressing — the chip clears.
    await emit('presence-updated', { status: 'third' });
    expect(second.container.querySelector('.presence-chip')).toBeNull();
  });
});

describe('Dashboard notification opt-in (#549)', () => {
  it('honours a toggle flipped after the Dashboard is already mounted', async () => {
    const { container } = render(Dashboard);
    await listenerReady('spotify-track-changed');
    await waitFor(() => expect(container.querySelector('.track-card')).not.toBeNull());

    // What the detached Settings window does: flip the shared opt-in.
    setNotificationsEnabled(true);
    await emit('spotify-track-changed', TRACK);
    await waitFor(() => expect(sendNotificationMock).toHaveBeenCalledTimes(1));
    expect(sendNotificationMock.mock.calls[0][0].title).toBe('A Track');
  });

  it('stays silent while the opt-in is off', async () => {
    render(Dashboard);
    await listenerReady('spotify-track-changed');
    await emit('spotify-track-changed', TRACK);
    expect(sendNotificationMock).not.toHaveBeenCalled();
  });
});

describe('Dashboard availability chip (#551)', () => {
  it('renders localized copy from the structured flag, not the backend label', async () => {
    const { container } = render(Dashboard);
    await listenerReady('presence-availability-updated');
    i18n.set('de');

    await emit('presence-availability-updated', { available: true, label: 'Listening (Available)' });
    expect(container.querySelector('.availability-chip')?.textContent?.trim()).toBe(
      t('dashboard.availabilityListening')
    );
    expect(container.textContent).not.toContain('Listening (Available)');
  });

  it('dismisses the "cleared" announcement instead of leaving it up all session', async () => {
    const { container } = render(Dashboard);
    await listenerReady('presence-availability-updated');

    vi.useFakeTimers();
    await emit('presence-availability-updated', { available: false, label: 'Availability cleared' });
    expect(container.querySelector('.availability-chip')?.textContent?.trim()).toBe(
      t('dashboard.availabilityCleared')
    );

    await vi.advanceTimersByTimeAsync(5000);
    await tick();
    expect(container.querySelector('.availability-chip')).toBeNull();
  });
});

describe('Dashboard tray refresh is claim-free (#592)', () => {
  it('never sends the backend a tray-state snapshot it would ignore', async () => {
    const { container } = render(Dashboard);
    await listenerReady('sync-started');
    await listenerReady('spotify-track-changed');
    await waitFor(() => expect(container.querySelector('.track-card')).not.toBeNull());

    // The two hot-path triggers: a sync toggle and a track change.
    await emit('sync-started', {});
    await emit('spotify-track-changed', TRACK);

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === 'update_tray_menu_state').length
      ).toBeGreaterThanOrEqual(2)
    );

    // The Rust command derives is_syncing/current_track from AppState; any
    // second argument would be a frontend claim riding a contract that no
    // longer exists.
    for (const call of invokeMock.mock.calls) {
      if (call[0] === 'update_tray_menu_state') expect(call[1]).toBeUndefined();
    }
  });
});
