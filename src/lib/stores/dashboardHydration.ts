/**
 * Issue #888 — the Dashboard's mount-time hydration, hoisted into a store.
 *
 * `+page.svelte` destroys the Dashboard on every view switch, so anything the
 * component awaits in `onMount` is paid again on every return to it. Two of
 * those awaits are expensive and both answer a question the app already asked:
 *
 *   - `load_config` — a config-file read whose answer includes an OS keychain
 *     probe for the Spotify client secret, and
 *   - `get_sync_status` — a snapshot that has to take the four polling state
 *     locks on the Rust side.
 *
 * Cross-component state lives in a store (AGENTS.md §5), so the snapshot and
 * the "when did we last ask" clock live here rather than in the component that
 * happens to be mounted. The always-mounted `+layout.svelte` warms the cache at
 * startup and the Dashboard joins it; a remount inside
 * {@link HYDRATION_TTL_MS} resolves from memory instead of issuing IPC.
 *
 * The TTL is what keeps the tray snooze honest: the tray writes `snooze_until`
 * straight into `config.json`, with no event and no webview involvement, so a
 * cache that never expired would show a Dashboard that never learns about it.
 * `+page.svelte` therefore calls {@link resetDashboardHydration} on `tray-click`
 * — the one signal the webview gets that the tray was just used — which makes
 * the very next mount re-read.
 */
import { get, writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { devLog } from '$lib/utils/dev';
import { loadConfig } from './config';
import { hydrate, presence } from './presence';
import type { SyncStatus } from '$lib/types';

/**
 * How long a hydrated snapshot is served from memory. Long enough that the
 * Dashboard ↔ Settings ↔ Logs round trip a user actually performs costs no IPC,
 * short enough that a config write made outside the webview is picked up by the
 * next view switch even if no `tray-click` invalidated the cache first.
 */
export const HYDRATION_TTL_MS = 15_000;

/** The last `get_sync_status` answer, or `null` before the first one lands. */
export const syncStatusSnapshot = writable<SyncStatus | null>(null);

/** Epoch ms of the last successful snapshot; `0` means "cold". */
let hydratedAt = 0;

/** The in-flight hydration, so N mounts in the same tick share one IPC pair. */
let inFlight: Promise<SyncStatus | null> | null = null;

/**
 * Drop the freshness of the cache without discarding the last snapshot, so a
 * reader never renders an empty view just because someone asked for a re-read.
 *
 * Called when the tray was used (it can write the config behind our back) and
 * by the test suites, which need each case to start from a cold cache.
 */
export function resetDashboardHydration(): void {
  hydratedAt = 0;
  devLog('[HYDRATION] cache invalidated');
}

/**
 * Re-read the config and the sync status unconditionally, publish the snapshot
 * and hydrate the shared presence store. Concurrent callers share one run.
 */
export async function refreshDashboardHydration(): Promise<SyncStatus | null> {
  if (inFlight) return inFlight;

  inFlight = (async () => {
    // `loadConfig` never rejects (it falls back to the frontend defaults), but
    // the store is the only reader of a tray-written `snooze_until`, so a
    // failure here must not abort the status read below it.
    try {
      await loadConfig();
    } catch (e) {
      console.error('[HYDRATION] loadConfig FAILED:', e);
    }

    try {
      devLog('[HYDRATION] calling invoke get_sync_status');
      // #670: a snapshot assembled before an event that already landed must not
      // undo it, so the presence revision is sampled before the round trip.
      const snapshotRevision = get(presence).revision;
      const status = await invoke<SyncStatus>('get_sync_status');
      syncStatusSnapshot.set(status);
      hydratedAt = Date.now();
      hydrate(status, snapshotRevision);
      return status;
    } catch (e) {
      console.error('[HYDRATION] get_sync_status FAILED:', e);
      return null;
    } finally {
      inFlight = null;
    }
  })();

  return inFlight;
}

/**
 * Hydrate the Dashboard, from the cache when it is still fresh.
 *
 * The Dashboard awaits this on mount and therefore renders the same first paint
 * it did before the store existed — the difference is that the second mount
 * inside the TTL costs no IPC.
 */
export async function ensureDashboardHydration(): Promise<SyncStatus | null> {
  const cached = get(syncStatusSnapshot);
  if (cached !== null && Date.now() - hydratedAt < HYDRATION_TTL_MS) {
    devLog('[HYDRATION] served from cache');
    return cached;
  }
  return refreshDashboardHydration();
}
