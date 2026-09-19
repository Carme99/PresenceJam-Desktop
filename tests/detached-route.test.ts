/**
 * #857 — the detached-pane router, rendered for real.
 *
 * `src/routes/detached/[pane]/+page.svelte` is the entry point of every
 * popped-out Logs/Settings window, and nothing rendered it in a test. The
 * pane string in the URL is the only link between the Rust-side
 * `WebviewWindow` creation and this route, so a rename on either side used
 * to ship green: the main window keeps working and only a user who pops a
 * pane out meets "Unknown pane".
 *
 * The suite pins the three branches — `logs`, `settings`, and a value that
 * matches neither — and, for the two valid panes, that the route passes
 * `detached` down: the pane's Back button reads `settings.popBackIn` only
 * when the prop arrived.
 *
 * `$app/state` is stubbed rather than aliased (vitest.config.js aliases only
 * `$lib`): the route reads `page.params.pane`, so the test drives exactly
 * the value the SvelteKit router would have parsed out of the window URL.
 * `coverage.exclude` is left alone — the v8 provider cannot instrument this
 * file, as the inline comment there records.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';

// Hoisted so the mock factory above the imports can close over the same
// object the tests mutate.
const { page } = vi.hoisted(() => ({ page: { params: { pane: 'logs' } } }));
vi.mock('$app/state', () => ({ page }));

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted')
}));
vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  focusDetached: vi.fn(async () => {})
}));

import { invoke } from '@tauri-apps/api/core';
import DetachedPane from '../src/routes/detached/[pane]/+page.svelte';
import { configStore, defaultConfig } from '$lib/stores/config';
import { t } from '$lib/i18n';
import type { Mock } from 'vitest';

const invokeMock = invoke as unknown as Mock;

/** A configured install, so Settings' onMount has something to adopt. */
function configuredConfig() {
  const cfg = structuredClone(defaultConfig);
  cfg.spotify.client_id = 'test-client-id';
  return cfg;
}

/** Render the route for `pane` — what the window URL would have carried. */
function renderPane(pane: string) {
  page.params.pane = pane;
  return render(DetachedPane);
}

beforeEach(() => {
  configStore.set(configuredConfig());
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        return get(configStore);
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      default:
        // `get_recent_logs` and the scope readers: an empty list is the
        // shape both panes tolerate.
        return [];
    }
  });
});

afterEach(() => {
  cleanup();
});

describe('detached pane router (#857)', () => {
  it('mounts the Logs pane, detached, for pane=logs', async () => {
    const { container, getByRole } = renderPane('logs');

    await waitFor(() => expect(container.querySelector('.log-viewer')).not.toBeNull());
    expect(container.querySelector('.settings')).toBeNull();
    // The `detached` prop arrived: the pane's Back control pops the window
    // back in instead of navigating the main view.
    expect(getByRole('button', { name: t('settings.popBackIn') })).toBeTruthy();
  });

  it('mounts the Settings pane, detached, for pane=settings', async () => {
    const { container, getByRole } = renderPane('settings');

    await waitFor(() => expect(container.querySelector('.settings')).not.toBeNull());
    expect(container.querySelector('.log-viewer')).toBeNull();
    expect(getByRole('button', { name: t('settings.popBackIn') })).toBeTruthy();
  });

  it('names an unrecognised pane instead of rendering a blank window', async () => {
    const { container } = renderPane('statistics');

    await waitFor(() => expect(container.querySelector('.unknown')).not.toBeNull());
    expect(container.querySelector('.log-viewer')).toBeNull();
    expect(container.querySelector('.settings')).toBeNull();
    expect(container.querySelector('.unknown')?.textContent).toBe(
      t('routes.unknownPane', { pane: 'statistics' })
    );
  });
});
