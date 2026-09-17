/**
 * #675 (4.7.0) — the four desktop-notification classes.
 *
 * Pre-4.7 there was exactly one class (a track-change toast owned by
 * `Dashboard.svelte`) behind a single `localStorage` boolean, and the events
 * that matter most to a user who is not looking at the Dashboard —
 * `sync-stopped` from the poller's own exit, `teams-reconnect-required`,
 * `update-stage-complete` — produced nothing at all.
 *
 * These tests pin the four behaviours the slice is accepted on:
 *   - each class notifies exactly once per occurrence when it is enabled, and
 *     never when it is disabled;
 *   - the three layout-driven classes are registered by the always-mounted
 *     `+layout.svelte`, so a notification is not lost while another view is
 *     on screen;
 *   - the pre-4.7 opt-in is migrated into `config.notifications.track_change`
 *     exactly once and then superseded by the config;
 *   - the track-change class keeps its 5 s throttle and replace-in-place id.
 *
 * The box is headless, so "a toast appeared" is asserted at the seam where the
 * store hands the payload to `plugin-notification`'s `sendNotification`, with
 * the class flags coming from the real config store.
 *
 * Fails pre-fix: `notifySyncStopped`/`notifyAuthRequired`/`notifyUpdateStaged`
 * and the per-class preferences do not exist, the layout registers no
 * `update-stage-complete` listener, and `stores/notifications.ts` exports a
 * single boolean that only the Dashboard reads.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
import type { AppConfig } from '$lib/types';
import { configStore, defaultConfig } from '$lib/stores/config';
import {
  NOTIFICATION_CLASSES,
  NOTIFICATION_PREFS_MIRROR_KEY,
  NOTIFICATIONS_STORAGE_KEY,
  convergeFromMirror,
  migrateLegacyNotificationPreference,
  notificationPreferences,
  notifyAuthRequired,
  notifySyncStopped,
  notifyTrackChange,
  notifyUpdateStaged,
  setNotificationPreference,
  type NotificationPreferences
} from '$lib/stores/notifications';
import { t } from '$lib/i18n';
import Layout from '../src/routes/+layout.svelte';

type Listener = { event: string; fn: (e: { payload: unknown }) => void };

// `vi.hoisted` keeps one mock instance per module, so the fns the store and
// the layout call are the fns these tests assert on.
const plugin = vi.hoisted(() => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn()
}));
const invoke = vi.hoisted(() => vi.fn());
const listeners = vi.hoisted(() => ({ all: [] as Listener[] }));

vi.mock('@tauri-apps/plugin-notification', () => plugin);
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, fn: (e: { payload: unknown }) => void) => {
    const entry: Listener = { event, fn };
    listeners.all.push(entry);
    return () => {
      const i = listeners.all.indexOf(entry);
      if (i >= 0) listeners.all.splice(i, 1);
    };
  }),
  emitTo: vi.fn(async () => {})
}));
// `+layout.svelte` reads the window label to decide whether it is the main
// window; jsdom has no Tauri window.
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({ label: 'main' }))
}));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.7.0') }));
// The layout mounts UpdatePrompt; a null answer keeps the banner out of the way.
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn(async () => null) }));
// The detach store reaches for WebviewWindow at import time; the layout only
// reads the pane flags and adopts the window set.
vi.mock('$lib/stores/detach', async () => {
  const { writable } = await import('svelte/store');
  return {
    detachedPanes: writable({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {}),
    reconcileDetachedPanes: vi.fn(async () => {})
  };
});

let storedConfig: AppConfig;

/**
 * Adopt `classes` (every other class off) as both the config the backend
 * would return and the config the frontend holds.
 */
function seed(classes: Partial<NotificationPreferences> = {}): void {
  storedConfig = {
    ...structuredClone(defaultConfig),
    notifications: {
      track_change: false,
      sync_stopped: false,
      auth_required: false,
      update_staged: false,
      ...classes
    }
  };
  configStore.set(storedConfig);
}

/**
 * Fake time, strictly increasing across the whole file: the track-change
 * throttle compares against the timestamp of the previous dispatch, so each
 * test has to start after the test before it.
 */
let clock = Date.parse('2026-09-17T00:00:00Z');

function advanceClock(ms: number): void {
  clock += ms;
  vi.setSystemTime(new Date(clock));
}

async function emit(event: string, payload: unknown = null) {
  for (const l of [...listeners.all]) if (l.event === event) l.fn({ payload });
  await tick();
}

const saveCalls = () => invoke.mock.calls.filter((c) => c[0] === 'save_config');

beforeEach(() => {
  window.localStorage.clear();
  listeners.all.length = 0;
  plugin.sendNotification.mockClear();
  plugin.isPermissionGranted.mockReset().mockResolvedValue(true);
  plugin.requestPermission.mockReset().mockResolvedValue('granted');
  // `isTauriRuntime` is computed at component init; without it the layout
  // registers no process-wide listeners at all.
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  invoke.mockReset().mockImplementation(async (cmd: string, args?: { config?: AppConfig }) => {
    if (cmd === 'load_config') return storedConfig;
    if (cmd === 'save_config') {
      storedConfig = args?.config ?? storedConfig;
      return storedConfig;
    }
    if (cmd === 'get_sync_status') {
      return {
        is_syncing: false,
        current_track: null,
        spotify_connected: true,
        teams_connected: true
      };
    }
    return undefined;
  });
  seed();
});

describe('notification classes (#675)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    advanceClock(60_000);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('covers exactly the classes the config schema declares, all on by default', () => {
    const defaults = defaultConfig.notifications;
    expect([...NOTIFICATION_CLASSES].sort()).toEqual(Object.keys(defaults).sort());
    expect(Object.values(defaults).every((on) => on === true)).toBe(true);
  });

  it('dispatches one sync-stopped notification per occurrence, and none when disabled', async () => {
    seed({ sync_stopped: true });
    expect(await notifySyncStopped()).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
    expect(plugin.sendNotification.mock.calls[0][0]).toMatchObject({
      title: t('notifications.syncStoppedTitle'),
      body: t('notifications.syncStoppedBody'),
      id: 1002,
      group: 'presencejam-sync-stopped'
    });

    // A second occurrence is a second notification — there is no timer in
    // this class — replacing the first in place where id reuse is supported.
    expect(await notifySyncStopped()).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(2);

    seed();
    expect(await notifySyncStopped()).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(2);
  });

  it('dispatches one auth-required notification per occurrence, and none when disabled', async () => {
    seed({ auth_required: true });
    expect(await notifyAuthRequired()).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
    expect(plugin.sendNotification.mock.calls[0][0]).toMatchObject({
      title: t('notifications.authRequiredTitle'),
      body: t('notifications.authRequiredBody'),
      id: 1003,
      group: 'presencejam-auth-required'
    });

    seed();
    expect(await notifyAuthRequired()).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
  });

  it('dispatches one update-staged notification carrying the version, and none when disabled', async () => {
    seed({ update_staged: true });
    expect(await notifyUpdateStaged('4.7.0')).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
    const sent = plugin.sendNotification.mock.calls[0][0];
    expect(sent.title).toBe(t('notifications.updateStagedTitle'));
    expect(sent.body).toBe(t('notifications.updateStagedBody', { version: '4.7.0' }));
    expect(sent.body).toContain('4.7.0');
    expect(sent.id).toBe(1004);

    seed();
    expect(await notifyUpdateStaged('4.7.1')).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
  });

  it('stays completely silent when every class is off', async () => {
    seed();
    await notifyTrackChange({ title: 'A Track', artist: 'An Artist' });
    await notifySyncStopped();
    await notifyAuthRequired();
    await notifyUpdateStaged('4.7.0');
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it('never notifies when the OS denied permission, even with the class on', async () => {
    seed({ track_change: true });
    plugin.isPermissionGranted.mockResolvedValue(false);
    plugin.requestPermission.mockResolvedValue('denied');
    expect(await notifyTrackChange({ title: 'Denied Track', artist: 'An Artist' })).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });
});

describe('track-change class: the pre-4.7 behaviour, unchanged (#675)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    advanceClock(60_000);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('throttles to one notification per 5 s and replaces in place', async () => {
    seed({ track_change: true });
    expect(await notifyTrackChange({ title: 'A', artist: 'X', album: 'Y' })).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
    expect(plugin.sendNotification.mock.calls[0][0]).toMatchObject({
      title: 'A',
      body: 'X — Y',
      id: 1001,
      group: 'presencejam-track-change'
    });

    // A different track inside the window is throttled, not queued.
    advanceClock(1_000);
    expect(await notifyTrackChange({ title: 'B', artist: 'X' })).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);

    // The same track never notifies twice, even after the window elapses.
    advanceClock(9_000);
    expect(await notifyTrackChange({ title: 'A', artist: 'X' })).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);

    // Once the window has elapsed the genuinely-current track gets through.
    advanceClock(10_000);
    expect(await notifyTrackChange({ title: 'B', artist: 'X' })).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(2);
    expect(plugin.sendNotification.mock.calls[1][0].id).toBe(1001);
  });

  it('says nothing for a track change when the class is off', async () => {
    seed({ sync_stopped: true });
    expect(await notifyTrackChange({ title: 'Class Off Track', artist: 'X' })).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it('ignores a payload with no title', async () => {
    seed({ track_change: true });
    expect(await notifyTrackChange({ artist: 'X' })).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });
});

describe('legacy opt-in migration (#675)', () => {
  it('folds the pre-4.7 boolean into track_change and removes the key', async () => {
    seed({ sync_stopped: true });
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, 'false');

    const next = await migrateLegacyNotificationPreference(storedConfig);

    expect(next.notifications.track_change).toBe(false);
    // The migrated value reaches the config, not just this session's store,
    // and the classes the legacy flag never governed are untouched.
    const saved = saveCalls()[0]?.[1] as { config: AppConfig };
    expect(saved.config.notifications).toMatchObject({ track_change: false, sync_stopped: true });
    expect(window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY)).toBeNull();
  });

  it("honours a legacy 'true' opt-in too", async () => {
    seed();
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, 'true');

    const next = await migrateLegacyNotificationPreference(storedConfig);

    expect(next.notifications.track_change).toBe(true);
    expect(window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY)).toBeNull();
  });

  it('is honoured once: afterwards the config is the only source of truth', async () => {
    seed();
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, 'false');
    await migrateLegacyNotificationPreference(storedConfig);
    const savesAfterMigration = saveCalls().length;

    // A later config (another window, or the next launch) says every class is
    // on; there is nothing left to migrate, so it must stand.
    const later: AppConfig = {
      ...structuredClone(defaultConfig),
      notifications: {
        track_change: true,
        sync_stopped: true,
        auth_required: true,
        update_staged: true
      }
    };
    const next = await migrateLegacyNotificationPreference(later);

    expect(next).toBe(later);
    expect(saveCalls().length).toBe(savesAfterMigration);
  });

  it('writes nothing when there is no legacy key', async () => {
    seed({ auth_required: true });
    const next = await migrateLegacyNotificationPreference(storedConfig);
    expect(next).toBe(storedConfig);
    expect(saveCalls()).toEqual([]);
  });
});

describe('the Settings toggle path (#675)', () => {
  it('refuses to enable a class the OS will not allow, without persisting it', async () => {
    seed();
    plugin.isPermissionGranted.mockResolvedValue(false);
    plugin.requestPermission.mockResolvedValue('denied');

    expect(await setNotificationPreference('sync_stopped', true)).toBe(false);
    expect(get(notificationPreferences).sync_stopped).toBe(false);
    expect(saveCalls()).toEqual([]);
  });

  it('persists an enabled class to the config and mirrors it to sibling windows', async () => {
    seed();

    expect(await setNotificationPreference('auth_required', true)).toBe(true);
    expect(get(notificationPreferences).auth_required).toBe(true);
    const saved = saveCalls()[0]?.[1] as { config: AppConfig };
    expect(saved.config.notifications.auth_required).toBe(true);
    expect(
      JSON.parse(window.localStorage.getItem(NOTIFICATION_PREFS_MIRROR_KEY) ?? '{}')
    ).toMatchObject({ auth_required: true });
  });

  it('converges on the value a sibling webview mirrored in-session', () => {
    seed({ track_change: true });
    const mirror = JSON.stringify({
      track_change: false,
      sync_stopped: true,
      auth_required: false,
      update_staged: false
    });

    window.dispatchEvent(
      new StorageEvent('storage', { key: NOTIFICATION_PREFS_MIRROR_KEY, newValue: mirror })
    );
    expect(get(notificationPreferences).sync_stopped).toBe(true);

    // An unrelated key, or an unparsable payload, changes nothing.
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'presencejam:theme', newValue: '"light"' })
    );
    window.dispatchEvent(
      new StorageEvent('storage', { key: NOTIFICATION_PREFS_MIRROR_KEY, newValue: '{oops' })
    );
    expect(get(notificationPreferences).sync_stopped).toBe(true);
    expect(convergeFromMirror('{oops')).toBe(false);
  });
});

describe('the always-mounted layout dispatches the new classes (#675)', () => {
  afterEach(() => {
    cleanup();
  });

  async function mountLayout() {
    const result = render(Layout);
    await waitFor(() =>
      expect(listeners.all.filter((l) => l.event === 'sync-stopped').length).toBe(1)
    );
    return result;
  }

  it('notifies once per stop when the poller stops, and not when the class is off', async () => {
    seed({ sync_stopped: true });
    await mountLayout();

    await emit('sync-stopped');
    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(1));
    expect(plugin.sendNotification.mock.calls[0][0].title).toBe(t('notifications.syncStoppedTitle'));

    // S1's contract: exactly one `sync-stopped` per stop — so this is a
    // second stop, a second replacing notification rather than a duplicate.
    await emit('sync-stopped');
    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(2));
  });

  it('notifies for a Teams reconnect while another view is on screen, with the other classes off', async () => {
    seed({ auth_required: true });
    await mountLayout();

    // The forced `invalid_grant` path: only auth_required is enabled, so the
    // one occurrence produces exactly one toast — and no track-change toast.
    await emit('teams-reconnect-required');
    await emit('spotify-track-changed', { title: 'A Track', artist: 'An Artist' });

    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(1));
    const sent = plugin.sendNotification.mock.calls[0][0];
    expect(sent.title).toBe(t('notifications.authRequiredTitle'));
    expect(sent.id).toBe(1003);
  });

  it('notifies when an update finishes staging, carrying the staged version', async () => {
    seed({ update_staged: true });
    await mountLayout();

    await emit('update-stage-complete', { version: '4.7.0' });
    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(1));
    const sent = plugin.sendNotification.mock.calls[0][0];
    expect(sent.title).toBe(t('notifications.updateStagedTitle'));
    expect(sent.body).toContain('4.7.0');
  });

  it('stays silent for those events when the classes are off', async () => {
    seed();
    await mountLayout();

    await emit('sync-stopped');
    await emit('teams-reconnect-required');
    await emit('update-stage-complete', { version: '4.7.0' });
    await emit('spotify-track-changed', { title: 'A Track', artist: 'An Artist' });

    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });
});
