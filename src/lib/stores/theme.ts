import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'presencejam:theme';

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

export function toggleTheme() {
  theme.update((t) => (t === 'dark' ? 'light' : 'dark'));
}
