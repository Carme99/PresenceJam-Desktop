/**
 * 4.7.0 (issue #674) — the boot path hydrates the shared config store.
 *
 * On a Dashboard-first launch (the normal one for an already-configured user)
 * nothing called `loadConfig()`: the config-consuming components only load when
 * their own view mounts. The i18n store reads `config.locale` from that store
 * and keys its one-shot legacy-locale migration on the store's hydration flag,
 * so while the store stayed at the frontend defaults the webview kept the
 * mirror's language for the whole session — the "German UI with an English
 * tray" case this slice exists to remove.
 *
 * This mounts the real app shell (`src/routes/+page.svelte`) with no prior
 * `loadConfig()` call, a `localStorage.locale` mirror that disagrees, and a
 * backend that holds `locale: 'de'`.
 *
 * Fails pre-fix: the store is never hydrated, so the webview keeps the mirror's
 * language and the persisted, tray-followed value never reaches `configStore`.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { get, writable } from 'svelte/store';
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
  }),
  emitTo: vi.fn(async () => undefined)
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn()
}));
// The detach store reaches for WebviewWindow at import time; the shell only
// reads the pane flags.
vi.mock('$lib/stores/detach', async () => {
  const { writable: store } = await import('svelte/store');
  return {
    detachedPanes: store({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {})
  };
});

import { invoke } from '@tauri-apps/api/core';
import Page from '../src/routes/+page.svelte';
import { configStore, defaultConfig } from '$lib/stores/config';
import { i18n } from '$lib/i18n';
import { currentView } from '$lib/stores/app';

const invokeMock = invoke as unknown as Mock;

/** What the backend has on disk for this launch. */
const PERSISTED = {
  ...structuredClone(defaultConfig),
  locale: 'de',
  spotify: { ...defaultConfig.spotify, client_id: 'stored-client-id' }
};

function setLocaleCalls(): unknown[][] {
  return invokeMock.mock.calls.filter((call) => call[0] === 'set_locale');
}

describe('boot hydration (#674)', () => {
  beforeEach(() => {
    listeners.length = 0;
    // A mirror that disagrees with the backend: the config must win.
    localStorage.setItem('presencejam:locale', 'fr');
    currentView.set('dashboard');
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case 'load_config':
          return structuredClone(PERSISTED);
        case 'is_onboarding_complete':
          return true;
        case 'get_sync_status':
          return null;
        case 'get_recent_logs':
          return [];
        default:
          return undefined;
      }
    });
  });

  afterEach(() => {
    cleanup();
    localStorage.clear();
  });

  it('a Dashboard-first launch adopts the persisted locale over the mirror', async () => {
    render(Page);

    // The shell booted through the store: the persisted document is in it…
    await waitFor(() => expect(get(configStore).locale).toBe('de'));
    // …and the Dashboard is still the landing view.
    expect(get(currentView)).toBe('dashboard');

    // The persisted value reaches the webview dictionary store…
    expect(i18n.locale).toBe('de');
    expect(document.documentElement.lang).toBe('de');

    // …and the pre-paint mirror is rewritten to match instead of being written
    // back over the persisted locale: `config.locale` is what the tray and the
    // app menu render (`set_locale` is the only writer of that field).
    expect(localStorage.getItem('presencejam:locale')).toBe('de');
    expect(setLocaleCalls()).toEqual([]);
  });
});
