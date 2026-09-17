/**
 * #547 / #549 / #551 / #670 — Dashboard presence state.
 *
 * `+page.svelte` destroys the Dashboard on every view switch, so these pin
 * the four behaviours that used to be lost or wrong across a remount:
 *   - presence events are captured by the always-mounted `+layout.svelte`
 *     listeners, so a status / gate / pause / stop that lands while another
 *     view is on screen still reaches the shared store (#670 / finding D2 —
 *     #547 moved the component state into the store but left the listeners
 *     on Dashboard, which is destroyed exactly when they are needed);
 *   - a mounting Dashboard seeds the status preview, the gate chip and the
 *     track card from `get_sync_status` (`last_posted_status` /
 *     `presence_gated` / `presence_paused`) with no event having fired
 *     (#670);
 *   - a pause keeps the track card in its paused state and only a genuine
 *     stop drops it (#670);
 *   - the notification opt-in is read live (#549), the availability chip
 *     renders localized copy from the structured flag and dismisses its
 *     "cleared" announcement (#551).
 *
 * Fails pre-fix: with `src/lib` + `src/routes` at the pre-#670 revision the
 * layout registers no presence listeners (`listenerReady('presence-paused')`
 * never settles), the hydrate assertions see "Not configured" with no gate
 * chip, and the paused card assertions see a playing card. Verified by
 * stashing the production half of the change and re-running this file.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
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
// `+layout.svelte` reads the window label to decide whether it is the main
// window; jsdom has no Tauri window.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({ label: 'main' }))
}));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.6.0') }));
// The layout mounts UpdatePrompt; a null answer keeps the banner out of the way.
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn(async () => null) }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn()
}));
// The detach store reaches for WebviewWindow at import time; Dashboard and the
// layout only read the pane flags. `svelte/store` is imported inside the
// factory because a hoisted `vi.mock` factory cannot use static imports.
vi.mock('$lib/stores/detach', async () => {
  const { writable } = await import('svelte/store');
  return {
    detachedPanes: writable({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {}),
    // The layout adopts the real window set on mount (#601).
    reconcileDetachedPanes: vi.fn(async () => {})
  };
});

import { invoke } from '@tauri-apps/api/core';
import { sendNotification } from '@tauri-apps/plugin-notification';
import Dashboard from '$lib/components/Dashboard.svelte';
import Layout from '../src/routes/+layout.svelte';
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

/** The `get_sync_status` shape, overridable per test. */
function syncStatus(overrides: Record<string, unknown> = {}) {
  return {
    is_syncing: true,
    current_track: TRACK,
    spotify_connected: true,
    teams_connected: true,
    last_posted_status: null,
    presence_gated: false,
    presence_paused: false,
    ...overrides
  };
}
let status = syncStatus();

/**
 * Deliver a Tauri event to every mounted listener and let Svelte flush, so an
 * assertion right after sees the rendered result. The copy matters: a handler
 * may unregister mid-iteration.
 */
async function emit(event: string, payload: unknown) {
  for (const l of [...listeners]) {
    if (l.event === event) l.fn({ payload });
  }
  await tick();
}

/** Wait until a listener for `event` is registered. */
async function listenerReady(event: string, count = 1) {
  await waitFor(() => {
    expect(listeners.filter((l) => l.event === event).length).toBe(count);
  });
}

/**
 * Mount the always-mounted shell (`+layout.svelte`) that `+page.svelte`
 * renders inside — the component that survives a view switch.
 */
async function mountShell() {
  const layout = render(Layout);
  await listenerReady('presence-paused');
  return layout;
}

/** Unmount a rendered component (leaving any other mount in place). */
async function unmount(component: { unmount: () => void }) {
  component.unmount();
  await tick();
}

beforeEach(() => {
  listeners.length = 0;
  // `isTauriRuntime` is computed at component init; the layout only binds
  // process-wide listeners inside the Tauri runtime.
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  presence.set({
    postedStatus: null,
    paused: false,
    pausedStatus: null,
    gated: false,
    gatedReason: '',
    availabilityListening: false,
    stopped: false,
    syncing: false,
    authPersistWarning: null,
    revision: 0
  });
  setNotificationsEnabled(false);
  i18n.set('en');
  sendNotificationMock.mockClear();
  invokeMock.mockReset();
  status = syncStatus();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === 'get_sync_status') return status;
    return undefined;
  });
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe('Presence events land while no Dashboard is mounted (#670 / D2)', () => {
  it('records a pause and a genuine stop from the always-mounted layout', async () => {
    await mountShell();
    await listenerReady('presence-cleared');
    await listenerReady('playback-state-changed');

    // No Dashboard and no other view is mounted: this is exactly the window in
    // which the pre-#670 listeners (registered by Dashboard itself) did not
    // exist, so every event landed nowhere.
    await emit('presence-paused', { status: '🎵 Paused' });
    expect(get(presence).paused).toBe(true);
    expect(get(presence).pausedStatus).toBe('🎵 Paused');
    expect(get(presence).stopped).toBe(false);

    await emit('playback-state-changed', { is_playing: false, track_key: 'track-1' });
    expect(get(presence).paused).toBe(true);

    await emit('presence-cleared', { timestamp: '2026-09-17T00:00:00Z' });
    expect(get(presence).stopped).toBe(true);
    expect(get(presence).paused).toBe(false);
    expect(get(presence).postedStatus).toBeNull();
  });

  it('keeps the gate reason for a Dashboard that mounts afterwards', async () => {
    // The backend still reports the suppression in the same session, so the
    // hydrated flag agrees with the event that was captured.
    status = syncStatus({ presence_gated: true });
    await mountShell();
    await emit('presence-gated', { reason: 'quiet-hours' });

    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.presence-chip')).not.toBeNull());
    expect(dash.container.querySelector('.presence-chip')?.textContent?.trim()).toBe(
      t('dashboard.presenceGatedQuietHours')
    );
  });
});

describe('Dashboard presence hydration (#547, #670)', () => {
  it('restores the status preview, the gate chip and the availability chip across a remount', async () => {
    status = syncStatus({
      last_posted_status: '🎵 An Artist - A Track 🎧',
      presence_gated: true
    });
    await mountShell();
    const first = render(Dashboard);
    await waitFor(() => expect(first.container.querySelector('.track-card')).not.toBeNull());

    await emit('presence-updated', { status: '🎵 An Artist - A Track 🎧' });
    await emit('presence-gated', { reason: 'in a call' });
    await emit('presence-availability-updated', { available: true, label: 'Listening (Available)' });
    expect(first.container.querySelector('.status-text')?.textContent).toBe('🎵 An Artist - A Track 🎧');
    expect(first.container.querySelector('.presence-chip')).not.toBeNull();
    expect(first.container.querySelector('.availability-chip')).not.toBeNull();

    // A view switch: Dashboard is destroyed and rebuilt from the shared state.
    await unmount(first);
    await waitFor(() => expect(listeners.filter((l) => l.event === 'spotify-track-changed').length).toBe(0));
    const second = render(Dashboard);
    await waitFor(() => expect(second.container.querySelector('.track-card')).not.toBeNull());

    expect(second.container.querySelector('.status-text')?.textContent).toBe('🎵 An Artist - A Track 🎧');
    expect(second.container.querySelector('.presence-chip')).not.toBeNull();
    expect(second.container.querySelector('.availability-chip')).not.toBeNull();
  });

  it('seeds the status preview and the gate chip from get_sync_status alone', async () => {
    status = syncStatus({
      last_posted_status: '🎵 An Artist - A Track 🎧',
      presence_gated: true
    });
    const { container } = render(Dashboard);

    await waitFor(() =>
      expect(container.querySelector('.status-text')?.textContent).toBe('🎵 An Artist - A Track 🎧')
    );
    expect(container.querySelector('.presence-chip')).not.toBeNull();
    expect(container.textContent).not.toContain(t('dashboard.statusNotConfigured'));
  });

  it('hydrates the paused track card instead of "Nothing playing"', async () => {
    status = syncStatus({
      current_track: { ...TRACK, is_playing: false },
      presence_paused: true,
      last_posted_status: '🎵 Paused'
    });
    const { container } = render(Dashboard);

    await waitFor(() => expect(container.querySelector('.track-card')).not.toBeNull());
    expect(container.querySelector('.not-playing')).toBeNull();
    expect(container.querySelector('.paused-indicator')?.textContent?.trim()).toContain(
      t('dashboard.paused')
    );
    expect(container.querySelector('.status-text')?.textContent).toBe('🎵 Paused');
  });
});

describe('A pause is not a stop (#670)', () => {
  it('keeps the track card on a pause and drops it only on a genuine stop', async () => {
    status = syncStatus({ last_posted_status: '🎵 An Artist - A Track 🎧' });
    await mountShell();
    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());
    expect(dash.container.querySelector('.paused-indicator')).toBeNull();

    // The Spotify client pauses: the poller reports a pause, not a stop.
    await emit('presence-paused', { status: '🎵 Paused' });
    await waitFor(() => expect(dash.container.querySelector('.paused-indicator')).not.toBeNull());
    expect(dash.container.querySelector('.track-card')).not.toBeNull();
    expect(dash.container.querySelector('.not-playing')).toBeNull();
    expect(dash.container.querySelector('.status-text')?.textContent).toBe('🎵 Paused');

    // The track ends: the poller clears the status. Only this drops the card.
    await emit('presence-cleared', { timestamp: '2026-09-17T00:00:00Z' });
    await waitFor(() => expect(dash.container.querySelector('.not-playing')).not.toBeNull());
    expect(dash.container.querySelector('.track-card')).toBeNull();
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
    await mountShell();
    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());
    i18n.set('de');

    await emit('presence-availability-updated', { available: true, label: 'Listening (Available)' });
    await waitFor(() =>
      expect(dash.container.querySelector('.availability-chip')?.textContent?.trim()).toBe(
        t('dashboard.availabilityListening')
      )
    );
    expect(dash.container.textContent).not.toContain('Listening (Available)');
  });

  it('dismisses the "cleared" announcement instead of leaving it up all session', async () => {
    await mountShell();
    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());

    vi.useFakeTimers();
    await emit('presence-availability-updated', { available: true });
    expect(dash.container.querySelector('.availability-chip')?.textContent?.trim()).toBe(
      t('dashboard.availabilityListening')
    );

    await emit('presence-availability-updated', { available: false, label: 'Availability cleared' });
    expect(dash.container.querySelector('.availability-chip')?.textContent?.trim()).toBe(
      t('dashboard.availabilityCleared')
    );

    await vi.advanceTimersByTimeAsync(5000);
    await tick();
    expect(dash.container.querySelector('.availability-chip')).toBeNull();
  });
});

describe('Dashboard tray refresh is claim-free (#592, #670)', () => {
  it('never sends the backend a tray-state snapshot it would ignore', async () => {
    await mountShell();
    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());

    // The two hot-path triggers: a sync-state transition (owned by the
    // always-mounted layout) and a track change (owned by Dashboard).
    await emit('sync-stopped', {});
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

describe('Hydration loses to a newer event (#670)', () => {
  it('discards a get_sync_status snapshot assembled before a pause already on screen', async () => {
    status = syncStatus({ last_posted_status: '🎵 An Artist - A Track 🎧' });
    await mountShell();

    // Hold the IPC answer so the pause event can land while it is in flight —
    // the snapshot genuinely predates the event that would otherwise revert it.
    const { promise: pendingStatus, resolve: answerStatus } = Promise.withResolvers<unknown>();
    let requested = false;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === 'get_sync_status') {
        requested = true;
        return pendingStatus;
      }
      return Promise.resolve(undefined);
    });

    const dash = render(Dashboard);
    await waitFor(() => expect(requested).toBe(true));
    await emit('presence-paused', { status: '🎵 Paused' });
    expect(get(presence).paused).toBe(true);

    answerStatus(syncStatus({ last_posted_status: '🎵 An Artist - A Track 🎧' }));
    await tick();

    // The pre-pause snapshot must not undo the pause the user can see.
    expect(get(presence).paused).toBe(true);
    expect(get(presence).pausedStatus).toBe('🎵 Paused');
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());
    expect(dash.container.querySelector('.paused-indicator')).not.toBeNull();
  });
});

describe('Auth persistence warning while another view owns the screen (#670 / D10)', () => {
  it('records the warning from the always-mounted layout', async () => {
    await mountShell();
    await emit('teams-auth-persist-warning', 'keychain locked: could not persist tokens');
    expect(get(presence).authPersistWarning).toBe('keychain locked: could not persist tokens');
  });
});

describe('Dashboard tray refresh is not refired by unrelated presence updates (#670)', () => {
  it('refreshes the tray only when the sync state actually changes', async () => {
    await mountShell();
    const dash = render(Dashboard);
    await waitFor(() => expect(dash.container.querySelector('.track-card')).not.toBeNull());
    await tick();
    const trayCallsBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === 'update_tray_menu_state'
    ).length;

    // Status / gate / pause events do not change the poller's run state.
    await emit('presence-updated', { status: '🎵 An Artist - A Track 🎧' });
    await emit('presence-gated', { reason: 'quiet-hours' });
    await emit('presence-paused', { status: '🎵 Paused' });

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === 'update_tray_menu_state').length
    ).toBe(trayCallsBefore);

    // A real sync transition still refreshes it.
    await emit('sync-stopped', {});
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === 'update_tray_menu_state').length
      ).toBe(trayCallsBefore + 1)
    );
  });
});
