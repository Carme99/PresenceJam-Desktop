import { get, writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
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
 * Labels are stable (`logs-detached` / `settings-detached`) and mirrored in
 * `src-tauri/capabilities/detached.json` (which grants those two windows their
 * minimal permission set) and in `lib.rs`'s `detached_pane_spec`.
 *
 * Issue #922: the windows are built Rust-side by the `detach_pane` command —
 * label, in-app URL and size come from that table — so the main window needs
 * no `core:webview:allow-create-webview-window` grant.
 */

export type DetachablePane = 'logs' | 'settings';

export const DETACHED_LABEL: Record<DetachablePane, string> = {
  logs: 'logs-detached',
  settings: 'settings-detached'
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
const popOutInFlight: Record<DetachablePane, boolean> = { logs: false, settings: false };

export async function popOut(pane: DetachablePane): Promise<void> {
  // #review-9: in-flight guard — two rapid popOut clicks must not
  // double-create the child window (both would pass getByLabel null).
  // Static pane-keyed lookup (Record, not Set — two fixed keys).
  if (popOutInFlight[pane]) return;
  popOutInFlight[pane] = true;
  try {
    await popOutInner(pane);
  } finally {
    popOutInFlight[pane] = false;
  }
}

async function popOutInner(pane: DetachablePane): Promise<void> {
  const label = DETACHED_LABEL[pane];
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    try {
      await existing.setFocus();
      markDetached(pane, true);
      return;
    } catch (e) {
      // #422: stale/zombie label — the window handle exists but focus
      // rejects. Clear the flag and fall through to re-creation so the
      // badge never claims a detached window that cannot be focused.
      console.warn(`[DETACH] focus ${label} failed (stale handle, recreating):`, e);
      markDetached(pane, false);
    }
  }

  // Issue #433: carry the current theme so the child's pre-paint snippet
  // (app.html) applies it before first paint — no flash of the wrong theme
  // even before the storage event converges. Read by literal key (no store
  // import — this module never imports it).
  let theme: 'dark' | 'light' | null = null;
  try {
    const stored = window.localStorage.getItem('presencejam:theme');
    if (stored === 'dark' || stored === 'light') theme = stored;
  } catch {
    // localStorage may be blocked; the child falls back to OS preference.
  }

  // Issue #922: the window itself is built by the `detach_pane` command, from
  // a fixed table of labels, in-app URLs and sizes. Creating it here would
  // need `core:webview:allow-create-webview-window`, a grant with no URL
  // scope: any script in this window could then raise a chromed window on an
  // arbitrary origin.
  try {
    await invoke('detach_pane', { pane, theme });
  } catch (e) {
    // A refused creation is not a detached pane: clear the badge and stop
    // (the pre-#922 `tauri://error` path).
    console.warn(`[DETACH] failed to open ${label}:`, e);
    markDetached(pane, false);
    return;
  }

  // The command returns no handle, so the window object is re-read to bind the
  // listener that clears the badge when the child is closed from its title bar.
  const win = await WebviewWindow.getByLabel(label);
  if (!win) {
    // Created and already gone (or never created): the badge must not claim a
    // window that is not on screen.
    markDetached(pane, false);
    return;
  }
  markDetached(pane, true);
  win.once('tauri://destroyed', () => markDetached(pane, false));
}

/** Close the detached window for `pane` (the "Pop back in" action). */
export async function popIn(pane: DetachablePane): Promise<void> {
  const label = DETACHED_LABEL[pane];
  const win = await WebviewWindow.getByLabel(label);
  if (!win) {
    markDetached(pane, false);
    return;
  }
  try {
    await win.close(); // 'tauri://destroyed' clears the store entry.
  } catch (e) {
    // #594: a refused close must never clear the badge — the detached
    // window is still on screen, and clearing it made the Dashboard claim
    // the pane was back in while the user was looking straight at it.
    // Re-derive the whole map from the live window set instead: whether a
    // window exists decides the flag, not whether the close call resolved.
    console.warn(`[DETACH] close ${label} failed:`, e);
    await reconcileDetachedPanes();
  }
}

/** Focus an already-detached pane's window (Dashboard nav click). */
export async function focusDetached(pane: DetachablePane): Promise<void> {
  const win = await WebviewWindow.getByLabel(DETACHED_LABEL[pane]);
  if (win) {
    try {
      await win.setFocus();
    } catch (e) {
      // #422: zombie label with no focusable window — clear the badge so
      // the Dashboard stops offering focus-instead-of-navigate.
      console.warn('[DETACH] focusDetached failed (clearing stale flag):', e);
      markDetached(pane, false);
    }
  } else {
    // #422: no window behind the flag — clear it so the badge clears.
    markDetached(pane, false);
  }
}

/**
 * Derive the badge map from the live window set. `WebviewWindow.getByLabel`
 * returns a handle only while the label still has a webview, so the probe is
 * the same call `popOut`/`focusDetached` already make.
 *
 * #601: called once at main-window boot because the map starts empty on
 * every load while detached windows survive a main-window reload (HMR,
 * manual reload, crash recovery) — without it the Dashboard offers
 * "navigate" for a pane that is actually popped out, so the main window
 * renders a second live copy of it. Also called when "Pop back in" is
 * refused (#594), where the badge must keep matching what is on screen.
 *
 * The probe result replaces the map wholesale — it is the whole truth about
 * which windows exist. A probe that rejects leaves that pane unclaimed, the
 * same default the store starts from.
 */
export async function reconcileDetachedPanes(): Promise<void> {
  const panes = Object.keys(DETACHED_LABEL) as DetachablePane[];
  const probed = await Promise.all(
    panes.map(async (pane) => {
      try {
        return [pane, (await WebviewWindow.getByLabel(DETACHED_LABEL[pane])) !== null] as const;
      } catch (e) {
        console.warn(`[DETACH] probe ${DETACHED_LABEL[pane]} failed:`, e);
        return [pane, false] as const;
      }
    })
  );
  detachedPanes.set(Object.fromEntries(probed) as Record<DetachablePane, boolean>);
}
