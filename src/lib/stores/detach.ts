import { get, writable } from 'svelte/store';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/**
 * C7 multi-window detach — Logs and Settings can "Pop out" into separate
 * WebviewWindows and "Pop back in" again.
 *
 * The main window is the single source of truth for AppConfig/polling
 * state; detached windows render read-mostly views that invoke the same
 * app-global Tauri commands. This store tracks, main-window-side, which
 * panes are currently popped out so the Dashboard nav buttons can show a
 * detached badge (and focus instead of navigate) while a pane is out.
 *
 * Labels are stable (`logs-detached` / `settings-detached`) and mirrored
 * in `src-tauri/capabilities/detached.json`, which grants those two
 * windows their minimal permission set.
 */

export type DetachablePane = 'logs' | 'settings';

export const DETACHED_LABEL: Record<DetachablePane, string> = {
  logs: 'logs-detached',
  settings: 'settings-detached'
};

const PANE_TITLE: Record<DetachablePane, string> = {
  logs: 'PresenceJam — Logs',
  settings: 'PresenceJam — Settings'
};

const PANE_SIZE: Record<DetachablePane, { width: number; height: number }> = {
  logs: { width: 720, height: 520 },
  settings: { width: 620, height: 720 }
};

// Main-window-only view state (currentView stays main-window-only; the
// main window never shows Logs/Settings content while they are popped out).
export const detachedPanes = writable<Record<DetachablePane, boolean>>({
  logs: false,
  settings: false
});

function markDetached(pane: DetachablePane, value: boolean) {
  detachedPanes.update((m) => ({ ...m, [pane]: value }));
}

/**
 * Open `pane` in its own window. Idempotent: if the pane is already out
 * (or the window survived a main-window reload and the store lost track),
 * focus the existing child window instead of creating a duplicate.
 */
export async function popOut(pane: DetachablePane): Promise<void> {
  const label = DETACHED_LABEL[pane];
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.setFocus();
    } catch (e) {
      console.warn(`[DETACH] focus ${label} failed:`, e);
    }
    markDetached(pane, true);
    return;
  }

  const size = PANE_SIZE[pane];
  const win = new WebviewWindow(label, {
    url: `/detached/${pane}`,
    title: PANE_TITLE[pane],
    width: size.width,
    height: size.height,
    minWidth: 400,
    minHeight: 400,
    center: true
  });

  // Optimistically mark as detached; both callbacks restore the badge
  // if creation fails or the user closes the child from its title bar.
  markDetached(pane, true);
  win.once('tauri://destroyed', () => markDetached(pane, false));
  win.once('tauri://error', (e) => {
    console.warn(`[DETACH] failed to open ${label}:`, e);
    markDetached(pane, false);
  });
}

/** Close the detached window for `pane` (the "Pop back in" action). */
export async function popIn(pane: DetachablePane): Promise<void> {
  const label = DETACHED_LABEL[pane];
  const win = await WebviewWindow.getByLabel(label);
  if (win) {
    try {
      await win.close(); // 'tauri://destroyed' clears the store entry.
    } catch (e) {
      console.warn(`[DETACH] close ${label} failed:`, e);
      markDetached(pane, false);
    }
  } else {
    markDetached(pane, false);
  }
}

/** Focus an already-detached pane's window (Dashboard nav click). */
export async function focusDetached(pane: DetachablePane): Promise<void> {
  const win = await WebviewWindow.getByLabel(DETACHED_LABEL[pane]);
  if (win) {
    try {
      await win.setFocus();
    } catch (e) {
      console.warn('[DETACH] focusDetached failed:', e);
    }
  }
}
