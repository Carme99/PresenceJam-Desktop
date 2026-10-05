/**
 * #743 — the popped-out Logs / Settings panes need a skip-link target.
 *
 * The root layout renders "Skip to main content" in every webview, detached
 * panes included, but the detached route mounted `LogViewer` / `Settings`
 * bare. In a popped-out pane the link was therefore the first tab stop with
 * no fragment target anywhere in the document: activating it did nothing.
 *
 * Fails pre-fix: `container.querySelector('#main-content')` is null for every
 * pane value, so both assertions below fail against the current route.
 *
 * The end-to-end focus proof (real fragment navigation in a real browser) is
 * `tests/browser/detached-skip-link.spec.ts`; jsdom does not move focus on a
 * fragment, so here the contract is asserted structurally — the pane body
 * owns the id the layout's link points at, it can hold focus, and it is the
 * only element in the document carrying that id (#742 owns the main
 * window's copy on `.app-container`, and this route never mounts it).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import type { Mock } from 'vitest';

const { invoke, routePane } = vi.hoisted(() => ({
  invoke: vi.fn(),
  routePane: { current: 'logs' as string | undefined }
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ label: 'detached-logs' })
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn(async () => {})
}));
vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  focusDetached: vi.fn(async () => {})
}));
vi.mock('$app/state', () => ({
  page: {
    params: {
      get pane() {
        return routePane.current;
      }
    }
  }
}));

import { defaultConfig } from '$lib/stores/config';

const invokeMock = invoke as unknown as Mock;

beforeEach(() => {
  routePane.current = 'logs';
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        return structuredClone(defaultConfig);
      case 'get_recent_logs':
        return [];
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      case 'get_spotify_granted_scopes':
      case 'get_teams_granted_scopes':
        return [];
      default:
        return null;
    }
  });
});

afterEach(() => cleanup());

/** The pane container the layout's `href="#main-content"` lands on. */
function skipTarget(container: HTMLElement): HTMLElement | null {
  return container.querySelector<HTMLElement>('#main-content');
}

describe('#743 detached panes carry the skip-link target', () => {
  it('gives the Logs pane body the id and a focusable tab stop', async () => {
    const { default: DetachedPane } = await import('../src/routes/detached/[pane]/+page.svelte');
    routePane.current = 'logs';
    const { container } = render(DetachedPane);

    const target = skipTarget(container);
    expect(target, 'the Logs pane has no #main-content for the skip link').not.toBeNull();
    expect(target!.querySelector('.log-viewer')).not.toBeNull();
    // A bare fragment navigation only moves focus when the target is
    // programmatically focusable; without this the jump scrolls and leaves
    // focus on the link.
    expect(target!.getAttribute('tabindex')).toBe('-1');
  });

  it('gives the Settings pane body the same target', async () => {
    const { default: DetachedPane } = await import('../src/routes/detached/[pane]/+page.svelte');
    routePane.current = 'settings';
    const { container } = render(DetachedPane);

    const target = skipTarget(container);
    expect(target, 'the Settings pane has no #main-content for the skip link').not.toBeNull();
    expect(target!.querySelector('.settings')).not.toBeNull();
    expect(target!.getAttribute('tabindex')).toBe('-1');
  });

  it('targets the unknown-pane notice too, and never duplicates the id', async () => {
    const { default: DetachedPane } = await import('../src/routes/detached/[pane]/+page.svelte');
    routePane.current = 'mystery';
    const { container } = render(DetachedPane);

    const target = skipTarget(container);
    expect(target).not.toBeNull();
    expect(target!.querySelector('.unknown')?.textContent).toContain('mystery');
    // #742 keeps the main window's copy on `.app-container`; a detached
    // document mounts this route alone, so the id must appear exactly once.
    expect(container.querySelectorAll('[id="main-content"]')).toHaveLength(1);
  });
});
