import { get, writable } from 'svelte/store';

/** Stored *preference*; `system` follows the OS appearance live (#680). */
export type Theme = 'dark' | 'light' | 'system';
/** What is actually painted on `<html>` — always concrete, never `system`. */
export type ResolvedTheme = 'dark' | 'light';
/** Spacing/type scale density; `comfortable` is the shipped default (#680). */
export type Density = 'comfortable' | 'compact';

export const STORAGE_KEY = 'presencejam:theme';
export const DENSITY_STORAGE_KEY = 'presencejam:density';

const LIGHT_QUERY = '(prefers-color-scheme: light)';

// Captured once: resolution and the #680 `change` listener must observe the
// same MediaQueryList.
const lightQuery = typeof window === 'undefined' ? undefined : window.matchMedia?.(LIGHT_QUERY);

/** Resolve a preference to the theme painted on `<html>`. */
export function resolveTheme(pref: Theme): ResolvedTheme {
  if (pref !== 'system') return pref;
  return lightQuery?.matches === true ? 'light' : 'dark';
}

function readInitial(): Theme {
  if (typeof window === 'undefined') return 'dark';
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  // Respect OS preference on first run (pinned to the concrete value it saw).
  return lightQuery?.matches === true ? 'light' : 'dark';
}

const storedDensity =
  typeof window === 'undefined' ? null : window.localStorage.getItem(DENSITY_STORAGE_KEY);

const initialTheme = readInitial();

export const theme = writable<Theme>(initialTheme);
/**
 * What is painted on `<html>` right now — `system` resolved to the OS value,
 * and republished whenever the OS appearance changes. Components that render
 * from the theme (the header toggle glyph) must read this, not `theme`: the
 * preference alone cannot say what the user is looking at (#680).
 */
export const appliedTheme = writable<ResolvedTheme>(resolveTheme(initialTheme));
export const density = writable<Density>(storedDensity === 'compact' ? 'compact' : 'comfortable');

/** Paint `<html>` and publish the resolved value to `appliedTheme`. */
function paint(value: Theme): void {
  const applied = resolveTheme(value);
  document.documentElement.setAttribute('data-theme', applied);
  appliedTheme.set(applied);
}

if (typeof document !== 'undefined') {
  theme.subscribe((value) => {
    paint(value);
    try {
      window.localStorage.setItem(STORAGE_KEY, value);
    } catch {
      // localStorage may be blocked; ignore.
    }
    window.dispatchEvent(new CustomEvent('presencejam:theme-changed', { detail: value }));
  });
}

// #680: density is a token-scale override on `<html>` (`[data-density="compact"]`
// in app.css). Painted independent of `data-theme`, so the OS-appearance path
// and this toggle never fight each other.
if (typeof document !== 'undefined') {
  density.subscribe((value) => {
    document.documentElement.setAttribute('data-density', value);
    try {
      window.localStorage.setItem(DENSITY_STORAGE_KEY, value);
    } catch {
      // localStorage may be blocked; ignore.
    }
  });
}

// #680: `readInitial()` used to sample the OS preference exactly once, so a
// `system` preference froze at the value seen at launch — switching the OS
// appearance did nothing until the next start. Follow the media query from now
// on, but only while the preference is `system`: an explicit light/dark is a
// user decision and an OS change must not override it.
if (typeof document !== 'undefined') {
  lightQuery?.addEventListener?.('change', () => {
    if (get(theme) !== 'system') return;
    if (get(appliedTheme) === resolveTheme('system')) return;
    paint('system');
    window.dispatchEvent(new CustomEvent('presencejam:theme-changed', { detail: 'system' }));
  });
}

// #423: cross-window convergence — every webview (main + detached Logs /
// Settings) owns an independent `theme` instance, so an already-open
// detached window never saw a toggle made in the main window. `storage`
// events fire in every OTHER same-origin webview on write, giving us
// convergence with no backend round-trip. The CustomEvent above stays as
// the same-window fan-out; this is the cross-window half. Density rides the
// same mechanism (#680) so a detached pane follows it too.
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key === STORAGE_KEY) {
      if (e.newValue !== 'light' && e.newValue !== 'dark' && e.newValue !== 'system') return;
      // #review-6: same-value guard — set() notifies subscribers even when
      // unchanged; skip the write churn + data-theme DOM flip when already there.
      if (get(theme) === e.newValue) return;
      theme.set(e.newValue);
      return;
    }
    if (e.key === DENSITY_STORAGE_KEY) {
      if (e.newValue !== 'compact' && e.newValue !== 'comfortable') return;
      if (get(density) === e.newValue) return;
      density.set(e.newValue);
    }
  });
}

// #680: flip relative to the *painted* theme, so the header toggle still does
// something visible while the preference is `system`.
export function toggleTheme() {
  theme.update((pref) => (resolveTheme(pref) === 'dark' ? 'light' : 'dark'));
}
