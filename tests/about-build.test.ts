/**
 * #839 — the vitest harness has to run the app's Vite config, not a
 * hand-written copy of it.
 *
 * vitest.config.js takes precedence over vite.config.js for the test run, and
 * it used to declare what the tests needed itself: the svelte plugin, plus a
 * `$lib` alias mirrored from .svelte-kit/tsconfig.json. So the suite inherited
 * neither the app's `define` block nor SvelteKit's plugin:
 *
 *   - `import.meta.env.VITE_APP_BUILD` (vite.config.js, rendered by About.svelte
 *     and read by the dashboard footer) was `undefined` under test, so the
 *     component rendered its `'dev build'` fallback and the build-define class
 *     of regression (#63) could not be caught at all — a test asserting on the
 *     real version string failed against correct code;
 *   - `$app/*` did not resolve, so the detached-pane route — the one component
 *     importing `$app/state` — could not even be collected by a test.
 *
 * Fails pre-fix: the first test sees "Version dev build", and the detached
 * route test rejects on the unresolved `$app/state` — the two failures stay
 * independent, so the About assertion is not taken down by unrelated breakage
 * in the route's import chain.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
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

import About from '$lib/components/About.svelte';
import { defaultConfig } from '$lib/stores/config';
import { t } from '$lib/i18n';

const invokeMock = invoke as unknown as Mock;
const pkg = JSON.parse(readFileSync(join(process.cwd(), 'package.json'), 'utf8')) as {
  version: string;
};

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

describe('#839 vitest.config.js loads the app Vite config', () => {
  it('renders the build string vite.config.js defines, not the dev fallback', () => {
    const { container } = render(About);
    const version = container.querySelector('.version')?.textContent ?? '';

    expect(version).not.toContain('dev build');
    // vite.config.js defines the value as `${pkg.version} (${ISO date})`, so a
    // real version *and* build date prove the define reached the component —
    // neither can come from a hand-copied string in the test.
    const expected = new RegExp(`${pkg.version.replace(/\./g, '\\.')} \\(\\d{4}-\\d{2}-\\d{2}\\)`);
    expect(version).toMatch(expected);
  });

  it('#857 renders each detached route branch from consumer output', async () => {
    // Keep this route import local: the build-define test must remain an
    // independent module-loading assertion when the detached import breaks.
    const { default: DetachedPane } = await import('../src/routes/detached/[pane]/+page.svelte');

    routePane.current = 'logs';
    const logs = render(DetachedPane);
    expect(logs.container.querySelector('.log-viewer')).not.toBeNull();
    expect(logs.getByRole('heading', { name: t('logs.title') })).toBeTruthy();
    expect(logs.getByRole('button', { name: t('settings.popBackIn') })).toBeTruthy();
    expect(logs.queryByRole('button', { name: t('settings.popOutActionTitle') })).toBeNull();
    expect(logs.container.querySelector('.unknown')).toBeNull();
    cleanup();

    routePane.current = 'settings';
    const settings = render(DetachedPane);
    expect(settings.container.querySelector('.settings')).not.toBeNull();
    expect(settings.getByRole('heading', { name: t('settings.title') })).toBeTruthy();
    expect(settings.getByRole('button', { name: t('settings.popBackIn') })).toBeTruthy();
    expect(settings.queryByRole('button', { name: t('settings.popOutActionTitle') })).toBeNull();
    expect(settings.container.querySelector('.unknown')).toBeNull();
    cleanup();

    routePane.current = 'mystery';
    const unknown = render(DetachedPane);
    expect(unknown.container.querySelector('.unknown')?.textContent).toBe(
      t('routes.unknownPane', { pane: 'mystery' })
    );
    expect(unknown.container.querySelector('.log-viewer')).toBeNull();
    expect(unknown.container.querySelector('.settings')).toBeNull();
  });
});
