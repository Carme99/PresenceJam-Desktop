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
 * Fails pre-fix: the first test sees "Version dev build", and the second test's
 * import of the detached-pane route throws on the unresolved `$app/state`.
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

// The detached-pane route is imported for its module graph: `$app/state` and
// `$lib/...` have to resolve for this file to load at all.
import About from '$lib/components/About.svelte';
import DetachedPane from '../src/routes/detached/[pane]/+page.svelte';

const pkg = JSON.parse(readFileSync(join(process.cwd(), 'package.json'), 'utf8')) as {
  version: string;
};

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

  it('collects a SvelteKit-dependent component, so $app/* resolves', () => {
    // Loading the module above is the assertion; this only pins that it is the
    // compiled component rather than an empty module.
    expect(DetachedPane).toBeTypeOf('function');
  });
});
