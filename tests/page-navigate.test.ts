/**
 * #967 — the app menu's "Show Dashboard" must reach the Dashboard for an
 * install that is already configured, even while the wizard owns the view
 * (Settings' "Run onboarding" or the boot probe's fail-open path put it there),
 * and must stay inert for a first-run install, where there is nothing to go
 * back to.
 *
 * The wizard offers the same escape hatch through its own header control; this
 * covers the programmatic route, which `+page.svelte`'s navigate listener used
 * to swallow unconditionally (C2's "onboarding owns the view" guard).
 *
 * #815 extends the same gate to the tray's show-about event: emitted while
 * the wizard owns the view it must leave the wizard mounted.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { get } from 'svelte/store';

const { invoke, listeners, detachedPaneStore } = vi.hoisted(() => {
  const paneValue = { logs: false, settings: false };
  const paneSubscribers = new Set<(value: typeof paneValue) => void>();
  return {
    invoke: vi.fn(),
    listeners: new Map<string, (event: { payload: unknown }) => void>(),
    // Dashboard reads the pane store, so the mock exposes a real (readable)
    // store without pulling `svelte/store` into a hoisted factory.
    detachedPaneStore: {
      subscribe(run: (value: typeof paneValue) => void) {
        run(paneValue);
        paneSubscribers.add(run);
        return () => paneSubscribers.delete(run);
      }
    }
  };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler) → unlisten. The
  // handler is kept so the test can emit the navigate event the app menu sends.
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'main' })
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted')
}));
vi.mock('$lib/stores/detach', () => ({
  detachedPanes: detachedPaneStore,
  focusDetached: vi.fn(async () => {}),
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  reconcileDetachedPanes: vi.fn(async () => {})
}));

import Page from '../src/routes/+page.svelte';
import { currentView } from '$lib/stores/app';
import { defaultConfig } from '$lib/stores/config';
import { resetAuthFlow } from '$lib/stores/authFlow.svelte';

const CLIENT_ID = 'a'.repeat(32);

function mockBackend(complete: boolean, hasCredentials: boolean) {
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'is_onboarding_complete':
        return complete;
      case 'load_config':
        return hasCredentials
          ? {
              ...structuredClone(defaultConfig),
              spotify: {
                ...defaultConfig.spotify,
                client_id: CLIENT_ID,
                client_secret_set: true,
                client_secret_state: 'present'
              }
            }
          : structuredClone(defaultConfig);
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      default:
        return undefined;
    }
  });
}

/** Mount the root page and settle boot's mocked IPC. */
async function mountPage(complete: boolean, hasCredentials = true) {
  mockBackend(complete, hasCredentials);
  const rendered = render(Page);
  for (let i = 0; i < 32; i++) await Promise.resolve();
  return rendered;
}

/** Emit the navigation the app menu sends. */
function fireNavigate(payload: string) {
  const handler = listeners.get('navigate');
  if (!handler) throw new Error('the page registered no navigate listener');
  handler({ payload });
}

/** Emit the About request the tray menu sends. */
function fireShowAbout() {
  const handler = listeners.get('show-about');
  if (!handler) throw new Error('the page registered no show-about listener');
  handler({ payload: undefined });
}

beforeEach(() => {
  invoke.mockReset();
  listeners.clear();
  resetAuthFlow();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('navigate while the wizard owns the view (#967)', () => {
  it('honours a dashboard target for an install that is already configured', async () => {
    await mountPage(true);
    // Settings' "Run onboarding" / the boot fail-open path drops the wizard in.
    currentView.set('onboarding');

    fireNavigate('dashboard');

    expect(get(currentView)).toBe('dashboard');
  });

  it('keeps a first-run install one-way', async () => {
    // No probe verdict and nothing stored: the boot gate lands on the wizard.
    await mountPage(false, false);
    // The boot gate itself put the wizard up: nothing is configured yet.
    expect(get(currentView)).toBe('onboarding');

    fireNavigate('dashboard');

    expect(get(currentView)).toBe('onboarding');
  });

  it('still ignores a non-dashboard target while the wizard owns the view', async () => {
    await mountPage(true);
    currentView.set('onboarding');

    fireNavigate('settings');

    expect(get(currentView)).toBe('onboarding');
  });

  it('show-about leaves the wizard mounted while it owns the view (#815)', async () => {
    await mountPage(true);
    currentView.set('onboarding');

    fireShowAbout();

    expect(get(currentView)).toBe('onboarding');
  });

  it('show-about leaves a first-run install one-way (#815)', async () => {
    await mountPage(false, false);
    expect(get(currentView)).toBe('onboarding');

    fireShowAbout();

    expect(get(currentView)).toBe('onboarding');
  });

  it('show-about still reaches About once onboarding yields the view (#815)', async () => {
    await mountPage(true);
    expect(get(currentView)).not.toBe('onboarding');

    fireShowAbout();

    expect(get(currentView)).toBe('about');
  });
});

describe('show-about before boot settles (#815)', () => {
  it('leaves the view alone while ready is still false', async () => {
    mockBackend(false, false);
    render(Page);
    // Wait only for onMount's listener registration (breaks after 1-2 ticks),
    // capped at 8 — well under the 32 mountPage needs to settle boot, so
    // `ready` is still false when we fire.
    for (let i = 0; i < 8 && !listeners.has('show-about'); i++) {
      await Promise.resolve();
    }
    if (!listeners.has('show-about')) throw new Error('show-about listener never registered');
    fireShowAbout();

    // Pre-fix this is already 'about' (unconditional set) — the gate must
    // hold the pre-boot view instead.
    expect(get(currentView)).not.toBe('about');
    for (let i = 0; i < 32; i++) await Promise.resolve();
    expect(get(currentView)).toBe('onboarding');
  });
});
