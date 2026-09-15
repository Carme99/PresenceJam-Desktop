import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light';

export const STORAGE_KEY = 'presencejam:theme';

function readInitial(): Theme {
  if (typeof window === 'undefined') return 'dark';
  const stored = window.localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark') return stored;
  // Respect OS preference on first run.
  const prefersLight = window.matchMedia?.('(prefers-color-scheme: light)').matches;
  return prefersLight ? 'light' : 'dark';
}

export const theme = writable<Theme>(readInitial());

if (typeof document !== 'undefined') {
  theme.subscribe((value) => {
    document.documentElement.setAttribute('data-theme', value);
    try {
      window.localStorage.setItem(STORAGE_KEY, value);
    } catch {
      // localStorage may be blocked; ignore.
    }
    window.dispatchEvent(new CustomEvent('presencejam:theme-changed', { detail: value }));
  });
}

// #423: cross-window convergence — every webview (main + detached Logs /
// Settings) owns an independent `theme` instance, so an already-open
// detached window never saw a toggle made in the main window. `storage`
// events fire in every OTHER same-origin webview on write, giving us
// convergence with no backend round-trip. The CustomEvent above stays as
// the same-window fan-out; this is the cross-window half.
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key !== STORAGE_KEY) return;
    if (e.newValue === 'light' || e.newValue === 'dark') theme.set(e.newValue);
  });
}

export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}
