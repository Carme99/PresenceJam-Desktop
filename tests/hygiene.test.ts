/**
 * #680 — follow-system appearance + compact density.
 *
 * Two frontend surfaces that had no test at all:
 *
 *   - the theme preference gains `system`, which must track the OS appearance
 *     *live* while selected, and must NOT override an explicit dark/light;
 *   - `[data-density="compact"]` in app.css must actually change the spacing
 *     and type tokens, and must be independent of `data-theme`.
 *
 * Fails pre-fix: `Theme` has no `system` member, nothing listens to the media
 * query (the OS preference is sampled once at module init), no density store
 * or `[data-density="compact"]` rule exists, and the token values are
 * identical in both density states.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { tick } from 'svelte';
import { readFileSync } from 'node:fs';
import PageHeader from '$lib/components/PageHeader.svelte';
import { theme as themeStore, toggleTheme } from '$lib/stores/theme';

// The header-toggle tests must drive the *same* theme instance the component
// subscribes to, so both are statically imported (`vi.resetModules` would give
// the test a fresh store, and a component re-imported afterwards would bind a
// second Svelte runtime — `Cannot read properties of undefined`). A media-query
// stub therefore has to exist before this file's imports run, which is what
// `vi.hoisted` is for: the statically-imported store captures this instance.
const osMedia = vi.hoisted(() => {
  const listeners: Array<() => void> = [];
  const mql = {
    matches: false,
    media: '(prefers-color-scheme: light)',
    addEventListener: (_type: string, cb: () => void) => listeners.push(cb),
    removeEventListener: () => {},
    addListener: (cb: () => void) => listeners.push(cb),
    removeListener: () => {},
    onchange: null
  };
  vi.stubGlobal('matchMedia', vi.fn(() => mql));
  return { mql, listeners };
});

const LIGHT_QUERY = '(prefers-color-scheme: light)';
// Vitest runs with the repo root as cwd (`npm test`); `import.meta.url` is not a
// file: URL under the transform, so the real stylesheet is read by path.
const appCss = readFileSync('src/app.css', 'utf8');
// The pre-paint bootstrap cannot be imported: it is an inline `<script>` in
// app.html that runs before SvelteKit and must stay dependency-free. Extracted
// and evaluated here so its contract is covered rather than assumed.
const appHtml = readFileSync('src/app.html', 'utf8');

/** Run app.html's inline pre-paint bootstrap against the current jsdom state. */
function runPrePaintBootstrap() {
  // Anchored to a line start: the surrounding comment also mentions `<script>`.
  const source = appHtml.match(/^\s*<script>([\s\S]*?)<\/script>/m)?.[1];
  if (!source) throw new Error('app.html pre-paint bootstrap not found');
  new Function(source)();
}

/** Minimal MediaQueryList stand-in jsdom does not provide. */
function installMatchMedia(initialLight: boolean) {
  const listeners: Array<() => void> = [];
  const mql = {
    matches: initialLight,
    media: LIGHT_QUERY,
    addEventListener: (_type: string, cb: () => void) => listeners.push(cb),
    removeEventListener: () => {},
    addListener: (cb: () => void) => listeners.push(cb),
    removeListener: () => {},
    onchange: null,
    dispatchEvent: () => true
  };
  vi.stubGlobal('matchMedia', vi.fn(() => mql));
  return {
    mql,
    /** Flip the OS appearance and fire the real `change` event. */
    setOsLight(value: boolean) {
      mql.matches = value;
      for (const cb of listeners) cb();
    },
    listenerCount: () => listeners.length
  };
}

/**
 * Re-import per test: the store samples `matchMedia` and localStorage at module
 * init, so a static import would freeze it before any mock exists. This is the
 * module-loading boundary the dynamic import is for, not a runtime-selected path.
 */
async function loadThemeModule() {
  vi.resetModules();
  return await import('$lib/stores/theme');
}

const paintedTheme = () => document.documentElement.getAttribute('data-theme');
const paintedDensity = () => document.documentElement.getAttribute('data-density');
const token = (name: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim();

const SPACING_TOKENS = Array.from({ length: 10 }, (_, i) => `--sp-${i + 1}`);
const TYPE_TOKENS = ['xs', 'sm', 'base', 'md', 'lg', 'xl', '2xl', '3xl'].map((k) => `--fs-${k}`);

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  document.documentElement.removeAttribute('data-density');
  document.head.querySelectorAll('style[data-test="app-css"]').forEach((el) => el.remove());
  window.history.replaceState({}, '', '/');
  // The statically-imported store outlives the per-test module resets above.
  osMedia.mql.matches = false;
  themeStore.set('dark');
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('theme preference: system follows the OS (#680)', () => {
  it('repaints when the OS appearance changes while the preference is system', async () => {
    const os = installMatchMedia(false);
    const th = await loadThemeModule();

    th.theme.set('system');
    expect(get(th.theme)).toBe('system');
    expect(paintedTheme()).toBe('dark');

    os.setOsLight(true);
    expect(paintedTheme()).toBe('light');

    os.setOsLight(false);
    expect(paintedTheme()).toBe('dark');
  });

  it('does not override an explicit light or dark preference', async () => {
    const os = installMatchMedia(false);
    const th = await loadThemeModule();

    th.theme.set('light');
    os.setOsLight(false);
    expect(paintedTheme()).toBe('light');

    th.theme.set('dark');
    os.setOsLight(true);
    expect(paintedTheme()).toBe('dark');
  });

  it('resolves a stored system preference on load and registers exactly one listener', async () => {
    const os = installMatchMedia(true);
    localStorage.setItem('presencejam:theme', 'system');
    const th = await loadThemeModule();

    expect(get(th.theme)).toBe('system');
    expect(paintedTheme()).toBe('light');
    expect(os.listenerCount()).toBe(1);
  });

  it('converges a detached window onto another window selecting system', async () => {
    installMatchMedia(false);
    const th = await loadThemeModule();
    th.theme.set('dark');

    window.dispatchEvent(
      new StorageEvent('storage', { key: th.STORAGE_KEY, newValue: 'system' })
    );
    expect(get(th.theme)).toBe('system');
    expect(paintedTheme()).toBe('dark');
  });

  it('flips relative to the painted theme from system', async () => {
    installMatchMedia(true);
    const th = await loadThemeModule();
    th.theme.set('system');
    expect(paintedTheme()).toBe('light');

    th.toggleTheme();
    expect(get(th.theme)).toBe('dark');
    expect(paintedTheme()).toBe('dark');
  });
});

describe('compact density (#680)', () => {
  it('paints data-density and converges across windows', async () => {
    installMatchMedia(false);
    const th = await loadThemeModule();

    expect(get(th.density)).toBe('comfortable');
    expect(paintedDensity()).toBe('comfortable');

    th.density.set('compact');
    expect(paintedDensity()).toBe('compact');
    expect(localStorage.getItem(th.DENSITY_STORAGE_KEY)).toBe('compact');

    window.dispatchEvent(
      new StorageEvent('storage', { key: th.DENSITY_STORAGE_KEY, newValue: 'comfortable' })
    );
    expect(get(th.density)).toBe('comfortable');
    expect(paintedDensity()).toBe('comfortable');
  });

  it('tightens every spacing and type token, independently of the theme', async () => {
    installMatchMedia(false);
    const th = await loadThemeModule();

    const style = document.createElement('style');
    style.setAttribute('data-test', 'app-css');
    style.textContent = appCss;
    document.head.appendChild(style);

    const comfortable = [...SPACING_TOKENS, ...TYPE_TOKENS].map(token);
    expect(comfortable).toContain('16px'); // --sp-4 ships at 16px

    th.density.set('compact');
    const compact = [...SPACING_TOKENS, ...TYPE_TOKENS].map(token);

    for (const [i, name] of [...SPACING_TOKENS, ...TYPE_TOKENS].entries()) {
      const before = parseFloat(comfortable[i]);
      const after = parseFloat(compact[i]);
      expect(after, `${name} must shrink in compact density`).toBeLessThan(before);
      expect(after).toBeGreaterThan(0);
    }

    // The OS-appearance path and the density toggle must not influence each other.
    th.theme.set('light');
    expect(paintedTheme()).toBe('light');
    expect(token('--sp-4')).toBe(compact[3]);
  });
});

describe('pre-paint bootstrap in app.html (#680)', () => {
  it('resolves a stored system preference through the OS before first paint', () => {
    installMatchMedia(true);
    localStorage.setItem('presencejam:theme', 'system');

    runPrePaintBootstrap();
    expect(paintedTheme()).toBe('light');
    // `system` is a preference, never a painted attribute value.
    expect(paintedTheme()).not.toBe('system');
  });

  it('paints the stored density so a compact user gets no comfortable flash', () => {
    installMatchMedia(false);
    localStorage.setItem('presencejam:density', 'compact');

    runPrePaintBootstrap();
    expect(paintedDensity()).toBe('compact');
  });

  it('still honours the detached `?theme=` override over the stored key', () => {
    installMatchMedia(true);
    localStorage.setItem('presencejam:theme', 'dark');
    window.history.replaceState({}, '', '/?theme=light');

    runPrePaintBootstrap();
    expect(paintedTheme()).toBe('light');
  });

  it('treats a `?theme=system` override as follow-the-OS, not as the stored key', () => {
    installMatchMedia(true);
    localStorage.setItem('presencejam:theme', 'dark');
    window.history.replaceState({}, '', '/?theme=system');

    runPrePaintBootstrap();
    expect(paintedTheme()).toBe('light');
  });
});

/**
 * The header toggle (`PageHeader`, used by Settings / Reconnect / LogViewer and
 * mirrored in `Dashboard`) must describe the *painted* theme. Derived from the
 * preference alone, `system` on a dark desktop rendered the same moon as an
 * explicit-light preference — wrong about both the current state and what the
 * click would do.
 */
function mountThemeToggle() {
  const { container } = render(PageHeader, { title: 'Settings', onBack: () => {} });
  return () => container.querySelector('.theme-btn')?.textContent?.trim() ?? '';
}

/** Flip the OS appearance the statically-imported store captured. */
async function setOsLight(value: boolean) {
  osMedia.mql.matches = value;
  for (const cb of [...osMedia.listeners]) cb();
  await tick();
}

describe('theme toggle glyph describes the painted theme (#680)', () => {
  it('shows the sun under system on a dark desktop, and the click yields light', async () => {
    themeStore.set('system');
    await tick();
    const toggleGlyph = mountThemeToggle();
    await tick();

    expect(paintedTheme()).toBe('dark');
    expect(toggleGlyph()).toBe('☀');

    toggleTheme();
    await tick();
    expect(get(themeStore)).toBe('light');
    expect(paintedTheme()).toBe('light');
    expect(toggleGlyph()).toBe('☾');
  });

  it('shows the moon under system on a light desktop, and the click yields dark', async () => {
    osMedia.mql.matches = true;
    themeStore.set('system');
    await tick();
    const toggleGlyph = mountThemeToggle();
    await tick();

    expect(paintedTheme()).toBe('light');
    expect(toggleGlyph()).toBe('☾');

    toggleTheme();
    await tick();
    expect(get(themeStore)).toBe('dark');
    expect(paintedTheme()).toBe('dark');
    expect(toggleGlyph()).toBe('☀');
  });

  it('follows the OS live while selected, glyph included', async () => {
    themeStore.set('system');
    const toggleGlyph = mountThemeToggle();
    await tick();
    expect(toggleGlyph()).toBe('☀');

    await setOsLight(true);
    expect(paintedTheme()).toBe('light');
    expect(toggleGlyph()).toBe('☾');

    await setOsLight(false);
    expect(toggleGlyph()).toBe('☀');
  });

  it('keeps an explicit dark or light glyph unchanged by an OS change', async () => {
    themeStore.set('dark');
    const toggleGlyph = mountThemeToggle();
    await tick();
    expect(toggleGlyph()).toBe('☀');
    await setOsLight(true);
    expect(toggleGlyph()).toBe('☀');

    themeStore.set('light');
    await tick();
    expect(toggleGlyph()).toBe('☾');
    await setOsLight(false);
    expect(toggleGlyph()).toBe('☾');
  });
});
