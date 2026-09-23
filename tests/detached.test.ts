/**
 * #594 / #601 — detached-window store and ACL contracts.
 *
 * Two behaviors shipped 4.5.x got wrong, both about the badge lying:
 *  - `WebviewWindow.close()` from inside a detached window was ACL-denied,
 *    so "Pop back in" did nothing and the store then cleared the badge
 *    while the window was still on screen;
 *  - the badge map started empty after a main-window reload even when the
 *    detached windows were still up, so the Dashboard offered "navigate"
 *    for a pane that was already popped out.
 *
 * The capability itself is asserted too: `core:default` expands to
 * `core:window:default`, whose allow-list omits `allow-close`, so the grant
 * has to be explicit — and the set must stay scoped to these two labels.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

interface FakeWindow {
  close?: () => Promise<void>;
}

const winState: { windows: Record<string, FakeWindow | null> } = { windows: {} };

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  // `getByLabel` is the only WebviewWindow member this store reaches.
  WebviewWindow: {
    getByLabel: async (label: string) => winState.windows[label] ?? null
  }
}));

import { DETACHED_LABEL, detachedPanes, popIn, reconcileDetachedPanes } from '$lib/stores/detach';

// vitest runs with the project root as cwd (its own "RUN <root>" banner).
const capability = JSON.parse(
  readFileSync(join(process.cwd(), 'src-tauri/capabilities/detached.json'), 'utf8')
) as { windows: string[]; permissions: string[] };

const mainCapability = JSON.parse(
  readFileSync(join(process.cwd(), 'src-tauri/capabilities/default.json'), 'utf8')
) as { permissions: string[] };

beforeEach(() => {
  winState.windows = {};
  detachedPanes.set({ logs: false, settings: false });
  // The refused-close path logs the rejection; keep the run output clean.
  vi.spyOn(console, 'warn').mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('detached capability (#594)', () => {
  it('is scoped to the two labels the store pops out', () => {
    expect(capability.windows).toEqual(Object.values(DETACHED_LABEL));
  });

  it('grants core:window:allow-close and no other window command', () => {
    expect(capability.permissions.filter((p) => p.startsWith('core:window:'))).toEqual([
      'core:window:allow-close'
    ]);
  });
});

describe('main capability (#919)', () => {
  it('does not expose the Rust-owned autostart and shortcut plugin commands', () => {
    const rustOwnedPluginGrants = mainCapability.permissions.filter(
      (permission) => permission.startsWith('autostart:') || permission.startsWith('global-shortcut:')
    );

    expect(rustOwnedPluginGrants).toEqual([]);
  });
});

describe('popIn close failure (#594)', () => {
  it('keeps claiming the pane is detached while its window is still alive', async () => {
    detachedPanes.set({ logs: true, settings: false });
    winState.windows[DETACHED_LABEL.logs] = {
      close: async () => {
        throw new Error('window.close not allowed');
      }
    };
    await popIn('logs');
    expect(get(detachedPanes).logs).toBe(true);
  });

  it('clears the flag when no window backs it', async () => {
    detachedPanes.set({ logs: true, settings: false });
    await popIn('logs');
    expect(get(detachedPanes).logs).toBe(false);
  });
});

describe('reconcileDetachedPanes (#601)', () => {
  it('adopts a window that outlived a main-window reload', async () => {
    winState.windows[DETACHED_LABEL.settings] = {};
    await reconcileDetachedPanes();
    expect(get(detachedPanes)).toEqual({ logs: false, settings: true });
  });

  it('clears the panes whose window is gone', async () => {
    detachedPanes.set({ logs: true, settings: true });
    winState.windows[DETACHED_LABEL.logs] = {};
    await reconcileDetachedPanes();
    expect(get(detachedPanes)).toEqual({ logs: true, settings: false });
  });
});
