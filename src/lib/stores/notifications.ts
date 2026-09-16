import { get, writable } from 'svelte/store';

/**
 * Track-change notification opt-in (#549, finding UiCore#4), shared state
 * rather than a per-component localStorage read.
 *
 * Dashboard used to read the flag exactly once, in `onMount`, and never
 * again. With Settings popped out into its own window (C7) the main window
 * keeps Dashboard mounted while the detached Settings window flips the
 * flag, so enabling notifications appeared to do nothing until Dashboard
 * was reloaded. The theme store already establishes the pattern for
 * exactly this case (#423): a module-level store plus a `storage` listener
 * for the other webviews.
 *
 * Same-window consumers (Settings and Dashboard live in one JS context)
 * share this store instance directly, which is why there is no
 * `presencejam:theme-changed`-style CustomEvent here — it would have no
 * listener.
 */
export const NOTIFICATIONS_STORAGE_KEY = 'notificationsEnabled';

function readInitial(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY) === 'true';
  } catch {
    // localStorage may be blocked; the opt-in defaults to off.
    return false;
  }
}

export const notificationsEnabled = writable<boolean>(readInitial());

/**
 * Persist the opt-in and fan it out to this webview. Other webviews pick it
 * up through the `storage` listener below — that is the detached-Settings
 * case, with no IPC round-trip.
 */
export function setNotificationsEnabled(enabled: boolean): void {
  try {
    window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, String(enabled));
  } catch {
    // Best-effort persistence; the in-memory store still reflects the choice.
  }
  if (get(notificationsEnabled) !== enabled) notificationsEnabled.set(enabled);
}

// #423 pattern: `storage` fires in every OTHER same-origin webview on write.
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key !== NOTIFICATIONS_STORAGE_KEY) return;
    const next = e.newValue === 'true';
    if (get(notificationsEnabled) !== next) notificationsEnabled.set(next);
  });
}
