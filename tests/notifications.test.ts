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
import type { AppConfig, TrackInfo } from '$lib/types';
import { configHydrated, configStore, defaultConfig, loadConfig } from '$lib/stores/config';
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
import Dashboard from '$lib/components/Dashboard.svelte';
import Layout from '../src/routes/+layout.svelte';

const TRACK_ACTIONS: NonNullable<TrackInfo['actions']> = {
  seeking: true,
  setting_volume: true,
  toggling_shuffle: true,
  toggling_repeat_context: true,
  toggling_repeat_track: true,
  skipping_prev: true,
  skipping_next: true,
  resuming: true,
  pausing: true,
  transferring_playback: true
};

/** A complete payload as emitted by the Rust `TrackInfo` event contract. */
function makeTrack(overrides: Partial<TrackInfo> = {}): TrackInfo {
  return {
    title: 'A Track',
    artist: 'An Artist',
    album: 'An Album',
    album_art_url: 'https://example.com/album-art.jpg',
    is_playing: true,
    progress_ms: 42_000,
    duration_ms: 240_000,
    volume_percent: 80,
    supports_volume: true,
    actions: TRACK_ACTIONS,
    ...overrides
  };
}

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

/**
 * Settle the fire-and-forget dispatch chain (a mocked async permission check
 * plus the send) without `waitFor`, which needs real timers and therefore
 * cannot be used while the clock is pinned.
 */
async function flush(): Promise<void> {
  for (let i = 0; i < 4; i++) await Promise.resolve();
  await tick();
  for (let i = 0; i < 4; i++) await Promise.resolve();
}

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
  // The seeded document stands in for a backend `load_config` result, so the
  // store counts as hydrated — #789 tests reset this explicitly.
  configHydrated.set(true);
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
const updateCalls = () => invoke.mock.calls.filter((c) => c[0] === 'update_config');

beforeEach(() => {
  window.localStorage.clear();
  listeners.all.length = 0;
  plugin.sendNotification.mockClear();
  plugin.isPermissionGranted.mockReset().mockResolvedValue(true);
  plugin.requestPermission.mockReset().mockResolvedValue('granted');
  // `isTauriRuntime` is computed at component init; without it the layout
  // registers no process-wide listeners at all.
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  invoke.mockReset().mockImplementation(async (cmd: string, args?: { config?: AppConfig; patch?: Partial<AppConfig> }) => {
    if (cmd === 'load_config') return storedConfig;
    if (cmd === 'save_config') {
      storedConfig = args?.config ?? storedConfig;
      return storedConfig;
    }
    if (cmd === 'update_config') {
      const patch = args?.patch;
      if (patch?.notifications) {
        storedConfig = {
          ...storedConfig,
          notifications: { ...storedConfig.notifications, ...patch.notifications }
        };
      }
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
    await notifyTrackChange(makeTrack());
    await notifySyncStopped();
    await notifyAuthRequired();
    await notifyUpdateStaged('4.7.0');
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it('never notifies when the OS denied permission, even with the class on', async () => {
    seed({ track_change: true });
    plugin.isPermissionGranted.mockResolvedValue(false);
    plugin.requestPermission.mockResolvedValue('denied');
    expect(await notifyTrackChange(makeTrack({ title: 'Denied Track' }))).toBe(false);
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
    expect(await notifyTrackChange(makeTrack({ title: 'A', artist: 'X', album: 'Y' }))).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
    expect(plugin.sendNotification.mock.calls[0][0]).toMatchObject({
      title: 'A',
      body: 'X — Y',
      id: 1001,
      group: 'presencejam-track-change'
    });

    // A different track inside the window is throttled, not queued.
    advanceClock(1_000);
    expect(await notifyTrackChange(makeTrack({ title: 'B', artist: 'X' }))).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);

    // The same track never notifies twice, even after the window elapses.
    advanceClock(9_000);
    expect(await notifyTrackChange(makeTrack({ title: 'A', artist: 'X' }))).toBe(false);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(1);

    // Once the window has elapsed the genuinely-current track gets through.
    advanceClock(10_000);
    expect(await notifyTrackChange(makeTrack({ title: 'B', artist: 'X' }))).toBe(true);
    expect(plugin.sendNotification).toHaveBeenCalledTimes(2);
    expect(plugin.sendNotification.mock.calls[1][0].id).toBe(1001);
  });

  it('says nothing for a track change when the class is off', async () => {
    seed({ sync_stopped: true });
    expect(await notifyTrackChange(makeTrack({ title: 'Class Off Track', artist: 'X' }))).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });

  it('ignores a payload with no title', async () => {
    seed({ track_change: true });
    expect(await notifyTrackChange(makeTrack({ title: '' }))).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });
});

describe('legacy opt-in migration (#675)', () => {
  it('folds the pre-4.7 boolean into track_change and removes the key', async () => {
    // `track_change` starts ON, so the legacy opt-out is what moves it.
    seed({ track_change: true, sync_stopped: true });
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, 'false');

    const next = await migrateLegacyNotificationPreference(storedConfig);

    expect(next.notifications.track_change).toBe(false);
    // The migrated value reaches the config, not just this session's store,
    // and the classes the legacy flag never governed are untouched.
    const saved = saveCalls()[0]?.[1] as { config: AppConfig };
    expect(saved.config.notifications).toMatchObject({ track_change: false, sync_stopped: true });
    expect(window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY)).toBeNull();
  });

  it('keeps the legacy opt-out when the save is rejected, and honours it next launch', async () => {
    seed({ track_change: true, sync_stopped: true });
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, 'false');
    // A full disk / read-only config dir: the write never lands.
    const save = invoke.getMockImplementation();
    invoke.mockImplementation(async (cmd: string, args?: { config?: AppConfig }) => {
      if (cmd === 'save_config') throw new Error('config.json is read-only');
      return save?.(cmd, args);
    });

    const failed = await migrateLegacyNotificationPreference(storedConfig);

    // The opt-out is live for this session…
    expect(failed.notifications.track_change).toBe(false);
    // …and the key is still there, because nothing was persisted: clearing it
    // here would destroy the user's opt-out for good.
    expect(window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY)).toBe('false');

    // Next launch: the config never learned about the migration, and the key
    // migrates again instead of the opt-out having vanished.
    invoke.mockImplementation(async (cmd: string, args?: { config?: AppConfig }) => {
      if (cmd === 'save_config') {
        storedConfig = args?.config ?? storedConfig;
        return storedConfig;
      }
      return save?.(cmd, args);
    });
    const relaunch = await migrateLegacyNotificationPreference(storedConfig);
    expect(relaunch.notifications.track_change).toBe(false);
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
    expect(updateCalls()).toEqual([]);
  });

  it('persists an enabled class through the partial patch and mirrors it to sibling windows', async () => {
    seed();

    expect(await setNotificationPreference('auth_required', true)).toBe(true);
    expect(get(notificationPreferences).auth_required).toBe(true);
    // Issue #789: a toggle merges one section — it never rewrites the whole
    // document from the in-memory copy.
    expect(saveCalls()).toEqual([]);
    const patched = updateCalls()[0]?.[1] as { patch: { notifications: NotificationPreferences } };
    expect(patched.patch.notifications).toEqual({ auth_required: true });
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
    // The config store converges too: a Save in the sibling window writes the
    // whole `AppConfig` it holds, so converging only the preferences would let
    // that Save write the class back to its old value on disk (#675 round 3).
    expect(get(configStore).notifications).toMatchObject({
      sync_stopped: true,
      track_change: false
    });

    // An unrelated key, or an unparsable payload, changes nothing.
    window.dispatchEvent(
      new StorageEvent('storage', { key: 'presencejam:theme', newValue: '"light"' })
    );
    window.dispatchEvent(
      new StorageEvent('storage', { key: NOTIFICATION_PREFS_MIRROR_KEY, newValue: '{oops' })
    );
    expect(get(notificationPreferences).sync_stopped).toBe(true);
    expect(get(configStore).notifications.sync_stopped).toBe(true);
    expect(convergeFromMirror('{oops')).toBe(false);
  });
});

describe('the toggle hydration gate (#789)', () => {
  it('leaves the document untouched and returns false when load_config rejects', async () => {
    seed();
    configHydrated.set(false);
    const before = structuredClone(storedConfig);
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'load_config') throw new Error('config.json is unreadable');
      return undefined;
    });

    expect(await setNotificationPreference('sync_stopped', true)).toBe(false);

    expect(saveCalls()).toEqual([]);
    expect(updateCalls()).toEqual([]);
    expect(storedConfig).toEqual(before);
    expect(get(notificationPreferences).sync_stopped).toBe(false);
    expect(get(configHydrated)).toBe(false);
  });

  it('stays silent after a failed load even with the compiled-in defaults on', async () => {
    seed({ sync_stopped: true });
    configHydrated.set(false);
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'load_config') throw new Error('config.json is unreadable');
      return undefined;
    });
    await loadConfig();
    expect(get(configHydrated)).toBe(false);
    expect(await notifySyncStopped()).toBe(false);
    expect(plugin.sendNotification).not.toHaveBeenCalled();
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

  it('notifies on the poller ending by itself, and not on a user-requested stop', async () => {
    seed({ sync_stopped: true });
    await mountLayout();

    // The user's own Pause Sync (commands::sync::stop_syncing) — reporting that
    // back as "PresenceJam stopped syncing on its own" would be false, and it
    // is the one action the user already knows about.
    await emit('sync-stopped', { self_terminated: false });
    expect(plugin.sendNotification).not.toHaveBeenCalled();

    // A payload in neither shape (an older backend, which emitted a unit) must
    // not be mislabelled either.
    await emit('sync-stopped', null);
    expect(plugin.sendNotification).not.toHaveBeenCalled();

    // The surprise: the poller gave up on its own (polling/state.rs).
    await emit('sync-stopped', { self_terminated: true });
    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(1));
    expect(plugin.sendNotification.mock.calls[0][0].title).toBe(t('notifications.syncStoppedTitle'));

    // S1's contract: exactly one `sync-stopped` per stop — so this is a
    // second stop, a second replacing notification rather than a duplicate.
    await emit('sync-stopped', { self_terminated: true });
    await waitFor(() => expect(plugin.sendNotification).toHaveBeenCalledTimes(2));
  });

  it('notifies for a dead Teams session while another view is on screen, and no track-change toast', async () => {
    seed({ auth_required: true });
    await mountLayout();
    // The Dashboard owns the `spotify-track-changed` listener, so it has to be
    // mounted for the "no track-change toast" half to mean anything. The track
    // class answers the same config the auth class does.
    const dash = render(Dashboard);
    await waitFor(() =>
      expect(listeners.all.filter((l) => l.event === 'spotify-track-changed').length).toBe(1)
    );

    // The throttle/dedup state is module-wide and shared with the tests above,
    // so pin the clock an hour past anything else in this file and use a track
    // no other test names — otherwise a silent track change would be
    // indistinguishable from a throttled one (which is exactly how the
    // original version of this assertion passed for the wrong reason).
    vi.useFakeTimers();
    vi.setSystemTime(new Date(Date.now() + 3_600_000));
    try {
      // The forced `invalid_grant` path (poll_once's `json!(null)` payload).
      await emit('teams-reconnect-required', null);
      await emit('spotify-track-changed', makeTrack({ title: 'Dead Session Track', artist: 'Zed' }));
      await flush();
      expect(plugin.sendNotification).toHaveBeenCalledTimes(1);
      const sent = plugin.sendNotification.mock.calls[0][0];
      expect(sent.title).toBe(t('notifications.authRequiredTitle'));
      expect(sent.id).toBe(1003);
    } finally {
      vi.useRealTimers();
    }
    dash.unmount();
  });

  it('does not notify for a reconnect the user just asked for', async () => {
    seed({ auth_required: true });
    await mountLayout();

    // Settings' "Reconnect Teams" (commands::onboarding::reconnect_teams) marks
    // its emit `user_initiated` — the user knows, and the layout navigates them
    // to the device-code flow anyway.
    await emit('teams-reconnect-required', { user_initiated: true });
    expect(plugin.sendNotification).not.toHaveBeenCalled();
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

    // The payloads that WOULD notify, so this can only be silence from the
    // class flags — not from the payload gate.
    await emit('sync-stopped', { self_terminated: true });
    await emit('teams-reconnect-required', null);
    await emit('update-stage-complete', { version: '4.7.0' });
    await emit('spotify-track-changed', makeTrack());

    expect(plugin.sendNotification).not.toHaveBeenCalled();
  });
});
