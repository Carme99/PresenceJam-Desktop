/**
 * #739 — what a view switch does to focus and to the screen-reader
 * announcement.
 *
 * `+page.svelte` replaces the mounted view without touching focus: the control
 * the user activated is destroyed with the old view, focus falls back to
 * `<body>`, and nothing is read out. The skip link had the same shape of
 * problem from the other end — its target sat on the wrapper around the whole
 * mounted view, so activating it left the next Tab inside the header chrome the
 * link exists to bypass.
 *
 * Fails pre-fix: there is no navigation effect at all, so `document.activeElement`
 * is `<body>` after a switch and there is no live region to read.
 *
 * #742 moved the skip link's target down onto each view's body (asserted in
 * `tests/skip-link-target.test.ts`); this file pins the focus + announcement
 * half of the contract, not target placement.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';

const { invoke, listeners } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>()
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'main' })
}));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.6.0') }));
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn(async () => null) }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn()
}));
// A hoisted `vi.mock` factory cannot use a static import — the module under
// test pulls this store in at collection time — so `svelte/store` is loaded
// inside the factory. The same exception `tests/dashboard.test.ts` documents.
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

import Page from '../src/routes/+page.svelte';
import { currentView, pendingMenuNav, settingsDirty } from '$lib/stores/app';
import { defaultConfig } from '$lib/stores/config';
import { resetAuthFlow } from '$lib/stores/authFlow.svelte';
import { resetDashboardHydration } from '$lib/stores/dashboardHydration';
import { i18n, t } from '$lib/i18n';

const CLIENT_ID = 'a'.repeat(32);

/** Boot an install that is configured, so the root page lands on the Dashboard. */
async function mountPage() {
  resetDashboardHydration();
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'is_onboarding_complete':
        return true;
      case 'load_config':
        return {
          ...structuredClone(defaultConfig),
          spotify: {
            ...defaultConfig.spotify,
            client_id: CLIENT_ID,
            client_secret_set: true,
            client_secret_state: 'present'
          }
        };
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: {
            id: 'track-1',
            title: 'A Track',
            artist: 'An Artist',
            album: 'An Album',
            album_art_url: null,
            duration_ms: 200000,
            progress_ms: 1000,
            is_playing: true
          },
          spotify_connected: true,
          teams_connected: true
        };
      // Settings reads its two OAuth scope lists as soon as it mounts. The
      // default `undefined` here is not an inert "unmodelled command": Settings
      // calls `.includes` on both answers, so an unmodelled scope list throws
      // an unhandled TypeError and takes the whole run red.
      case 'get_spotify_granted_scopes':
        return ['user-modify-playback-state', 'user-read-playback-state'];
      case 'get_teams_granted_scopes':
        return ['Presence.Read', 'Presence.ReadWrite'];
      default:
        return undefined;
    }
  });
  const rendered = render(Page);
  // The root page's guard refuses every navigation until boot settles, so a
  // test that fires one too early would silently assert against the old view.
  // `ready` is exactly when the loading branch gives way to the mounted view.
  await waitFor(() => expect(document.querySelector('.view-announcement')).not.toBeNull());
  await waitFor(() => expect(document.querySelector('.dashboard')).not.toBeNull());
  return rendered;
}

function currentViewIs(view: string): boolean {
  return get(currentView) === view;
}

/** Emit a navigation through the root page's guard — the same path the tray uses. */
function fireNavigate(payload: string) {
  const handler = listeners.get('navigate');
  if (!handler) throw new Error('the page registered no navigate listener');
  handler({ payload });
}

beforeEach(() => {
  invoke.mockReset();
  listeners.clear();
  resetAuthFlow();
  resetDashboardHydration();
  currentView.set('dashboard');
  settingsDirty.set(false);
  pendingMenuNav.set(null);
  void i18n.set('en');
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
});

afterEach(() => {
  cleanup();
});

describe('A view switch moves focus and announces (#739)', () => {
  it('focuses the destination view heading instead of leaving focus on <body>', async () => {
    await mountPage();
    fireNavigate('settings');

    await waitFor(() =>
      expect(document.activeElement?.textContent?.trim()).toBe(t('settings.title'))
    );
    // Focus is on the destination view's heading, not wherever the destroyed
    // control left it.
    expect(document.activeElement?.getAttribute('data-view-heading')).not.toBeNull();
    // The heading is focusable programmatically without being a tab stop —
    // otherwise moving focus here would insert a new stop in the tab order.
    expect(document.activeElement?.getAttribute('tabindex')).toBe('-1');
  });

  it('focuses again for a second switch, not just the first', async () => {
    await mountPage();
    fireNavigate('settings');
    await waitFor(() => expect(document.activeElement?.textContent?.trim()).toBe(t('settings.title')));

    fireNavigate('about');

    await waitFor(() => expect(document.activeElement?.textContent?.trim()).toBe('PresenceJam'));
    expect(document.activeElement?.getAttribute('data-view-heading')).not.toBeNull();
  });

  it('reads the new view name out through one polite live region', async () => {
    await mountPage();
    const region = document.querySelector('.view-announcement');
    expect(region?.getAttribute('aria-live')).toBe('polite');
    // The Dashboard is the app itself, so it announces by its product name.
    expect(region?.textContent?.trim()).toBe('PresenceJam');

    fireNavigate('settings');

    await waitFor(() =>
      expect(document.querySelector('.view-announcement')?.textContent?.trim()).toBe(t('settings.title'))
    );
  });

  it('announces the localized view name after a locale change', async () => {
    await mountPage();
    await i18n.set('de');
    fireNavigate('logs');

    await waitFor(() =>
      expect(document.querySelector('.view-announcement')?.textContent?.trim()).toBe(t('logs.title'))
    );
    // Not an English leftover: the announcement has to be the locale's own
    // word for the view, which is what `VIEW_ANNOUNCEMENT_KEY` resolves.
    expect(t('logs.title')).toBe('Protokolle');
  });
});
