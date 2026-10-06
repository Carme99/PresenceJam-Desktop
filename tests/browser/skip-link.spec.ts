import { expect, test, type Page } from '@playwright/test';

/**
 * #739 — focus really does land on the view heading at load — plus #742, which
 * moved the skip link's target down off `.app-container` onto each view's body
 * (the Dashboard's `<main>`, the other views' first region below their header
 * bar). This spec pins the post-move contract: the link is the first
 * focusable, activating it moves focus onto the view body rather than the
 * wrapper, the next Tab reaches a body control rather than a header button,
 * and the header bar itself never owns the target id.
 *
 * Fails pre-fix: the target still sits on `.app-container`, so the body
 * assertions below see the wrapper instead — focus lands above the header
 * and the next Tab reaches header chrome.
 */
async function openDashboard(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('presencejam:locale', 'en');

    let callbackId = 0;
    const callbacks = new Map<number, (...args: unknown[]) => void>();
    const internals = {
      metadata: { currentWindow: { label: 'main' } },
      transformCallback(callback: (...args: unknown[]) => void): number {
        const id = ++callbackId;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(command: string): Promise<unknown> {
        if (command === 'plugin:event|listen') return callbackId;
        if (command === 'plugin:event|unlisten') return null;
        if (command === 'is_onboarding_complete') return true;
        if (command === 'get_sync_status') {
          return {
            is_syncing: true,
            current_track: {
              id: 'track-1',
              title: 'A Track',
              artist: 'An Artist',
              album: 'An Album',
              album_art_url: null,
              duration_ms: 200000,
              progress_ms: 1000,
              is_playing: true
            },
            spotify_connected: true,
            teams_connected: true
          };
        }
        if (command === 'get_presence_history') return [];
        if (command === 'load_manual_status_command') return null;
        return null;
      }
    };
    Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
      configurable: true,
      value: { unregisterListener: () => undefined }
    });
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      value: internals
    });
  });

  await page.goto('/');
  await expect(page.locator('.dashboard header')).toBeVisible();
  await expect(page.locator('.track-card')).toBeVisible();
}

interface FocusReport {
  id: string;
  tagName: string;
  inHeader: boolean;
  inMainContent: boolean;
  isBody: boolean;
}

async function activeElement(page: Page): Promise<FocusReport> {
  return page.evaluate(() => {
    const active = document.activeElement;
    return {
      id: active?.id ?? '',
      tagName: active?.tagName ?? '',
      inHeader: Boolean(active?.closest('header')),
      inMainContent: Boolean(active?.closest('#main-content')),
      isBody: active === document.body
    };
  });
}

test('activating the skip link moves focus to its target and keeps it keyboard-reachable', async ({ page }) => {
  await openDashboard(page);

  // #739 puts focus on the view's heading at load, so the skip link — the
  // first focusable in the document — is one Shift+Tab back from there.
  await expect.poll(async () => (await activeElement(page)).tagName).toBe('H1');
  await page.keyboard.press('Shift+Tab');
  const onSkipLink = await page.evaluate(() =>
    Boolean(document.activeElement?.classList.contains('skip-link'))
  );
  expect(onSkipLink, 'the skip link is not the first focusable in the document').toBe(true);

  // Activating it is a fragment navigation, which is what moves focus.
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await activeElement(page)).id).toBe('main-content');
  // #742: the target is the Dashboard's `<main>` — below the header bar, not
  // the `.app-container` wrapper that contains it. `closest('header')` is
  // false (the target sits outside the header) and `closest('#main-content')`
  // is true for the target itself, which pins "below the header" in the DOM
  // rather than just "not in a header".
  await expect.poll(async () => page.evaluate(() => document.activeElement?.tagName)).toBe('MAIN');
  expect(await activeElement(page)).toMatchObject({ inHeader: false, inMainContent: true, isBody: false });
  expect(await page.evaluate(() => document.querySelector('.app-container')?.getAttribute('id'))).not.toBe('main-content');

  // One Tab from the body target reaches a body control — the track card's
  // play/pause-adjacent controls or the manual-status composer — never a
  // header button (theme, logs, diagnostics, settings, about, sync toggle).
  await page.keyboard.press('Tab');
  const afterTab = await activeElement(page);
  expect(afterTab.inHeader, 'the first Tab after the skip link must leave the header behind').toBe(false);
  expect(afterTab.inMainContent, 'the first Tab after the skip link must stay in the view body').toBe(true);
});
