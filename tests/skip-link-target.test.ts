/**
 * #742 — the skip link points at each view's body, not the view header.
 *
 * `.app-container` in `+page.svelte` used to carry `id="main-content"`, which
 * wraps the entire mounted view — including the Dashboard's header and the
 * PageHeader bar (Back, pop-out, theme) that repeats across Settings, Logs,
 * Diagnostics and Reconnect. Activating "Skip to main content" therefore
 * landed focus on the container, so the next Tab still reached the repeated
 * chrome the link is supposed to bypass.
 *
 * Fails pre-fix: the container still owns the id, so the "below the header"
 * queries below are null (and the no-duplicates assertion sees two copies
 * once both halves exist).
 *
 * The end-to-end focus proof (real fragment navigation in a real browser) is
 * `tests/browser/skip-link.spec.ts`; jsdom does not move focus on a
 * fragment, so here the contract is asserted structurally — each view's body
 * region owns the id the layout's link points at, it can hold focus, the
 * header bar does not carry it, and it is the only element in the document
 * carrying that id. The detached route (`detached/[pane]/+page.svelte`) owns
 * its own `#main-content` in a document that never mounts `.app-container`
 * (#743), so the uniqueness assertion here also guards the cross-route
 * contract: exactly one copy per mounted view, only one view mounted at a
 * time.
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
import { i18n } from '$lib/i18n';

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
          current_track: null,
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
      case 'get_recent_logs':
        return [];
      case 'get_diagnostics_snapshot':
        return null;
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


/**
 * The one structural assertion behind every view below: the header bar
 * renders WITHOUT the id, and exactly one element below it carries the id
 * with `tabindex="-1"` (focusable programmatically, not in the tab order).
 */
function expectSingleBodyTarget(headerSelector: string, bodySelector: string) {
  const header = document.querySelector(headerSelector);
  expect(header, `expected ${headerSelector} to be mounted`).not.toBeNull();
  expect(
    header!.querySelector('#main-content'),
    `${headerSelector} must not carry the skip-link target`
  ).toBeNull();
  expect(
    header!.getAttribute('id'),
    `${headerSelector} itself must not be the skip-link target`
  ).not.toBe('main-content');

  const targets = document.querySelectorAll('[id="main-content"]');
  expect(targets, 'exactly one #main-content per mounted view').toHaveLength(1);
  const target = targets[0] as HTMLElement;
  expect(target.getAttribute('tabindex')).toBe('-1');
  // Below the header: either a later sibling of the header bar, or an
  // element the header bar does not contain.
  const body = document.querySelector(bodySelector);
  expect(body, `expected ${bodySelector} to be mounted`).not.toBeNull();
  expect(target === body || body!.contains(target)).toBe(true);
  expect(header!.contains(target)).toBe(false);
}

describe('skip link targets each view body (#742)', () => {
  it('leaves the app wrapper without the id and puts it on the Dashboard body', async () => {
    await mountPage();

    expect(document.querySelector('.app-container')?.getAttribute('id')).not.toBe('main-content');
    expectSingleBodyTarget('.dashboard > header', '.dashboard main');
  });

  it('gives Settings exactly one target below its PageHeader', async () => {
    await mountPage();
    fireNavigate('settings');
    await waitFor(() => expect(document.querySelector('.settings')).not.toBeNull());

    expectSingleBodyTarget('.settings .page-header', '.settings .sections');
  });

  it('gives the Logs view exactly one target below its header and toolbar', async () => {
    await mountPage();
    fireNavigate('logs');
    await waitFor(() => expect(document.querySelector('.log-viewer')).not.toBeNull());

    expectSingleBodyTarget('.log-viewer .page-header', '.log-viewer .log-wrap');
  });

  it('gives Diagnostics exactly one target below its header and toolbar', async () => {
    await mountPage();
    fireNavigate('diagnostics');
    await waitFor(() => expect(document.querySelector('.diagnostics')).not.toBeNull());

    expectSingleBodyTarget('.diagnostics .page-header', '.diagnostics .content');
  });

  it('gives Reconnect exactly one target below its PageHeader', async () => {
    await mountPage();
    fireNavigate('reconnect');
    await waitFor(() => expect(document.querySelector('.reconnect')).not.toBeNull());

    expectSingleBodyTarget('.reconnect .page-header', '.reconnect .content');
  });

  it('keeps exactly one target across a Dashboard → Settings switch (no duplicates, no loss)', async () => {
    await mountPage();
    expectSingleBodyTarget('.dashboard > header', '.dashboard main');

    fireNavigate('settings');
    await waitFor(() => expect(document.querySelector('.settings')).not.toBeNull());

    // The Dashboard unmounted with its `<main>`; Settings mounted with its
    // own `.sections` — one copy moved, none duplicated, none lost.
    expect(document.querySelector('.dashboard')).toBeNull();
    expectSingleBodyTarget('.settings .page-header', '.settings .sections');
    expect(get(currentView)).toBe('settings');
  });
  it('gives Onboarding exactly one target on the wizard step body', async () => {
    await mountPage();
    fireNavigate('onboarding');
    await waitFor(() => expect(document.querySelector('.onboarding')).not.toBeNull());

    // The wizard has no PageHeader; the brand header is the repeated
    // chrome, so the step body below it carries the target.
    const targets = document.querySelectorAll('[id="main-content"]');
    expect(targets, 'exactly one #main-content on the wizard').toHaveLength(1);
    expect(targets[0].getAttribute('tabindex')).toBe('-1');
    expect(document.querySelector('.onboarding .step')).toBe(targets[0]);
  });

  it('gives About exactly one target on the card body', async () => {
    await mountPage();
    fireNavigate('about');
    await waitFor(() => expect(document.querySelector('.about')).not.toBeNull());

    const targets = document.querySelectorAll('[id="main-content"]');
    expect(targets, 'exactly one #main-content on About').toHaveLength(1);
    expect(targets[0].getAttribute('tabindex')).toBe('-1');
    expect(document.querySelector('.about .about-card')).toBe(targets[0]);
  });
});
